#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::{Element, ElementReader, RelMemberType};
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::regions::{seed_region_hierarchy, ProvinceSpec, RegionSpec};

pub const DEFAULT_HIGHWAY_CLASSES: &[&str] = &[
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "unclassified",
    "motorway_link",
    "trunk_link",
    "primary_link",
    "secondary_link",
];

#[derive(Debug, Clone)]
struct PlacePoi {
    osm_id: i64,
    name: String,
    ubigeo_tag: Option<String>,
    place_kind: String,
    lon: f64,
    lat: f64,
}

#[derive(Debug, Clone)]
struct HospitalPoi {
    name: String,
    lon: f64,
    lat: f64,
}

#[derive(Debug, Clone)]
struct ParsedHighwayWay {
    way_id: i64,
    highway: String,
    layer: i8,
    corridor_name: String,
    direction_type: &'static str,
    surface_type: &'static str,
    road_width_m: f64,
    max_supported_tons: f64,
    node_ids: Vec<i64>,
}

#[derive(Debug, Clone)]
struct ParsedWaterway {
    external_source_id: String,
    waterway_type: &'static str,
    name: Option<String>,
    wkt: String,
}

#[derive(Debug, Clone)]
struct SplitEdge {
    external_source_id: String,
    origin_osm_id: i64,
    dest_osm_id: i64,
    corridor_name: String,
    direction_type: &'static str,
    surface_type: &'static str,
    road_width_m: f64,
    max_supported_tons: f64,
    length_km: f64,
    base_travel_time_min: f64,
    wkt: String,
}

#[derive(Debug, Clone, Default)]
pub struct OsmIngestStats {
    pub districts_seeded: usize,
    pub road_nodes_upserted: usize,
    pub road_segments_upserted: usize,
    pub road_segments_verified: usize,
    pub waterways_upserted: usize,
}

pub fn haversine_m(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
}

pub fn parse_osm_layer(layer_tag: Option<&str>, bridge: Option<&str>, tunnel: Option<&str>) -> i8 {
    if let Some(raw) = layer_tag {
        if let Ok(v) = raw.trim().parse::<i8>() {
            return v;
        }
    }
    if matches!(bridge, Some("yes" | "true" | "viaduct" | "1")) {
        return 1;
    }
    if matches!(tunnel, Some("yes" | "true" | "1")) {
        return -1;
    }
    0
}

pub fn infer_surface_type(surface: Option<&str>, highway: &str) -> &'static str {
    if let Some(raw) = surface {
        let s = raw.trim().to_ascii_lowercase();
        if matches!(
            s.as_str(),
            "asphalt" | "paved" | "concrete" | "concrete:lanes" | "paving_stones" | "cobblestone"
        ) {
            return "PAVED";
        }
        if matches!(
            s.as_str(),
            "gravel" | "fine_gravel" | "compacted" | "pebblestone" | "macadam"
        ) {
            return "GRAVEL";
        }
        if matches!(
            s.as_str(),
            "unpaved" | "dirt" | "earth" | "ground" | "mud" | "sand" | "clay" | "grass" | "track"
        ) {
            return "DIRT_TRACK";
        }
    }
    match highway {
        "motorway" | "motorway_link" | "trunk" | "trunk_link" | "primary" | "primary_link" => {
            "PAVED"
        }
        "secondary" | "secondary_link" | "tertiary" => "GRAVEL",
        _ => "DIRT_TRACK",
    }
}

pub fn infer_direction_type(oneway: Option<&str>, junction: Option<&str>) -> &'static str {
    if matches!(oneway, Some("yes" | "true" | "1" | "-1"))
        || matches!(junction, Some("roundabout" | "circular"))
    {
        "ONE_WAY"
    } else {
        "BIDIRECTIONAL"
    }
}

