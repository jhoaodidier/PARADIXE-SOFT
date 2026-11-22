#![forbid(unsafe_code)]

use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use paradixe_domain::{DirectionType, PassabilityState, SurfaceType, TransferMode};
use paradixe_routing::{NodeSpec, SegmentSpec, TransferCorridorSpec};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const DRILL_FIXTURE_TAG: &str = "huanuco-carpish-drill-v1";

// Deterministic UUIDs for the Huánuco - Ambo - Carpish - Tingo María contingency drill.
pub const NODE_HUANUCO_ID: Uuid = Uuid::from_u128(0x1000_0000_0000_4000_8000_0000_0000_0001);
pub const NODE_AMBO_ID: Uuid = Uuid::from_u128(0x1000_0000_0000_4000_8000_0000_0000_0002);
pub const NODE_CARPISH_NORTE_ID: Uuid = Uuid::from_u128(0x1000_0000_0000_4000_8000_0000_0000_0003);
pub const NODE_CARPISH_SUR_ID: Uuid = Uuid::from_u128(0x1000_0000_0000_4000_8000_0000_0000_0004);
pub const NODE_TINGO_MARIA_ID: Uuid = Uuid::from_u128(0x1000_0000_0000_4000_8000_0000_0000_0005);

pub const SEG_HUANUCO_CARPISH_SUR_ID: Uuid =
    Uuid::from_u128(0x2000_0000_0000_4000_8000_0000_0000_0001);
pub const SEG_CARPISH_TUNNEL_ID: Uuid = Uuid::from_u128(0x2000_0000_0000_4000_8000_0000_0000_0002);
pub const SEG_CARPISH_NORTE_TINGO_ID: Uuid =
    Uuid::from_u128(0x2000_0000_0000_4000_8000_0000_0000_0003);
pub const SEG_BYPASS_AMBO_ID: Uuid = Uuid::from_u128(0x2000_0000_0000_4000_8000_0000_0000_0004);
pub const SEG_BYPASS_TINGO_ID: Uuid = Uuid::from_u128(0x2000_0000_0000_4000_8000_0000_0000_0005);

pub const SIM_TRANSFER_CORRIDOR_ID: Uuid =
    Uuid::from_u128(0x3000_0000_0000_4000_8000_0000_0000_0099);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillSegmentFixture {
    pub segment: SegmentSpec,
    pub soil_susceptibility_idx: Decimal,
    pub distance_to_ravine_m: Decimal,
    pub is_historical_hotspot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillCorridorFixture {
    pub corridor: TransferCorridorSpec,
    pub path_wkt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillFixturePayload {
    pub fixture_tag: String,
    pub reference_time: DateTime<Utc>,
    pub nodes: Vec<NodeSpec>,
    pub segments: Vec<DrillSegmentFixture>,
    pub transfer_corridors: Vec<DrillCorridorFixture>,
}

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).expect("valid static decimal literal")
}

