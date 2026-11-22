#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use flate2::{write::GzEncoder, Compression as GzCompression};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};

use crate::regions::RegionSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TilePayload {
    pub z: u8,
    pub x: u32,
    pub y: u32,
    pub tile_id: u64,
    pub gzipped_mvt: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirEntry {
    tile_id: u64,
    offset: u64,
    length: u64,
    run_length: u64,
}

fn rotate(n: i64, x: i64, y: i64, rx: i64, ry: i64) -> (i64, i64) {
    if ry == 0 {
        if rx != 0 {
            return (n - 1 - y, n - 1 - x);
        }
        return (y, x);
    }
    (x, y)
}

/// Converts `(z, x, y)` to a PMTiles v3 Hilbert `tile_id` matching the official TypeScript
/// `zxyToTileId` implementation in `pmtiles`.
pub fn zxy_to_tile_id(z: u8, x: u32, y: u32) -> Result<u64, String> {
    if z > 26 {
        return Err("Tile zoom level exceeds max safe limit (26)".to_string());
    }
    let n = 1u64 << z;
    if u64::from(x) >= n || u64::from(y) >= n {
        return Err(format!("Tile ({z}, {x}, {y}) outside zoom bounds"));
    }
    let mut acc: u64 = ((1u64 << (2 * u32::from(z))) - 1) / 3;
    let mut tx = i64::from(x);
    let mut ty = i64::from(y);
    if z > 0 {
        let mut a = i32::from(z) - 1;
        while a >= 0 {
            let s = 1i64 << a;
            let rx = tx & s;
            let ry = ty & s;
            acc = acc.wrapping_add((((3 * rx) ^ ry) * s) as u64);
            let (nx, ny) = rotate(s, tx, ty, rx, ry);
            tx = nx;
            ty = ny;
            a -= 1;
        }
    }
    Ok(acc)
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(((value & 0x7f) as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn gzip_bytes(raw: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = GzEncoder::new(Vec::new(), GzCompression::default());
    encoder.write_all(raw).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}

fn serialize_directory(entries: &[DirEntry]) -> Result<Vec<u8>, String> {
    let mut buf = Vec::with_capacity(entries.len() * 6 + 8);
    write_varint(&mut buf, entries.len() as u64);

    let mut last_id = 0u64;
    for e in entries {
        write_varint(&mut buf, e.tile_id.saturating_sub(last_id));
        last_id = e.tile_id;
    }
    for e in entries {
        write_varint(&mut buf, e.run_length);
    }
    for e in entries {
        write_varint(&mut buf, e.length);
    }
    for (i, e) in entries.iter().enumerate() {
        if i > 0 && e.offset == entries[i - 1].offset + entries[i - 1].length {
            write_varint(&mut buf, 0);
        } else {
            write_varint(&mut buf, e.offset + 1);
        }
    }
    gzip_bytes(&buf)
}

struct PmTilesHeaderParams<'a> {
    root_len: u64,
    meta_offset: u64,
    meta_len: u64,
    leaf_offset: u64,
    leaf_len: u64,
    tile_data_offset: u64,
    tile_data_len: u64,
    num_addressed: u64,
    num_entries: u64,
    num_contents: u64,
    min_zoom: u8,
    max_zoom: u8,
    region: &'a RegionSpec,
}

fn write_header(out: &mut Vec<u8>, h: &PmTilesHeaderParams<'_>) {
    out.extend_from_slice(b"PMTiles");
    out.push(3); // specVersion = 3
    out.extend_from_slice(&127u64.to_le_bytes()); // rootDirectoryOffset
    out.extend_from_slice(&h.root_len.to_le_bytes());
    out.extend_from_slice(&h.meta_offset.to_le_bytes());
    out.extend_from_slice(&h.meta_len.to_le_bytes());
    out.extend_from_slice(&h.leaf_offset.to_le_bytes());
    out.extend_from_slice(&h.leaf_len.to_le_bytes());
    out.extend_from_slice(&h.tile_data_offset.to_le_bytes());
    out.extend_from_slice(&h.tile_data_len.to_le_bytes());
    out.extend_from_slice(&h.num_addressed.to_le_bytes());
    out.extend_from_slice(&h.num_entries.to_le_bytes());
    out.extend_from_slice(&h.num_contents.to_le_bytes());
    out.push(1); // clustered = true
    out.push(2); // internalCompression = Gzip
    out.push(2); // tileCompression = Gzip
    out.push(1); // tileType = MVT
    out.push(h.min_zoom);
    out.push(h.max_zoom);

    let scale = |v: f64| -> [u8; 4] { ((v * 10_000_000.0).round() as i32).to_le_bytes() };
    let (cx, cy) = h.region.bbox.center();
    out.extend_from_slice(&scale(h.region.bbox.min_lon));
    out.extend_from_slice(&scale(h.region.bbox.min_lat));
    out.extend_from_slice(&scale(h.region.bbox.max_lon));
    out.extend_from_slice(&scale(h.region.bbox.max_lat));
    out.push(h.region.center_zoom);
    out.extend_from_slice(&scale(cx));
    out.extend_from_slice(&scale(cy));
    debug_assert_eq!(out.len(), 127);
}

/// Builds a complete clustered PMTiles v3 binary archive (matching `pmtiles` v3 JS reader
/// byte offsets `0..127` and automatic root/leaf splitting to stay under the 16 KiB header fetch).
pub fn build_pmtiles_v3_archive(
    tiles: Vec<TilePayload>,
    min_zoom: u8,
    max_zoom: u8,
    region: &RegionSpec,
) -> Result<Vec<u8>, String> {
    let mut sorted = tiles;
    sorted.sort_by_key(|t| t.tile_id);

    let mut tile_data = Vec::new();
    let mut entries: Vec<DirEntry> = Vec::with_capacity(sorted.len());
    let mut dedup: HashMap<[u8; 32], (u64, u64)> = HashMap::new();

    for tile in sorted {
        let digest: [u8; 32] = Sha256::digest(&tile.gzipped_mvt).into();
        let (offset, length) = if let Some(&(off, len)) = dedup.get(&digest) {
            (off, len)
        } else {
            let off = tile_data.len() as u64;
            let len = tile.gzipped_mvt.len() as u64;
            tile_data.extend_from_slice(&tile.gzipped_mvt);
            dedup.insert(digest, (off, len));
            (off, len)
        };

        if let Some(last) = entries.last_mut() {
            if last.tile_id + last.run_length == tile.tile_id && last.offset == offset {
                last.run_length += 1;
                continue;
            }
        }
        entries.push(DirEntry {
            tile_id: tile.tile_id,
            offset,
            length,
            run_length: 1,
        });
    }

    let num_addressed: u64 = entries.iter().map(|e| e.run_length).sum();
    let num_entries = entries.len() as u64;
    let num_contents = dedup.len() as u64;

    let metadata_json = serde_json::json!({
        "name": format!("PARADIXE SOFT — {}", region.name),
        "description": "Corredores viales verificados, nodos y distritos territoriales (TakumiSoft).",
        "format": "pbf",
        "type": "overlay",
        "attribution": "OpenStreetMap contributors, Copernicus DEM, Open-Meteo",
        "vector_layers": [
            { "id": "corridors", "description": "Tramos viales territoriales" },
            { "id": "nodes", "description": "Nodos viales, centros poblados y hospitales" },
            { "id": "districts", "description": "Unidades territoriales y distritos" }
        ]
    });
    let metadata_gz = gzip_bytes(metadata_json.to_string().as_bytes())?;

    let direct_root = serialize_directory(&entries)?;
    let (root_gz, leaf_bytes) = if 127 + direct_root.len() <= 16_384 {
        (direct_root, Vec::new())
    } else {
        let mut leaf_blob = Vec::new();
        let mut root_entries = Vec::new();
        for chunk in entries.chunks(4096) {
            let leaf_gz = serialize_directory(chunk)?;
            root_entries.push(DirEntry {
                tile_id: chunk[0].tile_id,
                offset: leaf_blob.len() as u64,
                length: leaf_gz.len() as u64,
                run_length: 0,
            });
            leaf_blob.extend_from_slice(&leaf_gz);
        }
        let root = serialize_directory(&root_entries)?;
        if 127 + root.len() > 16_384 {
            return Err("Root directory exceeds 16 KiB limit".to_string());
        }
        (root, leaf_blob)
    };

    let root_len = root_gz.len() as u64;
    let meta_offset = 127 + root_len;
    let meta_len = metadata_gz.len() as u64;
    let leaf_offset = meta_offset + meta_len;
    let leaf_len = leaf_bytes.len() as u64;
    let tile_data_offset = leaf_offset + leaf_len;
    let tile_data_len = tile_data.len() as u64;

    let mut archive = Vec::with_capacity((tile_data_offset + tile_data_len) as usize);
    write_header(
        &mut archive,
        &PmTilesHeaderParams {
            root_len,
            meta_offset,
            meta_len,
            leaf_offset,
            leaf_len,
            tile_data_offset,
            tile_data_len,
            num_addressed,
            num_entries,
            num_contents,
            min_zoom,
            max_zoom,
            region,
        },
    );
    archive.extend_from_slice(&root_gz);
    archive.extend_from_slice(&metadata_gz);
    archive.extend_from_slice(&leaf_bytes);
    archive.extend_from_slice(&tile_data);
    Ok(archive)
}

fn lon_lat_to_tile_xy(lon: f64, lat: f64, zoom: u8) -> (u32, u32) {
    let n = (1u64 << zoom) as f64;
    let x_f = ((lon + 180.0) / 360.0) * n;
    let lat_rad = lat.clamp(-85.0511, 85.0511).to_radians();
    let y_f =
        (1.0 - ((lat_rad.tan() + (1.0 / lat_rad.cos())).ln() / std::f64::consts::PI)) / 2.0 * n;
    let max_idx = (1u32 << zoom) - 1;
    let x = (x_f.floor() as i64).clamp(0, i64::from(max_idx)) as u32;
    let y = (y_f.floor() as i64).clamp(0, i64::from(max_idx)) as u32;
    (x, y)
}

/// Queries PostGIS 3.4 `ST_AsMVT` vector tiles across `min_zoom..=max_zoom` intersecting
/// `region.bbox`, packages them into a valid PMTiles v3 file at `out_path`, and returns bytes.
pub async fn build_region_pmtiles_from_postgis(
    pool: &PgPool,
    region: &RegionSpec,
    min_zoom: u8,
    max_zoom: u8,
    out_path: &Path,
) -> Result<(Vec<u8>, usize), String> {
    if min_zoom > max_zoom || max_zoom > 14 {
        return Err("Zoom range must satisfy min_zoom <= max_zoom <= 14".to_string());
    }

    let mut tiles = Vec::new();
    for z in min_zoom..=max_zoom {
        let (x_min, y_min) = lon_lat_to_tile_xy(region.bbox.min_lon, region.bbox.max_lat, z);
        let (x_max, y_max) = lon_lat_to_tile_xy(region.bbox.max_lon, region.bbox.min_lat, z);

        for x in x_min..=x_max {
            for y in y_min..=y_max {
                let row = sqlx::query(
                    "WITH env AS (
                        SELECT ST_TileEnvelope($1::int, $2::int, $3::int) AS geom,
                               ST_Transform(ST_TileEnvelope($1::int, $2::int, $3::int), 4326) AS wgs
                     ),
                     corridors_mvt AS (
                        SELECT ST_AsMVT(q, 'corridors', 4096, 'geom') AS mvt
                        FROM (
                            SELECT
                                rs.segment_id::text AS segment_id,
                                rs.corridor_name,
                                rs.surface_type,
                                rs.direction_type,
                                rs.length_km::float8 AS length_km,
                                rs.max_slope_forward_pct::float8 AS max_slope_pct,
                                COALESCE(active_inc.impact_passability, 'OPEN') AS passability_state,
                                latest_ivv.ivv_score::float8 AS ivv_score,
                                ST_AsMVTGeom(ST_Transform(rs.geom, 3857), env.geom, 4096, 64, true) AS geom
                            FROM road_segments rs
                            CROSS JOIN env
                            LEFT JOIN LATERAL (
                                SELECT impact_passability
                                FROM incidents i
                                WHERE i.segment_id = rs.segment_id
                                  AND i.verification_state IN ('SUSPECTED', 'CONFIRMED')
                                ORDER BY i.detected_at DESC
                                LIMIT 1
                            ) active_inc ON TRUE
                            LEFT JOIN LATERAL (
                                SELECT ia.ivv_score
                                FROM weather_observations wo
                                JOIN ivv_assessments ia ON ia.observation_id = wo.observation_id
                                WHERE wo.segment_id = rs.segment_id
                                ORDER BY wo.observed_at DESC, ia.calculated_at DESC
                                LIMIT 1
                            ) latest_ivv ON TRUE
                            WHERE rs.is_topology_verified = TRUE
                              AND rs.geom && env.wgs
                              AND ST_Intersects(rs.geom, env.wgs)
                        ) q
                        WHERE q.geom IS NOT NULL
                     ),
                     nodes_mvt AS (
                        SELECT ST_AsMVT(q, 'nodes', 4096, 'geom') AS mvt
                        FROM (
                            SELECT
                                rn.node_id::text AS node_id,
                                rn.name,
                                rn.node_type,
                                rn.elevation_m,
                                ST_AsMVTGeom(ST_Transform(rn.location_geom, 3857), env.geom, 4096, 64, true) AS geom
                            FROM road_nodes rn
                            CROSS JOIN env
                            WHERE rn.node_type <> 'INTERSECTION'
                              AND rn.location_geom && env.wgs
                              AND ST_Intersects(rn.location_geom, env.wgs)
                        ) q
                        WHERE q.geom IS NOT NULL
                     ),
                     districts_mvt AS (
                        SELECT ST_AsMVT(q, 'districts', 4096, 'geom') AS mvt
                        FROM (
                            SELECT
                                tu.ubigeo_code,
                                tu.name,
                                tu.admin_level,
                                ST_AsMVTGeom(ST_Transform(tu.centroid_geom, 3857), env.geom, 4096, 64, true) AS geom
                            FROM territorial_units tu
                            CROSS JOIN env
                            WHERE tu.centroid_geom && env.wgs
                              AND ST_Intersects(tu.centroid_geom, env.wgs)
                        ) q
                        WHERE q.geom IS NOT NULL
                     )
                     SELECT COALESCE((SELECT mvt FROM corridors_mvt), ''::bytea)
                         || COALESCE((SELECT mvt FROM nodes_mvt), ''::bytea)
                         || COALESCE((SELECT mvt FROM districts_mvt), ''::bytea) AS combined_mvt",
                )
                .bind(i32::from(z))
                .bind(i32::try_from(x).map_err(|e| e.to_string())?)
                .bind(i32::try_from(y).map_err(|e| e.to_string())?)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;

                let raw_mvt: Vec<u8> = row.try_get("combined_mvt").map_err(|e| e.to_string())?;
                if raw_mvt.is_empty() {
                    continue;
                }
                let gzipped_mvt = gzip_bytes(&raw_mvt)?;
                let tile_id = zxy_to_tile_id(z, x, y)?;
                tiles.push(TilePayload {
                    z,
                    x,
                    y,
                    tile_id,
                    gzipped_mvt,
                });
            }
        }
    }

    let tile_count = tiles.len();
    let archive = build_pmtiles_v3_archive(tiles, min_zoom, max_zoom, region)?;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(out_path, &archive).map_err(|e| e.to_string())?;

    Ok((archive, tile_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regions::resolve_region;
    use flate2::read::GzDecoder;
    use std::io::Read;

    #[test]
    fn hilbert_zxy_matches_pmtiles_spec_vectors() {
        assert_eq!(zxy_to_tile_id(0, 0, 0).unwrap(), 0);
        assert_eq!(zxy_to_tile_id(1, 0, 0).unwrap(), 1);
        assert_eq!(zxy_to_tile_id(1, 0, 1).unwrap(), 2);
        assert_eq!(zxy_to_tile_id(1, 1, 1).unwrap(), 3);
        assert_eq!(zxy_to_tile_id(1, 1, 0).unwrap(), 4);
    }

    #[test]
    fn archive_header_and_directory_roundtrip_matches_js_offsets() {
        let region = resolve_region("PE-HUC", None, None).unwrap();
        let tile = TilePayload {
            z: 8,
            x: 73,
            y: 134,
            tile_id: zxy_to_tile_id(8, 73, 134).unwrap(),
            gzipped_mvt: gzip_bytes(b"mock-mvt-payload").unwrap(),
        };
        let bytes = build_pmtiles_v3_archive(vec![tile], 8, 8, &region).unwrap();

        assert_eq!(&bytes[0..7], b"PMTiles");
        assert_eq!(bytes[7], 3);
        let root_off = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let root_len = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let meta_off = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        let meta_len = u64::from_le_bytes(bytes[32..40].try_into().unwrap());
        assert_eq!(root_off, 127);
        assert_eq!(meta_off, root_off + root_len);
        assert_eq!(bytes[96], 1); // clustered
        assert_eq!(bytes[97], 2); // internal gzip
        assert_eq!(bytes[98], 2); // tile gzip
        assert_eq!(bytes[99], 1); // MVT
        assert_eq!(bytes[100], 8);
        assert_eq!(bytes[101], 8);

        let mut gz = GzDecoder::new(&bytes[meta_off as usize..(meta_off + meta_len) as usize]);
        let mut meta_str = String::new();
        gz.read_to_string(&mut meta_str).unwrap();
        assert!(meta_str.contains("corridors"));
    }
}
