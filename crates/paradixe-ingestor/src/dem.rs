#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use flate2::read::GzDecoder;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::regions::RegionSpec;

const DEFAULT_SRTM_SKADI_BASE_URL: &str = "https://s3.amazonaws.com/elevation-tiles-prod/skadi";
const OPEN_METEO_ELEVATION_URL: &str = "https://api.open-meteo.com/v1/elevation";
const SRTM_VOID_I16: i16 = -32768;
const DB_CHUNK_SIZE: usize = 500;

#[derive(Debug, Deserialize)]
struct ElevationResponse {
    elevation: Vec<f64>,
}

/// Computes `SrtmTile` Skadi folder and tile basename (southwest 1°×1° corner) for `(lon, lat)`.
/// Example: `(-76.24, -9.93)` -> `("S10", "S10W077")`.
pub fn srtm_tile_key(lon: f64, lat: f64) -> (String, String) {
    let lat_floor = lat.floor() as i32;
    let lon_floor = lon.floor() as i32;
    let lat_hem = if lat_floor >= 0 { 'N' } else { 'S' };
    let lon_hem = if lon_floor >= 0 { 'E' } else { 'W' };
    let dir = format!("{lat_hem}{:02}", lat_floor.unsigned_abs());
    let tile = format!("{dir}{lon_hem}{:03}", lon_floor.unsigned_abs());
    (dir, tile)
}

/// In-memory decompressed 1°×1° NASA SRTM HGT grid (1 arcsec 3601×3601 or 3 arcsec 1201×1201).
#[derive(Debug, Clone)]
pub struct SrtmTile {
    raw: Vec<u8>,
    dim: usize,
    lat_floor: i32,
    lon_floor: i32,
}

impl SrtmTile {
    pub fn from_hgt_bytes(raw: Vec<u8>, lat_floor: i32, lon_floor: i32) -> Result<Self, String> {
        if raw.len() < 8 || !raw.len().is_multiple_of(2) {
            return Err(format!("Invalid HGT byte length: {}", raw.len()));
        }
        let samples = raw.len() / 2;
        let dim = (samples as f64).sqrt().round() as usize;
        if dim < 2 || dim * dim * 2 != raw.len() {
            return Err(format!(
                "Non-square HGT raster ({} bytes, inferred dim={dim})",
                raw.len()
            ));
        }
        Ok(Self {
            raw,
            dim,
            lat_floor,
            lon_floor,
        })
    }

    fn read_cell(&self, row: usize, col: usize) -> Option<i16> {
        let idx = (row * self.dim + col) * 2;
        let val = i16::from_be_bytes([self.raw[idx], self.raw[idx + 1]]);
        if val == SRTM_VOID_I16 {
            None
        } else {
            Some(val)
        }
    }

    /// Bilinear interpolation over valid HGT corners; skips SRTM `-32768` voids and falls back
    /// to a small nearest-pixel ring before returning `None`.
    pub fn sample(&self, lon: f64, lat: f64) -> Option<i32> {
        let frac_lat = (lat - f64::from(self.lat_floor)).clamp(0.0, 1.0);
        let frac_lon = (lon - f64::from(self.lon_floor)).clamp(0.0, 1.0);
        let max_idx = (self.dim - 1) as f64;

        // Row 0 is North edge (lat_floor + 1), row dim - 1 is South edge (lat_floor).
        let row_f = (1.0 - frac_lat) * max_idx;
        let col_f = frac_lon * max_idx;

        let r0 = (row_f.floor() as usize).min(self.dim - 2);
        let c0 = (col_f.floor() as usize).min(self.dim - 2);
        let dr = (row_f - r0 as f64).clamp(0.0, 1.0);
        let dc = (col_f - c0 as f64).clamp(0.0, 1.0);

        let corners = [
            (r0, c0, (1.0 - dr) * (1.0 - dc)),
            (r0, c0 + 1, (1.0 - dr) * dc),
            (r0 + 1, c0, dr * (1.0 - dc)),
            (r0 + 1, c0 + 1, dr * dc),
        ];

        let mut weighted_sum = 0.0f64;
        let mut weight_sum = 0.0f64;
        for (r, c, w) in corners {
            if let Some(elev) = self.read_cell(r, c) {
                weighted_sum += f64::from(elev) * w;
                weight_sum += w;
            }
        }

        if weight_sum > 0.0 {
            return Some((weighted_sum / weight_sum).round() as i32);
        }

        // Handle rare SRTM void pockets with a 3-pixel local neighborhood average.
        let mut ring_sum = 0i64;
        let mut ring_count = 0i64;
        let r_start = r0.saturating_sub(3);
        let r_end = (r0 + 4).min(self.dim - 1);
        let c_start = c0.saturating_sub(3);
        let c_end = (c0 + 4).min(self.dim - 1);
        for r in r_start..=r_end {
            for c in c_start..=c_end {
                if let Some(v) = self.read_cell(r, c) {
                    ring_sum += i64::from(v);
                    ring_count += 1;
                }
            }
        }
        if ring_count > 0 {
            Some(((ring_sum as f64) / (ring_count as f64)).round() as i32)
        } else {
            None
        }
    }
}