/// Builds the immutable synthetic contingency drill fixture (`huanuco-carpish-drill-v1`).
/// The synthetic transfer corridor exists strictly inside `fixture_payload_json` and is
/// never inserted into the operational `transfer_corridors` table.
pub fn build_drill_fixture(fixture_tag: &str) -> Result<(DrillFixturePayload, String), String> {
    if fixture_tag != DRILL_FIXTURE_TAG {
        return Err(format!("Unsupported fixture_tag '{fixture_tag}'"));
    }

    let ref_time = DateTime::parse_from_rfc3339("2026-11-20T08:00:00Z")
        .expect("valid fixed timestamp")
        .with_timezone(&Utc);

    let nodes = vec![
        NodeSpec {
            node_id: NODE_HUANUCO_ID,
            is_geotechnically_stable: true,
        },
        NodeSpec {
            node_id: NODE_AMBO_ID,
            is_geotechnically_stable: true,
        },
        NodeSpec {
            node_id: NODE_CARPISH_NORTE_ID,
            is_geotechnically_stable: true,
        },
        NodeSpec {
            node_id: NODE_CARPISH_SUR_ID,
            is_geotechnically_stable: true,
        },
        NodeSpec {
            node_id: NODE_TINGO_MARIA_ID,
            is_geotechnically_stable: true,
        },
    ];

    let segments = vec![
        DrillSegmentFixture {
            segment: SegmentSpec {
                segment_id: SEG_HUANUCO_CARPISH_SUR_ID,
                origin_node_id: NODE_HUANUCO_ID,
                destination_node_id: NODE_CARPISH_SUR_ID,
                direction_type: DirectionType::Bidirectional,
                length_km: dec("45.00"),
                max_slope_forward_pct: dec("7.5"),
                max_slope_reverse_pct: Some(dec("7.5")),
                road_width_m: dec("6.50"),
                max_supported_tons: dec("42.00"),
                surface_type: SurfaceType::Paved,
                base_travel_time_min: dec("55.00"),
                is_topology_verified: true,
                attributes_verified_at: Some(ref_time),
                passability_state: PassabilityState::Open,
                active_confirmed_incident_id: None,
                ivv_score: Some(dec("22.00")),
                ivv_is_stale: false,
            },
            soil_susceptibility_idx: dec("0.30"),
            distance_to_ravine_m: dec("400"),
            is_historical_hotspot: false,
        },
        // Critical Carpish tunnel approach flank: 27% max slope, soil=0.80, ravine=200m, hotspot=true.
        // Extreme rainfall 85/190 mm yields IVV = 84.00 RED under ivv-v1.0-initial.
        DrillSegmentFixture {
            segment: SegmentSpec {
                segment_id: SEG_CARPISH_TUNNEL_ID,
                origin_node_id: NODE_CARPISH_SUR_ID,
                destination_node_id: NODE_CARPISH_NORTE_ID,
                direction_type: DirectionType::Bidirectional,
                length_km: dec("8.00"),
                max_slope_forward_pct: dec("27.0"),
                max_slope_reverse_pct: Some(dec("27.0")),
                road_width_m: dec("6.00"),
                max_supported_tons: dec("40.00"),
                surface_type: SurfaceType::Paved,
                base_travel_time_min: dec("15.00"),
                is_topology_verified: true,
                attributes_verified_at: Some(ref_time),
                passability_state: PassabilityState::Open,
                active_confirmed_incident_id: None,
                ivv_score: Some(dec("35.00")),
                ivv_is_stale: false,
            },
            soil_susceptibility_idx: dec("0.80"),
            distance_to_ravine_m: dec("200"),
            is_historical_hotspot: true,
        },
        DrillSegmentFixture {
            segment: SegmentSpec {
                segment_id: SEG_CARPISH_NORTE_TINGO_ID,
                origin_node_id: NODE_CARPISH_NORTE_ID,
                destination_node_id: NODE_TINGO_MARIA_ID,
                direction_type: DirectionType::Bidirectional,
                length_km: dec("65.00"),
                max_slope_forward_pct: dec("8.0"),
                max_slope_reverse_pct: Some(dec("8.0")),
                road_width_m: dec("6.50"),
                max_supported_tons: dec("42.00"),
                surface_type: SurfaceType::Paved,
                base_travel_time_min: dec("75.00"),
                is_topology_verified: true,
                attributes_verified_at: Some(ref_time),
                passability_state: PassabilityState::Open,
                active_confirmed_incident_id: None,
                ivv_score: Some(dec("28.00")),
                ivv_is_stale: false,
            },
            soil_susceptibility_idx: dec("0.35"),
            distance_to_ravine_m: dec("350"),
            is_historical_hotspot: false,
        },
        // Mountain bypass legs passable by 4x4 ambulance (18% slope, 3.1m width, gravel)
        // but pruned for heavy trailers (max 10% slope, min 4.0m width).
        DrillSegmentFixture {
            segment: SegmentSpec {
                segment_id: SEG_BYPASS_AMBO_ID,
                origin_node_id: NODE_HUANUCO_ID,
                destination_node_id: NODE_AMBO_ID,
                direction_type: DirectionType::Bidirectional,
                length_km: dec("55.00"),
                max_slope_forward_pct: dec("18.0"),
                max_slope_reverse_pct: Some(dec("18.0")),
                road_width_m: dec("3.10"),
                max_supported_tons: dec("10.00"),
                surface_type: SurfaceType::Gravel,
                base_travel_time_min: dec("95.00"),
                is_topology_verified: true,
                attributes_verified_at: Some(ref_time),
                passability_state: PassabilityState::Open,
                active_confirmed_incident_id: None,
                ivv_score: Some(dec("30.00")),
                ivv_is_stale: false,
            },
            soil_susceptibility_idx: dec("0.25"),
            distance_to_ravine_m: dec("450"),
            is_historical_hotspot: false,
        },
        DrillSegmentFixture {
            segment: SegmentSpec {
                segment_id: SEG_BYPASS_TINGO_ID,
                origin_node_id: NODE_AMBO_ID,
                destination_node_id: NODE_TINGO_MARIA_ID,
                direction_type: DirectionType::Bidirectional,
                length_km: dec("115.00"),
                max_slope_forward_pct: dec("19.0"),
                max_slope_reverse_pct: Some(dec("19.0")),
                road_width_m: dec("3.10"),
                max_supported_tons: dec("10.00"),
                surface_type: SurfaceType::Gravel,
                base_travel_time_min: dec("165.00"),
                is_topology_verified: true,
                attributes_verified_at: Some(ref_time),
                passability_state: PassabilityState::Open,
                active_confirmed_incident_id: None,
                ivv_score: Some(dec("32.00")),
                ivv_is_stale: false,
            },
            soil_susceptibility_idx: dec("0.30"),
            distance_to_ravine_m: dec("420"),
            is_historical_hotspot: false,
        },
    ];

    let transfer_corridors = vec![DrillCorridorFixture {
        corridor: TransferCorridorSpec {
            corridor_id: SIM_TRANSFER_CORRIDOR_ID,
            side_a_node_id: NODE_CARPISH_SUR_ID,
            side_b_node_id: NODE_CARPISH_NORTE_ID,
            length_m: 450,
            transfer_mode: TransferMode::PedestrianSafePath,
            max_safe_ivv: dec("90.00"),
            is_verified_safe: true,
            last_inspected_at: ref_time - Duration::days(3),
            inspection_valid_days: 30,
            requires_revalidation: false,
            hazard_intersects_path: false,
            sector_ivv: Some(dec("42.00")),
            sector_ivv_is_stale: false,
            capabilities: vec![paradixe_routing::TransferCapabilitySpec {
                cargo_type: paradixe_domain::CargoType::PerishableAgri,
                max_payload_kg: dec("2000.00"),
                requires_medical_supervision: false,
                estimated_transfer_time_min: dec("25.00"),
            }],
        },
        // Safely offset (~1.5 km west) from Carpish main hazard center (-9.712, -76.091)
        // when hazard_radius_m <= 500, while still intersecting if a huge radius is tested.
        path_wkt: "LINESTRING(-76.108 -9.712, -76.108 -9.708)".to_string(),
    }];

    let payload = DrillFixturePayload {
        fixture_tag: fixture_tag.to_string(),
        reference_time: ref_time,
        nodes,
        segments,
        transfer_corridors,
    };

    let bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    let hash = hex::encode(Sha256::digest(&bytes));
    Ok((payload, hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drill_fixture_sha256_is_deterministic() {
        let (_, hash1) = build_drill_fixture(DRILL_FIXTURE_TAG).unwrap();
        let (_, hash2) = build_drill_fixture(DRILL_FIXTURE_TAG).unwrap();
        assert_eq!(hash1.len(), 64);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn drill_fixture_rejects_unknown_tag() {
        assert!(build_drill_fixture("unknown-tag").is_err());
    }
}
