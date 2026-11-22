#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Rounds a decimal value to `dp` decimal places using PostgreSQL-compatible
/// `NUMERIC` rounding (`MidpointAwayFromZero`).
pub fn round_half_away_from_zero(value: Decimal, dp: u32) -> Decimal {
    value.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    Green,
    Amber,
    Red,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PassabilityState {
    Open,
    CautionSuspected,
    PartialOneLane,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProposedPassability {
    Blocked,
    PartialOneLane,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationState {
    Suspected,
    Confirmed,
    Dismissed,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Direction {
    OriginToDest,
    DestToOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectionType {
    OneWay,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SurfaceType {
    Paved,
    Gravel,
    DirtTrack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VehicleCategoryCode {
    #[serde(rename = "AMBULANCE_4X4")]
    Ambulance4x4,
    #[serde(rename = "PICKUP_4X4")]
    Pickup4x4,
    LightTruck,
    HeavyTrailer,
    Motorcycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CargoType {
    MedicalPatient,
    MedicalSupplies,
    PerishableAgri,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransferMode {
    PedestrianSafePath,
    ShuttleRelay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MitigationStatus {
    Mitigated,
    UnmitigatedNoSafeRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NoSafeRouteReasonCode {
    UnverifiedPhysicalAttributes,
    StaleIvv,
    VehicleRestriction,
    TransferRevalidationRequired,
    TransferInspectionExpired,
    HazardIntersectsTransferPath,
    TransferIvvExceeded,
    TransferCargoCapacityExceeded,
    TransferCargoNotAllowed,
    NoVerifiedTransferCorridor,
    NetworkDisconnected,
}

impl NoSafeRouteReasonCode {
    /// Returns the deterministic diagnostic priority defined in the specification
    /// (lower number = higher precedence).
    pub fn precedence_rank(&self) -> u8 {
        match self {
            Self::UnverifiedPhysicalAttributes => 1,
            Self::StaleIvv => 2,
            Self::VehicleRestriction => 3,
            Self::TransferRevalidationRequired => 4,
            Self::TransferInspectionExpired => 5,
            Self::HazardIntersectsTransferPath => 6,
            Self::TransferIvvExceeded => 7,
            Self::TransferCargoCapacityExceeded => 8,
            Self::TransferCargoNotAllowed => 9,
            Self::NoVerifiedTransferCorridor => 10,
            Self::NetworkDisconnected => 11,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteHop {
    pub segment_id: Uuid,
    pub direction: Direction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeProposal {
    pub corridor_id: Uuid,
    pub side_a_node_id: Uuid,
    pub side_b_node_id: Uuid,
    pub transfer_mode: TransferMode,
    pub length_m: u32,
    pub last_inspected_at: DateTime<Utc>,
    pub inspection_expires_at: DateTime<Utc>,
    pub operational_authorization: String,
    pub estimated_transfer_time_min: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "resolution_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingResolution {
    AlternateRoute {
        routing_config_tag: String,
        total_distance_km: Decimal,
        estimated_time_min: Decimal,
        max_ivv_on_route: Decimal,
        hops: Vec<RouteHop>,
        eta_quality: String,
        operational_authorization: String,
        #[serde(default)]
        warnings: Vec<String>,
    },
    LogisticBridgeRequired {
        blocking_incident_id: Uuid,
        bridge_proposal: BridgeProposal,
        #[serde(default)]
        warnings: Vec<String>,
    },
    NoSafeRoute {
        blocking_incident_id: Option<Uuid>,
        reason_code: NoSafeRouteReasonCode,
        #[serde(default)]
        warnings: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "event_type",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum SimEventRequest {
    WeatherOverride {
        segment_id: Uuid,
        precip_24h_mm: Decimal,
        precip_72h_mm: Decimal,
    },
    BlockageOverride {
        segment_id: Uuid,
        simulated_passability: ProposedPassability,
        hazard_lat: f64,
        hazard_lon: f64,
        hazard_radius_m: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn rounding_matches_postgres_half_away_from_zero() {
        assert_eq!(
            round_half_away_from_zero(Decimal::from_str("12.345").unwrap(), 2),
            Decimal::from_str("12.35").unwrap()
        );
        assert_eq!(
            round_half_away_from_zero(Decimal::from_str("-12.345").unwrap(), 2),
            Decimal::from_str("-12.35").unwrap()
        );
    }

    #[test]
    fn sim_event_rejects_mixed_variant_fields() {
        let mixed = serde_json::json!({
            "segment_id": Uuid::new_v4(),
            "event_type": "WEATHER_OVERRIDE",
            "precip_24h_mm": 85.0,
            "precip_72h_mm": 190.0,
            "simulated_passability": "BLOCKED"
        });
        assert!(serde_json::from_value::<SimEventRequest>(mixed).is_err());
    }

    #[test]
    fn reason_code_precedence_is_strict() {
        let mut codes = vec![
            NoSafeRouteReasonCode::NetworkDisconnected,
            NoSafeRouteReasonCode::StaleIvv,
            NoSafeRouteReasonCode::UnverifiedPhysicalAttributes,
        ];
        codes.sort_by_key(NoSafeRouteReasonCode::precedence_rank);
        assert_eq!(
            codes,
            vec![
                NoSafeRouteReasonCode::UnverifiedPhysicalAttributes,
                NoSafeRouteReasonCode::StaleIvv,
                NoSafeRouteReasonCode::NetworkDisconnected,
            ]
        );
    }

    #[test]
    fn vehicle_category_codes_match_openapi_exact_names() {
        let expected = [
            (VehicleCategoryCode::Ambulance4x4, "\"AMBULANCE_4X4\""),
            (VehicleCategoryCode::Pickup4x4, "\"PICKUP_4X4\""),
            (VehicleCategoryCode::LightTruck, "\"LIGHT_TRUCK\""),
            (VehicleCategoryCode::HeavyTrailer, "\"HEAVY_TRAILER\""),
            (VehicleCategoryCode::Motorcycle, "\"MOTORCYCLE\""),
        ];
        for (variant, json_str) in expected {
            assert_eq!(serde_json::to_string(&variant).unwrap(), json_str);
            assert_eq!(
                serde_json::from_str::<VehicleCategoryCode>(json_str).unwrap(),
                variant
            );
        }
    }
}