fn gunzip_bytes(compressed: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoder = GzDecoder::new(compressed);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("Failed decompressing SRTM .hgt.gz: {e}"))?;
    Ok(out)
}

struct SrtmCache {
    tiles: HashMap<String, Option<SrtmTile>>,
    cache_dir: PathBuf,
    base_url: String,
    client: reqwest::Client,
    downloaded_tiles: usize,
    disk_hits: usize,
}

impl SrtmCache {
    fn new(client: reqwest::Client) -> Self {
        let base_url = std::env::var("PARADIXE_SRTM_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_SRTM_SKADI_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let cache_dir = std::env::var("PARADIXE_DEM_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/raw/dem"));
        Self {
            tiles: HashMap::new(),
            cache_dir,
            base_url,
            client,
            downloaded_tiles: 0,
            disk_hits: 0,
        }
    }

    async fn load_or_download_tile(
        &mut self,
        dir: &str,
        tile_key: &str,
        lat_floor: i32,
        lon_floor: i32,
    ) -> Result<Option<SrtmTile>, String> {
        let tile_dir = self.cache_dir.join(dir);
        let tile_path = tile_dir.join(format!("{tile_key}.hgt.gz"));

        if tile_path.exists() {
            match tokio::fs::read(&tile_path).await {
                Ok(bytes) if !bytes.is_empty() => match gunzip_bytes(&bytes)
                    .and_then(|raw| SrtmTile::from_hgt_bytes(raw, lat_floor, lon_floor))
                {
                    Ok(tile) => {
                        self.disk_hits += 1;
                        return Ok(Some(tile));
                    }
                    Err(err) => {
                        tracing::warn!(
                            path = %tile_path.display(),
                            error = %err,
                            "Corrupt cached SRTM tile; re-downloading"
                        );
                    }
                },
                _ => {}
            }
        }

        let url = format!("{}/{}/{}.hgt.gz", self.base_url, dir, tile_key);
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let resp = self.client.get(&url).send().await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let bytes = r.bytes().await.map_err(|e| e.to_string())?;
                    let raw = gunzip_bytes(&bytes)?;
                    let tile = SrtmTile::from_hgt_bytes(raw, lat_floor, lon_floor)?;

                    tokio::fs::create_dir_all(&tile_dir)
                        .await
                        .map_err(|e| format!("Failed creating DEM cache dir: {e}"))?;
                    let tmp_path = tile_dir.join(format!("{tile_key}.hgt.gz.part"));
                    tokio::fs::write(&tmp_path, &bytes)
                        .await
                        .map_err(|e| format!("Failed writing DEM cache tile: {e}"))?;
                    tokio::fs::rename(&tmp_path, &tile_path)
                        .await
                        .map_err(|e| format!("Failed renaming DEM cache tile: {e}"))?;

                    self.downloaded_tiles += 1;
                    tracing::info!(tile = %tile_key, bytes = bytes.len(), "Cached NASA SRTM 30m tile");
                    return Ok(Some(tile));
                }
                Ok(r) if r.status().as_u16() == 404 || r.status().as_u16() == 403 => {
                    tracing::warn!(tile = %tile_key, status = %r.status(), "SRTM tile unavailable; skipping");
                    return Ok(None);
                }
                Ok(r)
                    if attempt < 3
                        && (r.status().as_u16() == 429 || r.status().is_server_error()) =>
                {
                    tokio::time::sleep(Duration::from_millis(800 * u64::from(attempt))).await;
                }
                Ok(r) => {
                    return Err(format!("SRTM download HTTP {} for {url}", r.status()));
                }
                Err(e) if attempt < 3 => {
                    tracing::warn!(tile = %tile_key, attempt, error = %e, "Retrying SRTM download");
                    tokio::time::sleep(Duration::from_millis(800 * u64::from(attempt))).await;
                }
                Err(e) => {
                    return Err(format!("SRTM download failed for {url}: {e}"));
                }
            }
        }
    }

    async fn elevation_m(&mut self, lon: f64, lat: f64) -> Result<Option<i32>, String> {
        let (dir, tile_key) = srtm_tile_key(lon, lat);
        if !self.tiles.contains_key(&tile_key) {
            let lat_floor = lat.floor() as i32;
            let lon_floor = lon.floor() as i32;
            let loaded = self
                .load_or_download_tile(&dir, &tile_key, lat_floor, lon_floor)
                .await?;
            self.tiles.insert(tile_key.clone(), loaded);
        }

        if let Some(Some(tile)) = self.tiles.get(&tile_key) {
            if let Some(elev) = tile.sample(lon, lat) {
                return Ok(Some(elev));
            }
        }

        Ok(None)
    }
}