pub fn parse_numeric_tag(raw: Option<&str>) -> Option<f64> {
    let val = raw?.trim().to_ascii_lowercase();
    let cleaned = val
        .trim_end_matches("m")
        .trim_end_matches("t")
        .trim_end_matches("tons")
        .trim()
        .replace(',', ".");
    cleaned
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn default_width_and_tons(highway: &str, surface_type: &str) -> (f64, f64) {
    match (highway, surface_type) {
        ("motorway" | "trunk" | "trunk_link", _) => (7.0, 48.0),
        ("primary" | "primary_link", "PAVED") => (6.5, 42.0),
        ("primary" | "primary_link", _) => (5.5, 32.0),
        ("secondary" | "secondary_link", "PAVED") => (6.0, 36.0),
        ("secondary" | "secondary_link", _) => (4.5, 22.0),
        ("tertiary", "PAVED") => (5.5, 25.0),
        ("tertiary", _) => (3.8, 16.0),
        _ => (3.2, 10.0),
    }
}

fn nominal_speed_kmh(highway: &str, surface_type: &str) -> f64 {
    match (highway, surface_type) {
        ("motorway" | "trunk" | "trunk_link", "PAVED") => 60.0,
        ("primary" | "primary_link", "PAVED") => 50.0,
        (_, "PAVED") => 42.0,
        (_, "GRAVEL") => 30.0,
        _ => 18.0,
    }
}

/// Splits parsed highway ways into topological edges using `(osm_node_id, layer)` for interior
/// degree checks so non-intersecting overpasses (`layer=1` over `layer=0`) are NOT connected.
fn split_ways_layer_aware(
    ways: &[ParsedHighwayWay],
    coords: &HashMap<i64, (f64, f64)>,
) -> Vec<SplitEdge> {
    let mut layer_degree: HashMap<(i64, i8), usize> = HashMap::new();
    for way in ways {
        for (idx, &nid) in way.node_ids.iter().enumerate() {
            let entry = layer_degree.entry((nid, way.layer)).or_insert(0);
            // Endpoints and repeated interior occurrences force topological split.
            *entry += if idx == 0 || idx + 1 == way.node_ids.len() {
                2
            } else {
                1
            };
        }
    }

    let mut edges = Vec::new();
    for way in ways {
        if way.node_ids.len() < 2 {
            continue;
        }
        let mut sub_idx = 0usize;
        let mut start = 0usize;
        while start + 1 < way.node_ids.len() {
            let mut end = start + 1;
            while end + 1 < way.node_ids.len() {
                let nid = way.node_ids[end];
                if layer_degree.get(&(nid, way.layer)).copied().unwrap_or(0) >= 2 {
                    break;
                }
                end += 1;
            }

            let slice = &way.node_ids[start..=end];
            let orig = slice[0];
            let dest = slice[slice.len() - 1];
            if orig != dest {
                let pts: Vec<(f64, f64)> = slice
                    .iter()
                    .filter_map(|id| coords.get(id).copied())
                    .collect();
                if pts.len() >= 2 {
                    let dist_m: f64 = pts
                        .windows(2)
                        .map(|w| haversine_m(w[0].0, w[0].1, w[1].0, w[1].1))
                        .sum();
                    if dist_m >= 50.0 {
                        let length_km = ((dist_m / 1000.0) * 100.0).round().max(5.0) / 100.0;
                        let speed = nominal_speed_kmh(&way.highway, way.surface_type);
                        let travel_min =
                            ((length_km / speed) * 60.0 * 100.0).round().max(10.0) / 100.0;
                        let wkt_coords = pts
                            .iter()
                            .map(|(lon, lat)| format!("{lon:.6} {lat:.6}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        edges.push(SplitEdge {
                            external_source_id: format!("osm:way:{}:{sub_idx}", way.way_id),
                            origin_osm_id: orig,
                            dest_osm_id: dest,
                            corridor_name: way.corridor_name.clone(),
                            direction_type: way.direction_type,
                            surface_type: way.surface_type,
                            road_width_m: way.road_width_m,
                            max_supported_tons: way.max_supported_tons,
                            length_km,
                            base_travel_time_min: travel_min,
                            wkt: format!("LINESTRING({wkt_coords})"),
                        });
                        sub_idx += 1;
                    }
                }
            }
            start = end;
        }
    }
    edges
}

fn format_district_ubigeo_code(ubigeo_opt: Option<&str>, kind: char, osm_id: i64) -> String {
    if let Some(raw) = ubigeo_opt {
        if let Some(first) = raw.split(';').next() {
            let cleaned = first.trim();
            if !cleaned.is_empty() && cleaned.chars().count() <= 16 {
                return cleaned.to_string();
            }
        }
    }
    format!("O{kind}{:x}", osm_id.unsigned_abs() & 0x00FF_FFFF_FFFF_FFFF)
}

fn find_containing_province(
    lon: f64,
    lat: f64,
    provinces: &[(Uuid, ProvinceSpec)],
    region_id: Uuid,
) -> Uuid {
    provinces
        .iter()
        .filter(|(_, p)| p.bbox.contains(lon, lat))
        .min_by(|(_, a), (_, b)| {
            let da = haversine_m(lon, lat, a.capital_lon, a.capital_lat);
            let db = haversine_m(lon, lat, b.capital_lon, b.capital_lat);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(id, _)| *id)
        .unwrap_or(region_id)
}

async fn append_sync_change(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: Uuid,
    payload_json: serde_json::Value,
) -> Result<(), String> {
    let row = sqlx::query(
        "UPDATE sync_control SET last_seq = last_seq + 1 WHERE singleton_id = 1 RETURNING last_seq",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    let change_seq: i64 = row.try_get("last_seq").map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO sync_change_log (change_seq, entity_type, entity_id, operation, payload_json)
         VALUES ($1, $2, $3, 'UPSERT', $4)",
    )
    .bind(change_seq)
    .bind(entity_type)
    .bind(entity_id)
    .bind(sqlx::types::Json(payload_json))
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn ingest_osm_region(
    pool: &PgPool,
    pbf_path: &Path,
    region: &RegionSpec,
    allowed_highways: &[String],
) -> Result<OsmIngestStats, String> {
    let (region_unit_id, provinces) = seed_region_hierarchy(pool, region)
        .await
        .map_err(|e| e.to_string())?;

    let padded_bbox = region.bbox.padded(0.05);
    let highway_set: HashSet<String> = allowed_highways
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();

    // Pass 1: Read coordinates inside padded region bbox + place/hospital POIs.
    let mut coords: HashMap<i64, (f64, f64)> = HashMap::new();
    let mut places: Vec<PlacePoi> = Vec::new();
    let mut hospitals: Vec<HospitalPoi> = Vec::new();

    let reader1 = ElementReader::from_path(pbf_path).map_err(|e| e.to_string())?;
    reader1
        .for_each(|el| {
            let (id, lon, lat, tags_iter): (i64, f64, f64, Vec<(&str, &str)>) = match el {
                Element::DenseNode(dn) => {
                    if !padded_bbox.contains(dn.lon(), dn.lat()) {
                        return;
                    }
                    (dn.id(), dn.lon(), dn.lat(), dn.tags().collect())
                }
                Element::Node(n) => {
                    if !padded_bbox.contains(n.lon(), n.lat()) {
                        return;
                    }
                    (n.id(), n.lon(), n.lat(), n.tags().collect())
                }
                _ => return,
            };
            coords.insert(id, (lon, lat));
            if !region.bbox.contains(lon, lat) {
                return;
            }
            let mut place_val = None;
            let mut amenity_val = None;
            let mut name_val = None;
            let mut ubigeo_val = None;
            for (k, v) in tags_iter {
                match k {
                    "place" => place_val = Some(v),
                    "amenity" => amenity_val = Some(v),
                    "name" => name_val = Some(v),
                    "pe:ubigeo" | "ubigeo" | "ref:ubigeo" => ubigeo_val = Some(v),
                    _ => {}
                }
            }
            if let (Some(place), Some(name)) = (place_val, name_val) {
                if matches!(place, "city" | "town" | "village") {
                    places.push(PlacePoi {
                        osm_id: id,
                        name: name.to_string(),
                        ubigeo_tag: ubigeo_val.map(str::to_string),
                        place_kind: place.to_string(),
                        lon,
                        lat,
                    });
                }
            }
            if let (Some(amenity), Some(name)) = (amenity_val, name_val) {
                if matches!(amenity, "hospital" | "clinic") {
                    hospitals.push(HospitalPoi {
                        name: name.to_string(),
                        lon,
                        lat,
                    });
                }
            }
        })
        .map_err(|e| e.to_string())?;

    tracing::info!(
        region = %region.iso_code,
        nodes_in_bbox = coords.len(),
        places = places.len(),
        hospitals = hospitals.len(),
        "OSM Pass 1 complete"
    );

    // Pass 2: Extract highway ways, waterways, and admin_level=8 relations.
    let mut ways: Vec<ParsedHighwayWay> = Vec::new();
    let mut waterways: Vec<ParsedWaterway> = Vec::new();
    let mut district_rels: Vec<(i64, String, Option<String>, f64, f64)> = Vec::new();

    let reader2 = ElementReader::from_path(pbf_path).map_err(|e| e.to_string())?;
    reader2
        .for_each(|el| match el {
            Element::Way(w) => {
                let mut highway = None;
                let mut waterway = None;
                let mut name = None;
                let mut route_ref = None;
                let mut surface = None;
                let mut oneway = None;
                let mut junction = None;
                let mut width = None;
                let mut maxweight = None;
                let mut layer_tag = None;
                let mut bridge = None;
                let mut tunnel = None;

                for (k, v) in w.tags() {
                    match k {
                        "highway" => highway = Some(v),
                        "waterway" => waterway = Some(v),
                        "name" => name = Some(v),
                        "ref" => route_ref = Some(v),
                        "surface" => surface = Some(v),
                        "oneway" => oneway = Some(v),
                        "junction" => junction = Some(v),
                        "width" => width = Some(v),
                        "maxweight" => maxweight = Some(v),
                        "layer" => layer_tag = Some(v),
                        "bridge" => bridge = Some(v),
                        "tunnel" => tunnel = Some(v),
                        _ => {}
                    }
                }

                if highway.is_none() && waterway.is_none() {
                    return;
                }

                let node_ids: Vec<i64> = w.refs().collect();
                let valid_pts: Vec<(f64, f64)> = node_ids
                    .iter()
                    .filter_map(|id| coords.get(id).copied())
                    .collect();
                if valid_pts.len() < 2
                    || !valid_pts
                        .iter()
                        .any(|(lon, lat)| region.bbox.contains(*lon, *lat))
                {
                    return;
                }

                if let Some(hw) = highway {
                    let hw_lc = hw.trim().to_ascii_lowercase();
                    if highway_set.contains(&hw_lc) {
                        let surf = infer_surface_type(surface, &hw_lc);
                        let (def_w, def_t) = default_width_and_tons(&hw_lc, surf);
                        let road_w = parse_numeric_tag(width).unwrap_or(def_w).clamp(1.0, 20.0);
                        let max_t = parse_numeric_tag(maxweight)
                            .unwrap_or(def_t)
                            .clamp(0.4, 80.0);
                        let corridor_name = match (name, route_ref) {
                            (Some(n), _) if !n.trim().is_empty() => n.trim().to_string(),
                            (_, Some(r)) if !r.trim().is_empty() => r.trim().to_string(),
                            _ => format!("Red Vial {hw_lc}"),
                        };
                        let clipped: Vec<i64> = node_ids
                            .into_iter()
                            .filter(|id| coords.contains_key(id))
                            .collect();
                        if clipped.len() >= 2 {
                            ways.push(ParsedHighwayWay {
                                way_id: w.id(),
                                highway: hw_lc,
                                layer: parse_osm_layer(layer_tag, bridge, tunnel),
                                corridor_name: corridor_name.chars().take(100).collect(),
                                direction_type: infer_direction_type(oneway, junction),
                                surface_type: surf,
                                road_width_m: (road_w * 100.0).round() / 100.0,
                                max_supported_tons: (max_t * 100.0).round() / 100.0,
                                node_ids: clipped,
                            });
                        }
                    }
                } else if let Some(ww) = waterway {
                    let ww_type = match ww.trim().to_ascii_lowercase().as_str() {
                        "river" => "river",
                        "stream" => "stream",
                        "ravine" | "ditch" | "drain" => "ravine",
                        "canal" => "canal",
                        _ => return,
                    };
                    let wkt_pts = valid_pts
                        .iter()
                        .map(|(lon, lat)| format!("{lon:.6} {lat:.6}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    waterways.push(ParsedWaterway {
                        external_source_id: format!("osm:waterway:{}", w.id()),
                        waterway_type: ww_type,
                        name: name.map(|n| n.chars().take(120).collect()),
                        wkt: format!("LINESTRING({wkt_pts})"),
                    });
                }
            }
            Element::Relation(r) => {
                let mut boundary = None;
                let mut admin_lvl = None;
                let mut name = None;
                let mut ubigeo = None;
                for (k, v) in r.tags() {
                    match k {
                        "boundary" => boundary = Some(v),
                        "admin_level" => admin_lvl = Some(v),
                        "name" => name = Some(v),
                        "pe:ubigeo" | "ubigeo" | "ref:ubigeo" => ubigeo = Some(v),
                        _ => {}
                    }
                }
                if boundary == Some("administrative") && admin_lvl == Some("8") {
                    if let Some(dname) = name {
                        let mut admin_center = None;
                        let mut first_node = None;
                        for m in r.members() {
                            if m.member_type == RelMemberType::Node {
                                if let Some(&pt) = coords.get(&m.member_id) {
                                    first_node.get_or_insert(pt);
                                    if matches!(m.role(), Ok("admin_centre" | "label")) {
                                        admin_center = Some(pt);
                                        break;
                                    }
                                }
                            }
                        }
                        if let Some((lon, lat)) = admin_center.or(first_node) {
                            if region.bbox.contains(lon, lat) {
                                district_rels.push((
                                    r.id(),
                                    dname.chars().take(120).collect(),
                                    ubigeo.map(str::to_string),
                                    lon,
                                    lat,
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        })
        .map_err(|e| e.to_string())?;

    let split_edges = split_ways_layer_aware(&ways, &coords);
    tracing::info!(
        ways = ways.len(),
        edges = split_edges.len(),
        waterways = waterways.len(),
        district_rels = district_rels.len(),
        "OSM Pass 2 and layer-aware graph splitting complete"
    );

    let mut stats = OsmIngestStats::default();

    // Seed OSM district units (boundary relations first, supplemented by city/town nodes).
    let mut seen_district_names: HashSet<String> = HashSet::new();
    for (rel_id, dname, ubigeo_opt, lon, lat) in &district_rels {
        let code = format_district_ubigeo_code(ubigeo_opt.as_deref(), 'R', *rel_id);
        let parent = find_containing_province(*lon, *lat, &provinces, region_unit_id);
        let box_wkt = crate::regions::BBox {
            min_lon: lon - 0.08,
            min_lat: lat - 0.08,
            max_lon: lon + 0.08,
            max_lat: lat + 0.08,
        }
        .polygon_wkt();
        sqlx::query(
            "INSERT INTO territorial_units (
                ubigeo_code, parent_unit_id, name, admin_level, bbox_geom, centroid_geom, is_enabled
             ) VALUES (
                $1, $2, $3, 8, ST_GeomFromText($4, 4326), ST_SetSRID(ST_MakePoint($5, $6), 4326), TRUE
             )
             ON CONFLICT (ubigeo_code) DO UPDATE SET
                parent_unit_id = EXCLUDED.parent_unit_id,
                name = EXCLUDED.name,
                centroid_geom = EXCLUDED.centroid_geom,
                updated_at = NOW()",
        )
        .bind(code)
        .bind(parent)
        .bind(dname)
        .bind(box_wkt)
        .bind(lon)
        .bind(lat)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        seen_district_names.insert(dname.to_ascii_lowercase());
        stats.districts_seeded += 1;
    }

    for p in &places {
        if matches!(p.place_kind.as_str(), "city" | "town")
            && !seen_district_names.contains(&p.name.to_ascii_lowercase())
        {
            let code = format_district_ubigeo_code(p.ubigeo_tag.as_deref(), 'N', p.osm_id);
            let parent = find_containing_province(p.lon, p.lat, &provinces, region_unit_id);
            let box_wkt = crate::regions::BBox {
                min_lon: p.lon - 0.06,
                min_lat: p.lat - 0.06,
                max_lon: p.lon + 0.06,
                max_lat: p.lat + 0.06,
            }
            .polygon_wkt();
            sqlx::query(
                "INSERT INTO territorial_units (
                    ubigeo_code, parent_unit_id, name, admin_level, bbox_geom, centroid_geom, is_enabled
                 ) VALUES (
                    $1, $2, $3, 8, ST_GeomFromText($4, 4326), ST_SetSRID(ST_MakePoint($5, $6), 4326), TRUE
                 )
                 ON CONFLICT (ubigeo_code) DO NOTHING",
            )
            .bind(code)
            .bind(parent)
            .bind(p.name.chars().take(120).collect::<String>())
            .bind(box_wkt)
            .bind(p.lon)
            .bind(p.lat)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            stats.districts_seeded += 1;
        }
    }

    // Collect distinct topological endpoint nodes used by split_edges.
    let mut used_node_ids: Vec<i64> = {
        let mut s = HashSet::new();
        for e in &split_edges {
            s.insert(e.origin_osm_id);
            s.insert(e.dest_osm_id);
        }
        s.into_iter().collect()
    };
    used_node_ids.sort_unstable();

    // Upsert road_nodes in bulk via UNNEST.
    let mut osm_to_uuid: HashMap<i64, Uuid> = HashMap::with_capacity(used_node_ids.len());
    for chunk in used_node_ids.chunks(500) {
        let mut ext_ids = Vec::with_capacity(chunk.len());
        let mut names = Vec::with_capacity(chunk.len());
        let mut node_types = Vec::with_capacity(chunk.len());
        let mut lons = Vec::with_capacity(chunk.len());
        let mut lats = Vec::with_capacity(chunk.len());
        let mut radii = Vec::with_capacity(chunk.len());

        for &nid in chunk {
            let (lon, lat) = coords[&nid];
            let mut ntype = "INTERSECTION";
            let mut nname = format!("Nodo {nid}");
            let mut urban_radius = 0i32;

            if let Some(hosp) = hospitals
                .iter()
                .find(|h| haversine_m(lon, lat, h.lon, h.lat) <= 250.0)
            {
                ntype = "HOSPITAL";
                nname = hosp.name.chars().take(120).collect();
            } else if let Some(pl) = places
                .iter()
                .find(|p| haversine_m(lon, lat, p.lon, p.lat) <= 450.0)
            {
                ntype = "TOWN_CENTER";
                nname = pl.name.chars().take(120).collect();
                urban_radius = match pl.place_kind.as_str() {
                    "city" => 1500,
                    "town" => 600,
                    _ => 250,
                };
            }

            ext_ids.push(format!("osm:node:{nid}"));
            names.push(nname);
            node_types.push(ntype.to_string());
            lons.push(lon);
            lats.push(lat);
            radii.push(urban_radius);
        }

        let rows = sqlx::query(
            "INSERT INTO road_nodes (
                external_source_id, name, node_type, location_geom, elevation_m, is_geotechnically_stable, urban_radius_m
             )
             SELECT
                t.ext_id,
                t.name,
                t.node_type,
                ST_SetSRID(ST_MakePoint(t.lon, t.lat), 4326),
                -9999,
                TRUE,
                t.urban_radius
             FROM UNNEST($1::text[], $2::text[], $3::text[], $4::float8[], $5::float8[], $6::int4[])
                AS t(ext_id, name, node_type, lon, lat, urban_radius)
             ON CONFLICT (external_source_id) WHERE external_source_id IS NOT NULL DO UPDATE SET
                name = EXCLUDED.name,
                node_type = EXCLUDED.node_type,
                location_geom = EXCLUDED.location_geom,
                urban_radius_m = EXCLUDED.urban_radius_m
             RETURNING external_source_id, node_id",
        )
        .bind(&ext_ids)
        .bind(&names)
        .bind(&node_types)
        .bind(&lons)
        .bind(&lats)
        .bind(&radii)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        for row in rows {
            let ext: String = row
                .try_get("external_source_id")
                .map_err(|e| e.to_string())?;
            let uuid: Uuid = row.try_get("node_id").map_err(|e| e.to_string())?;
            if let Ok(nid) = ext.trim_start_matches("osm:node:").parse::<i64>() {
                osm_to_uuid.insert(nid, uuid);
            }
            stats.road_nodes_upserted += 1;
        }
    }

    // Bulk upsert waterways.
    for chunk in waterways.chunks(500) {
        let ext_ids: Vec<&str> = chunk
            .iter()
            .map(|w| w.external_source_id.as_str())
            .collect();
        let types: Vec<&str> = chunk.iter().map(|w| w.waterway_type).collect();
        let names: Vec<Option<&str>> = chunk.iter().map(|w| w.name.as_deref()).collect();
        let wkts: Vec<&str> = chunk.iter().map(|w| w.wkt.as_str()).collect();

        sqlx::query(
            "INSERT INTO hydrography_waterways (external_source_id, waterway_type, name, geom)
             SELECT t.ext_id, t.wtype, t.name, ST_GeomFromText(t.wkt, 4326)
             FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[]) AS t(ext_id, wtype, name, wkt)
             ON CONFLICT (external_source_id) DO UPDATE SET
                waterway_type = EXCLUDED.waterway_type,
                name = EXCLUDED.name,
                geom = EXCLUDED.geom",
        )
        .bind(&ext_ids)
        .bind(&types)
        .bind(&names)
        .bind(&wkts)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
        stats.waterways_upserted += chunk.len();
    }

    // Bulk upsert road_segments with attributes_verified_at strictly NULL.
    for chunk in split_edges.chunks(400) {
        let mut ext_ids = Vec::with_capacity(chunk.len());
        let mut orig_ids = Vec::with_capacity(chunk.len());
        let mut dest_ids = Vec::with_capacity(chunk.len());
        let mut names = Vec::with_capacity(chunk.len());
        let mut wkts = Vec::with_capacity(chunk.len());
        let mut dirs = Vec::with_capacity(chunk.len());
        let mut lengths = Vec::with_capacity(chunk.len());
        let mut fwd_slopes = Vec::with_capacity(chunk.len());
        let mut rev_slopes: Vec<Option<f64>> = Vec::with_capacity(chunk.len());
        let mut widths = Vec::with_capacity(chunk.len());
        let mut tons = Vec::with_capacity(chunk.len());
        let mut surfaces = Vec::with_capacity(chunk.len());
        let mut times = Vec::with_capacity(chunk.len());

        for edge in chunk {
            let Some(&orig_uuid) = osm_to_uuid.get(&edge.origin_osm_id) else {
                continue;
            };
            let Some(&dest_uuid) = osm_to_uuid.get(&edge.dest_osm_id) else {
                continue;
            };
            if orig_uuid == dest_uuid {
                continue;
            }
            ext_ids.push(edge.external_source_id.as_str());
            orig_ids.push(orig_uuid);
            dest_ids.push(dest_uuid);
            names.push(edge.corridor_name.as_str());
            wkts.push(edge.wkt.as_str());
            dirs.push(edge.direction_type);
            lengths.push(edge.length_km);
            fwd_slopes.push(0.0f64);
            rev_slopes.push(if edge.direction_type == "BIDIRECTIONAL" {
                Some(0.0f64)
            } else {
                None
            });
            widths.push(edge.road_width_m);
            tons.push(edge.max_supported_tons);
            surfaces.push(edge.surface_type);
            times.push(edge.base_travel_time_min);
        }

        if ext_ids.is_empty() {
            continue;
        }

        sqlx::query(
            "INSERT INTO road_segments (
                external_source_id, origin_node_id, destination_node_id, corridor_name, geom,
                direction_type, length_km, avg_slope_pct, max_slope_forward_pct, max_slope_reverse_pct,
                road_width_m, max_supported_tons, surface_type, soil_susceptibility_idx,
                distance_to_ravine_m, is_historical_hotspot, base_travel_time_min,
                is_topology_verified, geometry_source, attributes_verified_at
             )
             SELECT
                t.ext_id,
                t.orig_id,
                t.dest_id,
                t.corridor_name,
                ST_GeomFromText(t.wkt, 4326),
                t.dir_type,
                GREATEST(0.05, ROUND((ST_Length(ST_GeomFromText(t.wkt, 4326)::geography) / 1000.0)::numeric, 2)),
                0.0,
                t.fwd_slope::numeric(4,1),
                t.rev_slope::numeric(4,1),
                t.width::numeric(4,2),
                t.tons::numeric(5,2),
                t.surface,
                0.30,
                500,
                FALSE,
                t.travel_min::numeric(6,2),
                FALSE,
                'OSM_CURATED',
                NULL
             FROM UNNEST(
                $1::text[], $2::uuid[], $3::uuid[], $4::text[], $5::text[], $6::text[],
                $7::float8[], $8::float8[], $9::float8[], $10::float8[], $11::float8[],
                $12::text[], $13::float8[]
             ) AS t(
                ext_id, orig_id, dest_id, corridor_name, wkt, dir_type,
                len_km, fwd_slope, rev_slope, width, tons, surface, travel_min
             )
             ON CONFLICT (external_source_id) WHERE external_source_id IS NOT NULL DO UPDATE SET
                origin_node_id = EXCLUDED.origin_node_id,
                destination_node_id = EXCLUDED.destination_node_id,
                corridor_name = EXCLUDED.corridor_name,
                geom = EXCLUDED.geom,
                direction_type = EXCLUDED.direction_type,
                length_km = EXCLUDED.length_km,
                road_width_m = EXCLUDED.road_width_m,
                max_supported_tons = EXCLUDED.max_supported_tons,
                surface_type = EXCLUDED.surface_type,
                base_travel_time_min = EXCLUDED.base_travel_time_min,
                max_slope_reverse_pct = CASE
                    WHEN EXCLUDED.direction_type = 'ONE_WAY' THEN NULL
                    ELSE COALESCE(road_segments.max_slope_reverse_pct, 0.0)
                END",
        )
        .bind(&ext_ids)
        .bind(&orig_ids)
        .bind(&dest_ids)
        .bind(&names)
        .bind(&wkts)
        .bind(&dirs)
        .bind(&lengths)
        .bind(&fwd_slopes)
        .bind(&rev_slopes)
        .bind(&widths)
        .bind(&tons)
        .bind(&surfaces)
        .bind(&times)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

        stats.road_segments_upserted += ext_ids.len();
    }

    // PostGIS metric spatial joins: distance to ravine/river + district/province link + SDD 6.1 topology gate.
    let region_wkt = padded_bbox.polygon_wkt();
    sqlx::query(
        "UPDATE road_segments rs
         SET distance_to_ravine_m = COALESCE(
             (
                 SELECT LEAST(5000, ROUND(ST_Distance(rs.geom::geography, hw.geom::geography)))::int4
                 FROM hydrography_waterways hw
                 WHERE hw.geom && ST_Expand(rs.geom, 0.05)
                   AND ST_DWithin(rs.geom::geography, hw.geom::geography, 5000)
                 ORDER BY rs.geom <-> hw.geom
                 LIMIT 1
             ),
             500
         ),
         territorial_unit_id = (
             SELECT tu.unit_id
             FROM territorial_units tu
             WHERE ST_Intersects(ST_Centroid(rs.geom), tu.bbox_geom)
             ORDER BY tu.admin_level DESC, ST_Centroid(rs.geom) <-> tu.centroid_geom
             LIMIT 1
         )
         WHERE rs.external_source_id LIKE 'osm:%'
           AND ST_Intersects(rs.geom, ST_GeomFromText($1, 4326))",
    )
    .bind(&region_wkt)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let verified_row = sqlx::query(
        "UPDATE road_segments rs
         SET is_topology_verified = (
             ST_IsValid(rs.geom)
             AND ST_IsSimple(rs.geom)
             AND ST_Length(rs.geom::geography) > 0
             AND ST_DWithin(ST_StartPoint(rs.geom)::geography, o.location_geom::geography, 15)
             AND ST_DWithin(ST_EndPoint(rs.geom)::geography, d.location_geom::geography, 15)
         )
         FROM road_nodes o, road_nodes d
         WHERE o.node_id = rs.origin_node_id
           AND d.node_id = rs.destination_node_id
           AND rs.external_source_id LIKE 'osm:%'
           AND ST_Intersects(rs.geom, ST_GeomFromText($1, 4326))
           AND rs.is_topology_verified IS DISTINCT FROM TRUE",
    )
    .bind(&region_wkt)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let total_verified_row = sqlx::query(
        "SELECT count(*) AS cnt
         FROM road_segments
         WHERE is_topology_verified = TRUE
           AND ST_Intersects(geom, ST_GeomFromText($1, 4326))",
    )
    .bind(&region_wkt)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let cnt: i64 = total_verified_row
        .try_get("cnt")
        .map_err(|e| e.to_string())?;
    stats.road_segments_verified = cnt.max(0) as usize;
    let _ = verified_row;

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    append_sync_change(
        &mut tx,
        "ROAD_SEGMENT",
        region_unit_id,
        json!({
            "region": region.iso_code,
            "road_nodes_upserted": stats.road_nodes_upserted,
            "road_segments_upserted": stats.road_segments_upserted,
            "road_segments_verified": stats.road_segments_verified,
        }),
    )
    .await?;
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_aware_split_does_not_connect_overpass_crossing() {
        let mut coords = HashMap::new();
        coords.insert(1, (-76.100, -9.900));
        coords.insert(2, (-76.090, -9.900)); // shared crossing node id 2 at different layer!
        coords.insert(3, (-76.080, -9.900));
        coords.insert(4, (-76.090, -9.910));
        coords.insert(5, (-76.090, -9.890));

        let ways = vec![
            ParsedHighwayWay {
                way_id: 100,
                highway: "trunk".to_string(),
                layer: 0,
                corridor_name: "Carretera Central".to_string(),
                direction_type: "BIDIRECTIONAL",
                surface_type: "PAVED",
                road_width_m: 7.0,
                max_supported_tons: 48.0,
                node_ids: vec![1, 2, 3],
            },
            ParsedHighwayWay {
                way_id: 200,
                highway: "primary".to_string(),
                layer: 1, // elevated bridge crossing over node 2
                corridor_name: "Viaducto".to_string(),
                direction_type: "BIDIRECTIONAL",
                surface_type: "PAVED",
                road_width_m: 6.5,
                max_supported_tons: 42.0,
                node_ids: vec![4, 2, 5],
            },
        ];

        let edges = split_ways_layer_aware(&ways, &coords);
        // Neither way was split at interior node 2 because layer=0 degree=1 and layer=1 degree=1!
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].origin_osm_id, 1);
        assert_eq!(edges[0].dest_osm_id, 3);
        assert_eq!(edges[1].origin_osm_id, 4);
        assert_eq!(edges[1].dest_osm_id, 5);
    }

    #[test]
    fn surface_and_direction_inference_are_conservative() {
        assert_eq!(infer_surface_type(Some("asphalt"), "unclassified"), "PAVED");
        assert_eq!(infer_surface_type(None, "secondary"), "GRAVEL");
        assert_eq!(infer_surface_type(None, "unclassified"), "DIRT_TRACK");
        assert_eq!(infer_direction_type(Some("yes"), None), "ONE_WAY");
        assert_eq!(infer_direction_type(None, Some("roundabout")), "ONE_WAY");
        assert_eq!(infer_direction_type(None, None), "BIDIRECTIONAL");
    }

    #[test]
    fn district_ubigeo_code_always_fits_varchar_16() {
        assert_eq!(
            format_district_ubigeo_code(Some("250101"), 'R', 99),
            "250101"
        );
        assert_eq!(
            format_district_ubigeo_code(Some("250101;250102;250103"), 'R', 99),
            "250101"
        );
        let long_fallback = format_district_ubigeo_code(
            Some("valor_extremadamente_largo_invalido"),
            'N',
            12_345_678_901,
        );
        assert!(long_fallback.len() <= 16);
        assert!(long_fallback.starts_with("ON"));

        let none_fallback = format_district_ubigeo_code(None, 'N', 12_345_678_901);
        assert!(none_fallback.len() <= 16);
        assert!(none_fallback.starts_with("ON"));
    }
}