async fn fallback_open_meteo_elevation(
    client: &reqwest::Client,
    lon: f64,
    lat: f64,
) -> Option<i32> {
    let resp = client
        .get(OPEN_METEO_ELEVATION_URL)
        .query(&[
            ("latitude", format!("{lat:.5}")),
            ("longitude", format!("{lon:.5}")),
        ])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: ElevationResponse = resp.json().await.ok()?;
    parsed.elevation.first().map(|v| v.round() as i32)
}

fn dec1(v: f64) -> Decimal {
    let scaled = (v.clamp(0.0, 999.9) * 10.0).round() as i64;
    Decimal::new(scaled, 1)
}

fn dec2(v: f64) -> Decimal {
    let scaled = (v.clamp(0.0, 1.0) * 100.0).round() as i64;
    Decimal::new(scaled, 2)
}

/// Computes `avg_slope_pct`, `max_slope_forward_pct`, and `max_slope_reverse_pct` from
/// an evenly spaced elevation profile along geodesic segment length `length_km`.
pub fn compute_profile_slopes(elevations_m: &[i32], length_km: f64) -> (f64, f64, f64) {
    if elevations_m.len() < 2 {
        return (0.0, 0.0, 0.0);
    }

    let total_m = (length_km * 1000.0).max(50.0);
    let first = f64::from(*elevations_m.first().unwrap_or(&0));
    let last = f64::from(*elevations_m.last().unwrap_or(&0));
    let avg_slope = (((last - first).abs() / total_m) * 100.0).min(80.0);

    let intervals = (elevations_m.len() - 1) as f64;
    let step_m = (total_m / intervals).max(25.0);
    let mut peak = avg_slope;
    for pair in elevations_m.windows(2) {
        let grade = ((f64::from(pair[1]) - f64::from(pair[0])).abs() / step_m) * 100.0;
        peak = peak.max(grade);
    }
    let peak = peak.min(80.0);

    let round1 = |v: f64| (v * 10.0).round() / 10.0;
    (round1(avg_slope), round1(peak), round1(peak))
}

/// Computes `avg_slope_pct`, `max_slope_forward_pct`, and `max_slope_reverse_pct` from
/// origin, midpoint, and destination elevations along geodesic segment length `length_km`.
pub fn compute_segment_slopes(
    elev_orig_m: i32,
    elev_mid_m: Option<i32>,
    elev_dest_m: i32,
    length_km: f64,
) -> (f64, f64, f64) {
    match elev_mid_m {
        Some(mid) => compute_profile_slopes(&[elev_orig_m, mid, elev_dest_m], length_km),
        None => compute_profile_slopes(&[elev_orig_m, elev_dest_m], length_km),
    }
}

/// Heuristic geotechnical susceptibility baseline prior to INGEMMET/MTC geological overlay.
pub fn heuristic_soil_susceptibility(max_slope_pct: f64, distance_to_ravine_m: i32) -> f64 {
    let slope_factor = (max_slope_pct / 45.0).clamp(0.0, 1.0);
    let ravine_factor = 1.0 - (f64::from(distance_to_ravine_m) / 500.0).clamp(0.0, 1.0);
    let idx = (0.20 + 0.50 * slope_factor + 0.30 * ravine_factor).clamp(0.10, 0.95);
    (idx * 100.0).round() / 100.0
}

/// Enriches `road_nodes.elevation_m` using NASA SRTM 30m tiles (AWS Open Data Skadi + local cache
/// in `data/raw/dem/`) and updates multi-sample segment slopes & geotechnical baseline in PostGIS.
pub async fn enrich_dem_elevations_and_slopes(
    pool: &PgPool,
    region: &RegionSpec,
    refresh_all: bool,
    max_nodes: Option<usize>,
) -> Result<(usize, usize), String> {
    let client = reqwest::Client::builder()
        .user_agent("PARADIXE-SOFT-Ingestor/0.1 (TakumiSoft)")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let mut srtm = SrtmCache::new(client.clone());
    let bbox_wkt = region.bbox.padded(0.05).polygon_wkt();
    let limit_clause = max_nodes.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let filter_clause = if refresh_all {
        ""
    } else {
        "AND rn.elevation_m = -9999"
    };

    let sql = format!(
        "SELECT rn.node_id, ST_X(rn.location_geom) AS lon, ST_Y(rn.location_geom) AS lat
         FROM road_nodes rn
         WHERE ST_Intersects(rn.location_geom, ST_GeomFromText($1, 4326))
           {filter_clause}
         ORDER BY rn.node_id
         {limit_clause}"
    );
    let node_rows = sqlx::query(&sql)
        .bind(&bbox_wkt)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut update_node_ids = Vec::with_capacity(node_rows.len());
    let mut update_node_elevs = Vec::with_capacity(node_rows.len());
    let mut fallback_count = 0usize;

    for r in node_rows {
        let id: Uuid = r.try_get("node_id").map_err(|e| e.to_string())?;
        let lon: f64 = r.try_get("lon").map_err(|e| e.to_string())?;
        let lat: f64 = r.try_get("lat").map_err(|e| e.to_string())?;

        let elev_opt = match srtm.elevation_m(lon, lat).await? {
            Some(e) => Some(e),
            None => {
                fallback_count += 1;
                fallback_open_meteo_elevation(&client, lon, lat).await
            }
        };

        if let Some(elev) = elev_opt {
            update_node_ids.push(id);
            update_node_elevs.push(elev.clamp(-500, 6800));
        }
    }

    let mut nodes_updated = 0usize;
    for (id_chunk, elev_chunk) in update_node_ids
        .chunks(DB_CHUNK_SIZE)
        .zip(update_node_elevs.chunks(DB_CHUNK_SIZE))
    {
        sqlx::query(
            "UPDATE road_nodes AS rn
             SET elevation_m = v.elev
             FROM UNNEST($1::uuid[], $2::int4[]) AS v(id, elev)
             WHERE rn.node_id = v.id",
        )
        .bind(id_chunk)
        .bind(elev_chunk)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        nodes_updated += id_chunk.len();
    }

    let seg_rows = sqlx::query(
        "SELECT
            rs.segment_id,
            rs.direction_type,
            rs.length_km::float8 AS length_km,
            rs.distance_to_ravine_m,
            o.elevation_m AS orig_elev,
            d.elevation_m AS dest_elev,
            ST_X(ST_LineInterpolatePoint(rs.geom, 0.25)) AS p25_lon,
            ST_Y(ST_LineInterpolatePoint(rs.geom, 0.25)) AS p25_lat,
            ST_X(ST_LineInterpolatePoint(rs.geom, 0.50)) AS p50_lon,
            ST_Y(ST_LineInterpolatePoint(rs.geom, 0.50)) AS p50_lat,
            ST_X(ST_LineInterpolatePoint(rs.geom, 0.75)) AS p75_lon,
            ST_Y(ST_LineInterpolatePoint(rs.geom, 0.75)) AS p75_lat
         FROM road_segments rs
         JOIN road_nodes o ON o.node_id = rs.origin_node_id
         JOIN road_nodes d ON d.node_id = rs.destination_node_id
         WHERE o.elevation_m <> -9999
           AND d.elevation_m <> -9999
           AND ST_Intersects(rs.geom, ST_GeomFromText($1, 4326))",
    )
    .bind(&bbox_wkt)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let n_segs = seg_rows.len();
    let mut seg_ids = Vec::with_capacity(n_segs);
    let mut avg_slopes = Vec::with_capacity(n_segs);
    let mut fwd_slopes = Vec::with_capacity(n_segs);
    let mut rev_slopes: Vec<Option<Decimal>> = Vec::with_capacity(n_segs);
    let mut soil_idxs = Vec::with_capacity(n_segs);
    let mut hotspots = Vec::with_capacity(n_segs);

    for r in seg_rows {
        let seg_id: Uuid = r.try_get("segment_id").map_err(|e| e.to_string())?;
        let dir_type: String = r.try_get("direction_type").map_err(|e| e.to_string())?;
        let length_km: f64 = r.try_get("length_km").map_err(|e| e.to_string())?;
        let dist_ravine: i32 = r
            .try_get("distance_to_ravine_m")
            .map_err(|e| e.to_string())?;
        let orig_elev: i32 = r.try_get("orig_elev").map_err(|e| e.to_string())?;
        let dest_elev: i32 = r.try_get("dest_elev").map_err(|e| e.to_string())?;

        let sample_coords = [
            (
                r.try_get::<f64, _>("p25_lon").map_err(|e| e.to_string())?,
                r.try_get::<f64, _>("p25_lat").map_err(|e| e.to_string())?,
            ),
            (
                r.try_get::<f64, _>("p50_lon").map_err(|e| e.to_string())?,
                r.try_get::<f64, _>("p50_lat").map_err(|e| e.to_string())?,
            ),
            (
                r.try_get::<f64, _>("p75_lon").map_err(|e| e.to_string())?,
                r.try_get::<f64, _>("p75_lat").map_err(|e| e.to_string())?,
            ),
        ];

        let mut profile = Vec::with_capacity(5);
        profile.push(orig_elev);
        for (lon, lat) in sample_coords {
            if let Some(elev) = srtm.elevation_m(lon, lat).await? {
                profile.push(elev.clamp(-500, 6800));
            }
        }
        profile.push(dest_elev);

        let (avg_s, fwd_s, rev_s) = compute_profile_slopes(&profile, length_km);
        let soil = heuristic_soil_susceptibility(fwd_s, dist_ravine);
        let hotspot = fwd_s >= 20.0 && dist_ravine <= 250;
        let rev_dec = if dir_type == "BIDIRECTIONAL" {
            Some(dec1(rev_s))
        } else {
            None
        };

        seg_ids.push(seg_id);
        avg_slopes.push(dec1(avg_s));
        fwd_slopes.push(dec1(fwd_s));
        rev_slopes.push(rev_dec);
        soil_idxs.push(dec2(soil));
        hotspots.push(hotspot);
    }

    let mut segments_updated = 0usize;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for start in (0..n_segs).step_by(DB_CHUNK_SIZE) {
        let end = (start + DB_CHUNK_SIZE).min(n_segs);
        sqlx::query(
            "UPDATE road_segments AS rs
             SET avg_slope_pct = v.avg_s,
                 max_slope_forward_pct = v.fwd_s,
                 max_slope_reverse_pct = v.rev_s,
                 soil_susceptibility_idx = v.soil,
                 is_historical_hotspot = v.hotspot
             FROM UNNEST(
                 $1::uuid[],
                 $2::numeric[],
                 $3::numeric[],
                 $4::numeric[],
                 $5::numeric[],
                 $6::bool[]
             ) AS v(id, avg_s, fwd_s, rev_s, soil, hotspot)
             WHERE rs.segment_id = v.id",
        )
        .bind(&seg_ids[start..end])
        .bind(&avg_slopes[start..end])
        .bind(&fwd_slopes[start..end])
        .bind(&rev_slopes[start..end])
        .bind(&soil_idxs[start..end])
        .bind(&hotspots[start..end])
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        segments_updated += end - start;
    }
    tx.commit().await.map_err(|e| e.to_string())?;

    tracing::info!(
        region = %region.iso_code,
        downloaded_tiles = srtm.downloaded_tiles,
        disk_hits = srtm.disk_hits,
        fallback_count,
        nodes_updated,
        segments_updated,
        "SRTM 30m DEM enrichment summary"
    );

    Ok((nodes_updated, segments_updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srtm_tile_key_matches_andean_and_northern_hemisphere() {
        assert_eq!(
            srtm_tile_key(-76.24, -9.93),
            ("S10".to_string(), "S10W077".to_string())
        );
        assert_eq!(
            srtm_tile_key(10.2, 45.8),
            ("N45".to_string(), "N45E010".to_string())
        );
    }

    #[test]
    fn srtm_bilinear_sampling_and_void_handling() {
        // 3x3 raster over [-10.0..-9.0] lat, [-77.0..-76.0] lon
        // Row 0 = north (-9.0): 1000, 1200, 1400
        // Row 1 = mid   (-9.5): 2000, VOID(-32768), 2400
        // Row 2 = south (-10.0): 3000, 3200, 3400
        let grid: [i16; 9] = [
            1000,
            1200,
            1400,
            2000,
            SRTM_VOID_I16,
            2400,
            3000,
            3200,
            3400,
        ];
        let mut raw = Vec::with_capacity(18);
        for v in grid {
            raw.extend_from_slice(&v.to_be_bytes());
        }
        let tile = SrtmTile::from_hgt_bytes(raw, -10, -77).unwrap();

        assert_eq!(tile.sample(-77.0, -9.0), Some(1000));
        assert_eq!(tile.sample(-76.0, -10.0), Some(3400));

        // Midpoint of top-left cell (-76.75, -9.25) has 3 valid corners (1000, 1200, 2000) and
        // void bottom-right corner; weighted average over equal 0.25 weights = (1000+1200+2000)/3 = 1400.
        assert_eq!(tile.sample(-76.75, -9.25), Some(1400));
    }

    #[test]
    fn slope_calculation_captures_valley_reversal_peak() {
        // Endpoint diff = 0m, but midpoint climbs 300m over 2 km total (1 km half-legs => 30.0%)
        let (avg, fwd, rev) = compute_segment_slopes(2000, Some(2300), 2000, 2.0);
        assert_eq!(avg, 0.0);
        assert_eq!(fwd, 30.0);
        assert_eq!(rev, 30.0);
    }

    #[test]
    fn profile_slopes_captures_steep_quarter_step() {
        // 4-interval 2 km profile (500 m per interval): climbs 150 m on interval 2 -> 30.0% peak.
        let (avg, fwd, rev) = compute_profile_slopes(&[1800, 1800, 1950, 1950, 1900], 2.0);
        assert_eq!(avg, 5.0);
        assert_eq!(fwd, 30.0);
        assert_eq!(rev, 30.0);
    }

    #[test]
    fn soil_susceptibility_is_bounded() {
        assert!((0.10..=0.95).contains(&heuristic_soil_susceptibility(0.0, 500)));
        assert_eq!(heuristic_soil_susceptibility(45.0, 0), 0.95);
    }
}
