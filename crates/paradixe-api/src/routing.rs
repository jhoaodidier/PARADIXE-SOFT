#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Duration, Utc};
use paradixe_domain::{
    CargoType, Direction, DirectionType, NoSafeRouteReasonCode, PassabilityState, RouteHop,
    RoutingResolution, SurfaceType, TransferMode, VehicleCategoryCode,
};
use paradixe_impact::{evaluate_impact, ImpactInput, ImpactParameters};
use paradixe_ivv::evaluate_ivv_freshness;
use paradixe_routing::{
    calculate_route, HumanitarianWindowSpec, NodeSpec, RoutingConfig, RoutingInput, RoutingRequest,
    SegmentSpec, TransferCapabilitySpec, TransferCorridorSpec, VehicleCategorySpec,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    auth::{write_audit_log, AuthUser, RoleCode},
    error::{ApiError, ValidatedJson, ValidatedPath},
    events::{dispatch_corridor_alerts, publish_operational_event},
    sync::append_sync_change,
    AppState,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingCalculateRequest {
    pub origin_node_id: Uuid,
    pub destination_node_id: Uuid,
    pub vehicle_category_code: VehicleCategoryCode,
    pub cargo_type: CargoType,
    pub cargo_payload_kg: Option<Decimal>,
    pub patient_count: Option<u8>,
}

#[derive(Debug, Serialize)]
pub struct BridgeProposalDto {
    pub corridor_id: Uuid,
    pub side_a_node_id: Uuid,
    pub side_b_node_id: Uuid,
    pub transfer_mode: TransferMode,
    pub length_m: u32,
    pub last_inspected_at: DateTime<Utc>,
    pub inspection_expires_at: DateTime<Utc>,
    pub operational_authorization: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub estimated_transfer_time_min: Decimal,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteEconomicImpactDto {
    pub estimated_co2_kg: f64,
    pub baseline_blockage_co2_kg: f64,
    pub co2_delta_kg: f64,
    pub estimated_cargo_loss_avoided_usd: f64,
    pub time_saved_vs_blockage_min: f64,
    pub reference_blockage_hours: f64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "resolution_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingCalculateResponseDto {
    AlternateRoute {
        routing_config_tag: String,
        #[serde(with = "rust_decimal::serde::float")]
        total_distance_km: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        estimated_time_min: Decimal,
        #[serde(with = "rust_decimal::serde::float")]
        max_ivv_on_route: Decimal,
        hops: Vec<RouteHop>,
        eta_quality: String,
        operational_authorization: String,
        warnings: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        economic_impact: Option<RouteEconomicImpactDto>,
    },
    LogisticBridgeRequired {
        blocking_incident_id: Uuid,
        bridge_proposal: BridgeProposalDto,
        warnings: Vec<String>,
    },
    NoSafeRoute {
        blocking_incident_id: Option<Uuid>,
        reason_code: NoSafeRouteReasonCode,
        warnings: Vec<String>,
    },
}

impl From<RoutingResolution> for RoutingCalculateResponseDto {
    fn from(res: RoutingResolution) -> Self {
        match res {
            RoutingResolution::AlternateRoute {
                routing_config_tag,
                total_distance_km,
                estimated_time_min,
                max_ivv_on_route,
                hops,
                eta_quality,
                operational_authorization,
                warnings,
            } => Self::AlternateRoute {
                routing_config_tag,
                total_distance_km,
                estimated_time_min,
                max_ivv_on_route,
                hops,
                eta_quality,
                operational_authorization,
                warnings,
                economic_impact: None,
            },
            RoutingResolution::LogisticBridgeRequired {
                blocking_incident_id,
                bridge_proposal,
                warnings,
            } => Self::LogisticBridgeRequired {
                blocking_incident_id,
                bridge_proposal: BridgeProposalDto {
                    corridor_id: bridge_proposal.corridor_id,
                    side_a_node_id: bridge_proposal.side_a_node_id,
                    side_b_node_id: bridge_proposal.side_b_node_id,
                    transfer_mode: bridge_proposal.transfer_mode,
                    length_m: bridge_proposal.length_m,
                    last_inspected_at: bridge_proposal.last_inspected_at,
                    inspection_expires_at: bridge_proposal.inspection_expires_at,
                    operational_authorization: bridge_proposal.operational_authorization,
                    estimated_transfer_time_min: bridge_proposal.estimated_transfer_time_min,
                },
                warnings,
            },
            RoutingResolution::NoSafeRoute {
                blocking_incident_id,
                reason_code,
                warnings,
            } => Self::NoSafeRoute {
                blocking_incident_id,
                reason_code,
                warnings,
            },
        }
    }
}

pub fn vehicle_code_str(code: VehicleCategoryCode) -> &'static str {
    match code {
        VehicleCategoryCode::Ambulance4x4 => "AMBULANCE_4X4",
        VehicleCategoryCode::Pickup4x4 => "PICKUP_4X4",
        VehicleCategoryCode::LightTruck => "LIGHT_TRUCK",
        VehicleCategoryCode::HeavyTrailer => "HEAVY_TRAILER",
        VehicleCategoryCode::Motorcycle => "MOTORCYCLE",
    }
}

pub fn parse_vehicle_code(raw: &str) -> Result<VehicleCategoryCode, ApiError> {
    match raw {
        "AMBULANCE_4X4" => Ok(VehicleCategoryCode::Ambulance4x4),
        "PICKUP_4X4" => Ok(VehicleCategoryCode::Pickup4x4),
        "LIGHT_TRUCK" => Ok(VehicleCategoryCode::LightTruck),
        "HEAVY_TRAILER" => Ok(VehicleCategoryCode::HeavyTrailer),
        "MOTORCYCLE" => Ok(VehicleCategoryCode::Motorcycle),
        _ => Err(ApiError::validation("Unknown vehicle_category_code")),
    }
}

pub fn parse_cargo_type(raw: &str) -> Result<CargoType, ApiError> {
    match raw {
        "MEDICAL_PATIENT" => Ok(CargoType::MedicalPatient),
        "MEDICAL_SUPPLIES" => Ok(CargoType::MedicalSupplies),
        "PERISHABLE_AGRI" => Ok(CargoType::PerishableAgri),
        _ => Err(ApiError::validation("Unknown cargo_type")),
    }
}

pub fn cargo_type_str(c: CargoType) -> &'static str {
    match c {
        CargoType::MedicalPatient => "MEDICAL_PATIENT",
        CargoType::MedicalSupplies => "MEDICAL_SUPPLIES",
        CargoType::PerishableAgri => "PERISHABLE_AGRI",
    }
}

pub fn validate_cargo_request(
    cargo_type: CargoType,
    cargo_payload_kg: Option<Decimal>,
    patient_count: Option<u8>,
) -> Result<(), ApiError> {
    match cargo_type {
        CargoType::MedicalPatient => {
            if cargo_payload_kg.is_some() {
                return Err(ApiError::validation(
                    "cargo_payload_kg must be omitted for MEDICAL_PATIENT",
                ));
            }
            if patient_count != Some(1) {
                return Err(ApiError::validation(
                    "patient_count must be 1 for MEDICAL_PATIENT",
                ));
            }
        }
        CargoType::MedicalSupplies | CargoType::PerishableAgri => {
            if let Some(kg) = cargo_payload_kg {
                if kg <= Decimal::ZERO {
                    return Err(ApiError::validation("cargo_payload_kg must be > 0"));
                }
            }
        }
    }
    Ok(())
}

pub async fn load_vehicle_spec(
    pool: &PgPool,
    code: VehicleCategoryCode,
) -> Result<VehicleCategorySpec, ApiError> {
    let row = sqlx::query(
        "SELECT max_slope_pct, min_road_width_m, max_weight_tons, allows_dirt_track
         FROM vehicle_categories
         WHERE code = $1",
    )
    .bind(vehicle_code_str(code))
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::validation("Vehicle category not seeded"))?;

    Ok(VehicleCategorySpec {
        code,
        max_slope_pct: row.try_get("max_slope_pct").map_err(ApiError::from_db)?,
        min_road_width_m: row.try_get("min_road_width_m").map_err(ApiError::from_db)?,
        max_weight_tons: row.try_get("max_weight_tons").map_err(ApiError::from_db)?,
        allows_dirt_track: row
            .try_get("allows_dirt_track")
            .map_err(ApiError::from_db)?,
    })
}

pub async fn load_active_routing_config(pool: &PgPool) -> Result<RoutingConfig, ApiError> {
    let row = sqlx::query(
        "SELECT version_tag, alpha_ivv_weight, beta_suspected_penalty
         FROM routing_config_versions
         WHERE is_active = TRUE
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::internal("No active routing_config_versions row"))?;

    Ok(RoutingConfig {
        version_tag: row.try_get("version_tag").map_err(ApiError::from_db)?,
        alpha_ivv_weight: row.try_get("alpha_ivv_weight").map_err(ApiError::from_db)?,
        beta_suspected_penalty: row
            .try_get("beta_suspected_penalty")
            .map_err(ApiError::from_db)?,
        allow_synthetic_attributes: false,
    })
}

fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

async fn load_endpoint_bbox(
    pool: &PgPool,
    origin_node_id: Uuid,
    destination_node_id: Uuid,
) -> Result<Option<(f64, f64, f64, f64)>, ApiError> {
    let rows = sqlx::query(
        "SELECT ST_X(location_geom) AS lon, ST_Y(location_geom) AS lat
         FROM road_nodes
         WHERE node_id IN ($1, $2)",
    )
    .bind(origin_node_id)
    .bind(destination_node_id)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from_db)?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut min_lon = f64::INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    for r in rows {
        let lon: f64 = r.try_get("lon").map_err(ApiError::from_db)?;
        let lat: f64 = r.try_get("lat").map_err(ApiError::from_db)?;
        min_lon = min_lon.min(lon);
        min_lat = min_lat.min(lat);
        max_lon = max_lon.max(lon);
        max_lat = max_lat.max(lat);
    }
    Ok(Some((min_lon, min_lat, max_lon, max_lat)))
}

async fn load_pruned_graph_for_routing(
    pool: &PgPool,
    bbox: Option<(f64, f64, f64, f64)>,
    margin_deg: f64,
    origin_node_id: Uuid,
    destination_node_id: Uuid,
    extra_node_ids: &[Uuid],
    evaluation_time: DateTime<Utc>,
) -> Result<(Vec<NodeSpec>, Vec<SegmentSpec>), ApiError> {
    let (has_bbox, min_lon, min_lat, max_lon, max_lat) = match bbox {
        Some((x0, y0, x1, y1)) => (
            true,
            (x0 - margin_deg).max(-180.0),
            (y0 - margin_deg).max(-90.0),
            (x1 + margin_deg).min(180.0),
            (y1 + margin_deg).min(90.0),
        ),
        None => (false, 0.0, 0.0, 0.0, 0.0),
    };

    let seg_rows = sqlx::query(
        "SELECT
            rs.segment_id, rs.origin_node_id, rs.destination_node_id, rs.direction_type,
            rs.length_km, rs.max_slope_forward_pct, rs.max_slope_reverse_pct,
            rs.road_width_m, rs.max_supported_tons, rs.surface_type, rs.base_travel_time_min,
            rs.is_topology_verified, rs.attributes_verified_at,
            active_inc.incident_id AS active_incident_id,
            active_inc.verification_state AS active_verification_state,
            active_inc.impact_passability AS active_passability,
            latest_ivv.ivv_score,
            latest_ivv.calculated_at,
            latest_ivv.validity_hours
         FROM road_segments rs
         LEFT JOIN LATERAL (
            SELECT i.incident_id, i.verification_state, i.impact_passability
            FROM incidents i
            WHERE i.segment_id = rs.segment_id
              AND i.verification_state IN ('SUSPECTED', 'CONFIRMED')
            ORDER BY CASE
                WHEN i.verification_state = 'CONFIRMED' AND i.impact_passability = 'BLOCKED' THEN 1
                WHEN i.verification_state = 'CONFIRMED' AND i.impact_passability = 'PARTIAL_ONE_LANE' THEN 2
                WHEN i.verification_state = 'SUSPECTED' THEN 3
                ELSE 4
            END ASC, i.detected_at DESC
            LIMIT 1
         ) active_inc ON TRUE
         LEFT JOIN LATERAL (
            SELECT ia.ivv_score, ia.calculated_at, imv.validity_hours
            FROM weather_observations wo
            JOIN ivv_assessments ia ON ia.observation_id = wo.observation_id
            JOIN ivv_model_versions imv ON imv.model_version_id = ia.model_version_id
            WHERE wo.segment_id = rs.segment_id
            ORDER BY wo.observed_at DESC, ia.calculated_at DESC
            LIMIT 1
         ) latest_ivv ON TRUE
         WHERE NOT $1 OR rs.geom && ST_MakeEnvelope($2, $3, $4, $5, 4326)",
    )
    .bind(has_bbox)
    .bind(min_lon)
    .bind(min_lat)
    .bind(max_lon)
    .bind(max_lat)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut needed_nodes: HashSet<Uuid> = HashSet::with_capacity(seg_rows.len() * 2 + 4);
    needed_nodes.insert(origin_node_id);
    needed_nodes.insert(destination_node_id);
    for id in extra_node_ids {
        needed_nodes.insert(*id);
    }

    let mut segments = Vec::with_capacity(seg_rows.len());
    for r in seg_rows {
        let orig_id: Uuid = r.try_get("origin_node_id").map_err(ApiError::from_db)?;
        let dest_id: Uuid = r
            .try_get("destination_node_id")
            .map_err(ApiError::from_db)?;
        needed_nodes.insert(orig_id);
        needed_nodes.insert(dest_id);

        let dir_str: String = r.try_get("direction_type").map_err(ApiError::from_db)?;
        let direction_type = match dir_str.as_str() {
            "ONE_WAY" => DirectionType::OneWay,
            "BIDIRECTIONAL" => DirectionType::Bidirectional,
            _ => return Err(ApiError::internal("Invalid direction_type")),
        };
        let surf_str: String = r.try_get("surface_type").map_err(ApiError::from_db)?;
        let surface_type = match surf_str.as_str() {
            "PAVED" => SurfaceType::Paved,
            "GRAVEL" => SurfaceType::Gravel,
            "DIRT_TRACK" => SurfaceType::DirtTrack,
            _ => return Err(ApiError::internal("Invalid surface_type")),
        };

        let active_pass: Option<String> =
            r.try_get("active_passability").map_err(ApiError::from_db)?;
        let active_verif: Option<String> = r
            .try_get("active_verification_state")
            .map_err(ApiError::from_db)?;
        let active_incident_id: Option<Uuid> =
            r.try_get("active_incident_id").map_err(ApiError::from_db)?;

        let passability_state = match active_pass.as_deref() {
            Some("BLOCKED") => PassabilityState::Blocked,
            Some("PARTIAL_ONE_LANE") => PassabilityState::PartialOneLane,
            Some("CAUTION_SUSPECTED") => PassabilityState::CautionSuspected,
            _ => PassabilityState::Open,
        };
        let active_confirmed_incident_id = if active_verif.as_deref() == Some("CONFIRMED") {
            active_incident_id
        } else {
            None
        };

        let ivv_score: Option<Decimal> = r.try_get("ivv_score").map_err(ApiError::from_db)?;
        let calculated_at: Option<DateTime<Utc>> =
            r.try_get("calculated_at").map_err(ApiError::from_db)?;
        let validity_hours: Option<i16> = r.try_get("validity_hours").map_err(ApiError::from_db)?;
        let ivv_is_stale = match (ivv_score, calculated_at, validity_hours) {
            (Some(_), Some(calc), Some(vh)) => {
                evaluate_ivv_freshness(calc, i64::from(vh), evaluation_time)
            }
            _ => true,
        };

        segments.push(SegmentSpec {
            segment_id: r.try_get("segment_id").map_err(ApiError::from_db)?,
            origin_node_id: orig_id,
            destination_node_id: dest_id,
            direction_type,
            length_km: r.try_get("length_km").map_err(ApiError::from_db)?,
            max_slope_forward_pct: r
                .try_get("max_slope_forward_pct")
                .map_err(ApiError::from_db)?,
            max_slope_reverse_pct: r
                .try_get("max_slope_reverse_pct")
                .map_err(ApiError::from_db)?,
            road_width_m: r.try_get("road_width_m").map_err(ApiError::from_db)?,
            max_supported_tons: r.try_get("max_supported_tons").map_err(ApiError::from_db)?,
            surface_type,
            base_travel_time_min: r
                .try_get("base_travel_time_min")
                .map_err(ApiError::from_db)?,
            is_topology_verified: r
                .try_get("is_topology_verified")
                .map_err(ApiError::from_db)?,
            attributes_verified_at: r
                .try_get("attributes_verified_at")
                .map_err(ApiError::from_db)?,
            passability_state,
            active_confirmed_incident_id,
            ivv_score,
            ivv_is_stale,
        });
    }

    let node_ids: Vec<Uuid> = needed_nodes.into_iter().collect();
    let node_rows = sqlx::query(
        "SELECT node_id, is_geotechnically_stable
         FROM road_nodes
         WHERE node_id = ANY($1::uuid[])",
    )
    .bind(&node_ids)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from_db)?;

    let nodes: Vec<NodeSpec> = node_rows
        .into_iter()
        .map(|r| {
            Ok(NodeSpec {
                node_id: r.try_get("node_id").map_err(ApiError::from_db)?,
                is_geotechnically_stable: r
                    .try_get("is_geotechnically_stable")
                    .map_err(ApiError::from_db)?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;

    Ok((nodes, segments))
}

async fn compute_route_economic_impact(
    pool: &PgPool,
    vehicle_code: VehicleCategoryCode,
    cargo_type: CargoType,
    resolution: &RoutingResolution,
) -> Option<RouteEconomicImpactDto> {
    let RoutingResolution::AlternateRoute {
        total_distance_km,
        estimated_time_min,
        ..
    } = resolution
    else {
        return None;
    };

    let row = sqlx::query(
        "SELECT ipv.version_tag, ipv.co2_idle_kg_per_hour, ipv.co2_drive_kg_per_km,
                ipv.agri_cargo_value_usd, ipv.perishability_loss_rate_per_hour,
                ipv.transfer_handling_loss_pct, ipv.spoilage_grace_hours
         FROM impact_parameter_versions ipv
         JOIN vehicle_categories vc ON vc.category_id = ipv.vehicle_category_id
         WHERE vc.code = $1 AND ipv.is_active = TRUE
         LIMIT 1",
    )
    .bind(vehicle_code_str(vehicle_code))
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let params = ImpactParameters {
        version_tag: row.try_get("version_tag").ok()?,
        co2_idle_kg_per_hour: row.try_get("co2_idle_kg_per_hour").ok()?,
        co2_drive_kg_per_km: row.try_get("co2_drive_kg_per_km").ok()?,
        agri_cargo_value_usd: row.try_get("agri_cargo_value_usd").ok()?,
        perishability_loss_rate_per_hour: row.try_get("perishability_loss_rate_per_hour").ok()?,
        transfer_handling_loss_pct: row.try_get("transfer_handling_loss_pct").ok()?,
        spoilage_grace_hours: row.try_get("spoilage_grace_hours").ok()?,
    };

    // Baseline comparison models a standard 4h blockage wait on the primary trunk.
    let wait_min = Decimal::from(240);
    let impact = evaluate_impact(&ImpactInput {
        params,
        cargo_type,
        baseline_normal_travel_min: *estimated_time_min,
        baseline_wait_min: wait_min,
        baseline_distance_km: *total_distance_km,
        resolution: resolution.clone(),
        bridge_leg_total_distance_km: None,
        bridge_leg_total_travel_min: None,
        mitigated_idle_min: None,
    })
    .ok()?;

    Some(RouteEconomicImpactDto {
        estimated_co2_kg: decimal_to_f64(impact.mitigated.co2_kg?),
        baseline_blockage_co2_kg: decimal_to_f64(impact.baseline.co2_kg),
        co2_delta_kg: decimal_to_f64(impact.deltas.net_co2_delta_kg?),
        estimated_cargo_loss_avoided_usd: decimal_to_f64(impact.deltas.net_agri_savings_usd?),
        time_saved_vs_blockage_min: decimal_to_f64(impact.deltas.time_delta_min?),
        reference_blockage_hours: 4.0,
    })
}

pub async fn calculate_route_handler(
    State(state): State<AppState>,
    _auth: AuthUser,
    ValidatedJson(body): ValidatedJson<RoutingCalculateRequest>,
) -> Result<Json<RoutingCalculateResponseDto>, ApiError> {
    validate_cargo_request(body.cargo_type, body.cargo_payload_kg, body.patient_count)?;

    let evaluation_time = Utc::now();
    let vehicle = load_vehicle_spec(&state.pool, body.vehicle_category_code).await?;
    let config = load_active_routing_config(&state.pool).await?;

    let endpoint_bbox =
        load_endpoint_bbox(&state.pool, body.origin_node_id, body.destination_node_id).await?;

    let win_rows = sqlx::query(
        "SELECT i.segment_id, vc.code AS vehicle_code, hw.direction, hw.starts_at, hw.ends_at
         FROM humanitarian_windows hw
         JOIN incidents i ON i.incident_id = hw.incident_id
         JOIN vehicle_categories vc ON vc.category_id = hw.vehicle_category_id
         WHERE hw.status IN ('SCHEDULED', 'ACTIVE')
           AND i.verification_state = 'CONFIRMED'",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut humanitarian_windows = Vec::with_capacity(win_rows.len());
    for r in win_rows {
        let dir_str: String = r.try_get("direction").map_err(ApiError::from_db)?;
        let direction = match dir_str.as_str() {
            "ORIGIN_TO_DEST" => Direction::OriginToDest,
            "DEST_TO_ORIGIN" => Direction::DestToOrigin,
            _ => return Err(ApiError::internal("Invalid humanitarian window direction")),
        };
        let vc_str: String = r.try_get("vehicle_code").map_err(ApiError::from_db)?;
        humanitarian_windows.push(HumanitarianWindowSpec {
            segment_id: r.try_get("segment_id").map_err(ApiError::from_db)?,
            vehicle_category_code: parse_vehicle_code(&vc_str)?,
            direction,
            starts_at: r.try_get("starts_at").map_err(ApiError::from_db)?,
            ends_at: r.try_get("ends_at").map_err(ApiError::from_db)?,
        });
    }

    let corr_rows = sqlx::query(
        "SELECT
            tc.corridor_id, tc.side_a_node_id, tc.side_b_node_id, tc.length_m,
            tc.transfer_mode, tc.max_safe_ivv, tc.is_verified_safe, tc.last_inspected_at,
            tc.inspection_valid_days, tc.requires_revalidation,
            EXISTS (
                SELECT 1
                FROM incidents i
                WHERE i.verification_state IN ('SUSPECTED', 'CONFIRMED')
                  AND ST_DWithin(
                      tc.path_geom::geography,
                      i.hazard_location_geom::geography,
                      i.hazard_radius_m
                  )
            ) AS hazard_intersects_path,
            sector.ivv_score AS sector_ivv,
            sector.calculated_at AS sector_calc_at,
            sector.validity_hours AS sector_validity_hours
         FROM transfer_corridors tc
         LEFT JOIN LATERAL (
            SELECT latest.ivv_score, latest.calculated_at, latest.validity_hours
            FROM road_segments rs
            JOIN LATERAL (
                SELECT ia.ivv_score, ia.calculated_at, imv.validity_hours
                FROM weather_observations wo
                JOIN ivv_assessments ia ON ia.observation_id = wo.observation_id
                JOIN ivv_model_versions imv ON imv.model_version_id = ia.model_version_id
                WHERE wo.segment_id = rs.segment_id
                ORDER BY wo.observed_at DESC, ia.calculated_at DESC
                LIMIT 1
            ) latest ON TRUE
            WHERE rs.origin_node_id IN (tc.side_a_node_id, tc.side_b_node_id)
               OR rs.destination_node_id IN (tc.side_a_node_id, tc.side_b_node_id)
               OR ST_DWithin(tc.path_geom::geography, rs.geom::geography, 500)
            ORDER BY latest.ivv_score DESC
            LIMIT 1
         ) sector ON TRUE",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let cap_rows = sqlx::query(
        "SELECT corridor_id, cargo_type, max_payload_kg, requires_medical_supervision, estimated_transfer_time_min
         FROM transfer_corridor_capabilities",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut caps_by_corridor: HashMap<Uuid, Vec<TransferCapabilitySpec>> = HashMap::new();
    for cap in cap_rows {
        let corridor_id: Uuid = cap.try_get("corridor_id").map_err(ApiError::from_db)?;
        let ctype_str: String = cap.try_get("cargo_type").map_err(ApiError::from_db)?;
        caps_by_corridor
            .entry(corridor_id)
            .or_default()
            .push(TransferCapabilitySpec {
                cargo_type: parse_cargo_type(&ctype_str)?,
                max_payload_kg: cap.try_get("max_payload_kg").map_err(ApiError::from_db)?,
                requires_medical_supervision: cap
                    .try_get("requires_medical_supervision")
                    .map_err(ApiError::from_db)?,
                estimated_transfer_time_min: cap
                    .try_get("estimated_transfer_time_min")
                    .map_err(ApiError::from_db)?,
            });
    }

    let mut corridor_node_ids = Vec::new();
    let mut transfer_corridors = Vec::with_capacity(corr_rows.len());
    for r in corr_rows {
        let corridor_id: Uuid = r.try_get("corridor_id").map_err(ApiError::from_db)?;
        let side_a: Uuid = r.try_get("side_a_node_id").map_err(ApiError::from_db)?;
        let side_b: Uuid = r.try_get("side_b_node_id").map_err(ApiError::from_db)?;
        corridor_node_ids.push(side_a);
        corridor_node_ids.push(side_b);

        let mode_str: String = r.try_get("transfer_mode").map_err(ApiError::from_db)?;
        let transfer_mode = match mode_str.as_str() {
            "PEDESTRIAN_SAFE_PATH" => TransferMode::PedestrianSafePath,
            "SHUTTLE_RELAY" => TransferMode::ShuttleRelay,
            _ => return Err(ApiError::internal("Invalid transfer_mode")),
        };
        let length_m: i32 = r.try_get("length_m").map_err(ApiError::from_db)?;
        let valid_days: i16 = r
            .try_get("inspection_valid_days")
            .map_err(ApiError::from_db)?;
        let sector_ivv: Option<Decimal> = r.try_get("sector_ivv").map_err(ApiError::from_db)?;
        let sector_calc: Option<DateTime<Utc>> =
            r.try_get("sector_calc_at").map_err(ApiError::from_db)?;
        let sector_vh: Option<i16> = r
            .try_get("sector_validity_hours")
            .map_err(ApiError::from_db)?;
        let sector_ivv_is_stale = match (sector_ivv, sector_calc, sector_vh) {
            (Some(_), Some(calc), Some(vh)) => {
                evaluate_ivv_freshness(calc, i64::from(vh), evaluation_time)
            }
            _ => true,
        };

        transfer_corridors.push(TransferCorridorSpec {
            corridor_id,
            side_a_node_id: side_a,
            side_b_node_id: side_b,
            length_m: u32::try_from(length_m).unwrap_or(0),
            transfer_mode,
            max_safe_ivv: r.try_get("max_safe_ivv").map_err(ApiError::from_db)?,
            is_verified_safe: r.try_get("is_verified_safe").map_err(ApiError::from_db)?,
            last_inspected_at: r.try_get("last_inspected_at").map_err(ApiError::from_db)?,
            inspection_valid_days: i64::from(valid_days),
            requires_revalidation: r
                .try_get("requires_revalidation")
                .map_err(ApiError::from_db)?,
            hazard_intersects_path: r
                .try_get("hazard_intersects_path")
                .map_err(ApiError::from_db)?,
            sector_ivv,
            sector_ivv_is_stale,
            capabilities: caps_by_corridor.remove(&corridor_id).unwrap_or_default(),
        });
    }

    let request = RoutingRequest {
        origin_node_id: body.origin_node_id,
        destination_node_id: body.destination_node_id,
        vehicle_category_code: body.vehicle_category_code,
        cargo_type: body.cargo_type,
        cargo_payload_kg: body.cargo_payload_kg,
        patient_count: body.patient_count,
    };

    let (nodes, segments) = load_pruned_graph_for_routing(
        &state.pool,
        endpoint_bbox,
        0.40,
        body.origin_node_id,
        body.destination_node_id,
        &corridor_node_ids,
        evaluation_time,
    )
    .await?;

    let mut resolution = calculate_route(&RoutingInput {
        vehicle: vehicle.clone(),
        nodes,
        segments,
        transfer_corridors: transfer_corridors.clone(),
        humanitarian_windows: humanitarian_windows.clone(),
        config: config.clone(),
        request: request.clone(),
        evaluation_time,
    });

    if matches!(
        resolution,
        RoutingResolution::NoSafeRoute {
            reason_code: NoSafeRouteReasonCode::NetworkDisconnected
                | NoSafeRouteReasonCode::NoVerifiedTransferCorridor,
            ..
        }
    ) && endpoint_bbox.is_some()
    {
        let (wide_nodes, wide_segments) = load_pruned_graph_for_routing(
            &state.pool,
            endpoint_bbox,
            1.60,
            body.origin_node_id,
            body.destination_node_id,
            &corridor_node_ids,
            evaluation_time,
        )
        .await?;
        resolution = calculate_route(&RoutingInput {
            vehicle,
            nodes: wide_nodes,
            segments: wide_segments,
            transfer_corridors,
            humanitarian_windows,
            config,
            request,
            evaluation_time,
        });
    }

    state.metrics.routing_total.fetch_add(1, Ordering::Relaxed);
    match &resolution {
        RoutingResolution::AlternateRoute { .. } => {
            state
                .metrics
                .routing_alternate
                .fetch_add(1, Ordering::Relaxed);
        }
        RoutingResolution::LogisticBridgeRequired { .. } => {
            state.metrics.routing_bridge.fetch_add(1, Ordering::Relaxed);
        }
        RoutingResolution::NoSafeRoute { .. } => {
            state
                .metrics
                .routing_no_safe
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    let econ_impact = compute_route_economic_impact(
        &state.pool,
        body.vehicle_category_code,
        body.cargo_type,
        &resolution,
    )
    .await;

    let mut dto = RoutingCalculateResponseDto::from(resolution);
    if let RoutingCalculateResponseDto::AlternateRoute {
        ref mut economic_impact,
        ..
    } = dto
    {
        *economic_impact = econ_impact;
    }

    Ok(Json(dto))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBridgeRequest {
    pub incident_id: Uuid,
    pub corridor_id: Uuid,
    pub cargo_type: CargoType,
}

#[derive(Debug, Serialize)]
pub struct LogisticBridgeResponse {
    pub bridge_id: Uuid,
    pub incident_id: Uuid,
    pub corridor_id: Uuid,
    pub cargo_type: CargoType,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

async fn verify_corridor_ready_for_bridge(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    corridor_id: Uuid,
    cargo_type: CargoType,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let corr = sqlx::query(
        "SELECT is_verified_safe, requires_revalidation, last_inspected_at, inspection_valid_days,
                EXISTS (
                    SELECT 1 FROM incidents i
                    WHERE i.verification_state IN ('SUSPECTED', 'CONFIRMED')
                      AND ST_DWithin(
                          transfer_corridors.path_geom::geography,
                          i.hazard_location_geom::geography,
                          i.hazard_radius_m
                      )
                ) AS hazard_intersects
         FROM transfer_corridors
         WHERE corridor_id = $1",
    )
    .bind(corridor_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| {
        ApiError::validation("Transfer corridor does not exist in operational catalog")
    })?;

    let is_verified_safe: bool = corr
        .try_get("is_verified_safe")
        .map_err(ApiError::from_db)?;
    let requires_revalidation: bool = corr
        .try_get("requires_revalidation")
        .map_err(ApiError::from_db)?;
    let last_inspected_at: DateTime<Utc> = corr
        .try_get("last_inspected_at")
        .map_err(ApiError::from_db)?;
    let valid_days: i16 = corr
        .try_get("inspection_valid_days")
        .map_err(ApiError::from_db)?;
    let hazard_intersects: bool = corr
        .try_get("hazard_intersects")
        .map_err(ApiError::from_db)?;

    if !is_verified_safe || requires_revalidation || hazard_intersects {
        return Err(ApiError::validation(
            "Corridor fails verification, revalidation, or hazard safety clearance",
        ));
    }
    if now > last_inspected_at + Duration::days(i64::from(valid_days)) {
        return Err(ApiError::validation(
            "Corridor safety inspection is expired",
        ));
    }

    let cap_exists = sqlx::query(
        "SELECT EXISTS (
            SELECT 1 FROM transfer_corridor_capabilities
            WHERE corridor_id = $1 AND cargo_type = $2
        ) AS ok",
    )
    .bind(corridor_id)
    .bind(cargo_type_str(cargo_type))
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::from_db)?;
    let ok: bool = cap_exists.try_get("ok").map_err(ApiError::from_db)?;
    if !ok {
        return Err(ApiError::validation(
            "Corridor lacks verified capability for requested cargo_type",
        ));
    }
    Ok(())
}

pub async fn propose_logistic_bridge(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateBridgeRequest>,
) -> Result<(StatusCode, Json<LogisticBridgeResponse>), ApiError> {
    auth.require_roles(&[
        RoleCode::CoerAdmin,
        RoleCode::HealthOperator,
        RoleCode::CivilDefense,
    ])?;

    let now = Utc::now();
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let inc_exists = sqlx::query("SELECT 1 FROM incidents WHERE incident_id = $1")
        .bind(body.incident_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;
    if inc_exists.is_none() {
        return Err(ApiError::validation("Unknown incident_id"));
    }

    verify_corridor_ready_for_bridge(&mut tx, body.corridor_id, body.cargo_type, now).await?;

    let bridge_id = Uuid::new_v4();
    let cargo_str = cargo_type_str(body.cargo_type);
    let inserted = sqlx::query(
        "INSERT INTO logistic_bridges (bridge_id, incident_id, corridor_id, cargo_type, status, created_at)
         VALUES ($1, $2, $3, $4, 'PROPOSED', $5)
         RETURNING created_at",
    )
    .bind(bridge_id)
    .bind(body.incident_id)
    .bind(body.corridor_id)
    .bind(cargo_str)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    append_sync_change(
        &mut tx,
        "LOGISTIC_BRIDGE",
        bridge_id,
        "UPSERT",
        Some(json!({
            "bridge_id": bridge_id,
            "incident_id": body.incident_id,
            "corridor_id": body.corridor_id,
            "cargo_type": cargo_str,
            "status": "PROPOSED"
        })),
    )
    .await?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "LOGISTIC_BRIDGE_PROPOSED",
        "LOGISTIC_BRIDGE",
        bridge_id,
        None,
        json!({ "status": "PROPOSED", "corridor_id": body.corridor_id }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;

    publish_operational_event(
        &state,
        "BRIDGE_UPDATED",
        Some(bridge_id),
        None,
        format!("Puente logistico propuesto ({cargo_str})"),
    );

    Ok((
        StatusCode::CREATED,
        Json(LogisticBridgeResponse {
            bridge_id,
            incident_id: body.incident_id,
            corridor_id: body.corridor_id,
            cargo_type: body.cargo_type,
            status: "PROPOSED".to_string(),
            created_at: inserted.try_get("created_at").map_err(ApiError::from_db)?,
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBridgeStatusRequest {
    pub status: String,
}

pub async fn update_bridge_status(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(bridge_id): ValidatedPath<Uuid>,
    ValidatedJson(body): ValidatedJson<UpdateBridgeStatusRequest>,
) -> Result<Json<LogisticBridgeResponse>, ApiError> {
    // HEALTH_OPERATOR cannot unilaterally activate a bridge.
    auth.require_roles(&[RoleCode::CoerAdmin, RoleCode::CivilDefense])?;

    if !matches!(
        body.status.as_str(),
        "ACTIVE" | "SUSPENDED_UNSAFE" | "COMPLETED"
    ) {
        return Err(ApiError::validation("Invalid target bridge status"));
    }

    let now = Utc::now();
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let row = sqlx::query(
        "SELECT incident_id, corridor_id, cargo_type, status, created_at
         FROM logistic_bridges
         WHERE bridge_id = $1
         FOR UPDATE",
    )
    .bind(bridge_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::not_found("Logistic bridge not found"))?;

    let incident_id: Uuid = row.try_get("incident_id").map_err(ApiError::from_db)?;
    let corridor_id: Uuid = row.try_get("corridor_id").map_err(ApiError::from_db)?;
    let cargo_str: String = row.try_get("cargo_type").map_err(ApiError::from_db)?;
    let cargo_type = parse_cargo_type(&cargo_str)?;
    let prev_status: String = row.try_get("status").map_err(ApiError::from_db)?;
    let created_at: DateTime<Utc> = row.try_get("created_at").map_err(ApiError::from_db)?;

    if body.status == "ACTIVE" {
        verify_corridor_ready_for_bridge(&mut tx, corridor_id, cargo_type, now).await?;
    }

    sqlx::query("UPDATE logistic_bridges SET status = $2 WHERE bridge_id = $1")
        .bind(bridge_id)
        .bind(&body.status)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

    append_sync_change(
        &mut tx,
        "LOGISTIC_BRIDGE",
        bridge_id,
        "UPSERT",
        Some(json!({
            "bridge_id": bridge_id,
            "incident_id": incident_id,
            "corridor_id": corridor_id,
            "cargo_type": cargo_str,
            "status": body.status
        })),
    )
    .await?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "LOGISTIC_BRIDGE_STATUS_UPDATED",
        "LOGISTIC_BRIDGE",
        bridge_id,
        Some(json!({ "status": prev_status })),
        json!({ "status": body.status }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;

    publish_operational_event(
        &state,
        "BRIDGE_UPDATED",
        Some(bridge_id),
        None,
        format!("Estado de puente logistico actualizado a {}", body.status),
    );
    if body.status == "ACTIVE" {
        dispatch_corridor_alerts(
            &state,
            "Puente Logistico".to_string(),
            "RED",
            format!("PARADIXE: Puente logistico activo ({cargo_str}) para transbordo habilitado."),
        );
    }

    Ok(Json(LogisticBridgeResponse {
        bridge_id,
        incident_id,
        corridor_id,
        cargo_type,
        status: body.status,
        created_at,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHumanitarianWindowRequest {
    pub vehicle_category_id: Uuid,
    pub direction: Direction,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HumanitarianWindowResponse {
    pub window_id: Uuid,
    pub incident_id: Uuid,
    pub direction: Direction,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: String,
}

pub async fn create_humanitarian_window(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(incident_id): ValidatedPath<Uuid>,
    ValidatedJson(body): ValidatedJson<CreateHumanitarianWindowRequest>,
) -> Result<(StatusCode, Json<HumanitarianWindowResponse>), ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin, RoleCode::CivilDefense])?;

    if body.ends_at <= body.starts_at {
        return Err(ApiError::validation(
            "ends_at must be strictly after starts_at",
        ));
    }

    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;
    let inc = sqlx::query(
        "SELECT verification_state, impact_passability
         FROM incidents
         WHERE incident_id = $1",
    )
    .bind(incident_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::not_found("Incident not found"))?;

    let vstate: String = inc
        .try_get("verification_state")
        .map_err(ApiError::from_db)?;
    let ipass: String = inc
        .try_get("impact_passability")
        .map_err(ApiError::from_db)?;
    if vstate != "CONFIRMED" || ipass != "PARTIAL_ONE_LANE" {
        return Err(ApiError::validation(
            "Humanitarian windows require a CONFIRMED PARTIAL_ONE_LANE incident",
        ));
    }

    let dir_str = match body.direction {
        Direction::OriginToDest => "ORIGIN_TO_DEST",
        Direction::DestToOrigin => "DEST_TO_ORIGIN",
    };
    let now = Utc::now();
    let status = if body.starts_at <= now && now <= body.ends_at {
        "ACTIVE"
    } else {
        "SCHEDULED"
    };

    let window_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO humanitarian_windows (
            window_id, incident_id, authorized_by_user_id, vehicle_category_id, direction, starts_at, ends_at, status
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(window_id)
    .bind(incident_id)
    .bind(auth.user_id)
    .bind(body.vehicle_category_id)
    .bind(dir_str)
    .bind(body.starts_at)
    .bind(body.ends_at)
    .bind(status)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    append_sync_change(
        &mut tx,
        "HUMANITARIAN_WINDOW",
        window_id,
        "UPSERT",
        Some(json!({
            "window_id": window_id,
            "incident_id": incident_id,
            "direction": dir_str,
            "starts_at": body.starts_at,
            "ends_at": body.ends_at,
            "status": status
        })),
    )
    .await?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "HUMANITARIAN_WINDOW_CREATED",
        "HUMANITARIAN_WINDOW",
        window_id,
        None,
        json!({ "incident_id": incident_id, "direction": dir_str, "status": status }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;

    publish_operational_event(
        &state,
        "BRIDGE_UPDATED",
        Some(window_id),
        None,
        format!("Ventana humanitaria {status} ({dir_str})"),
    );
    dispatch_corridor_alerts(
        &state,
        "Ventana Humanitaria".to_string(),
        "AMBER",
        format!(
            "PARADIXE: Ventana humanitaria {status} ({dir_str}) vigente hasta {}",
            body.ends_at.format("%H:%M UTC")
        ),
    );

    Ok((
        StatusCode::CREATED,
        Json(HumanitarianWindowResponse {
            window_id,
            incident_id,
            direction: body.direction,
            starts_at: body.starts_at,
            ends_at: body.ends_at,
            status: status.to_string(),
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct TransferCorridorSummaryDto {
    pub corridor_id: Uuid,
    pub side_a_node_id: Uuid,
    pub side_b_node_id: Uuid,
    pub length_m: i32,
    #[serde(with = "rust_decimal::serde::float")]
    pub max_slope_pct: Decimal,
    pub transfer_mode: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub max_safe_ivv: Decimal,
    pub is_verified_safe: bool,
    pub last_inspected_at: DateTime<Utc>,
    pub inspection_valid_days: i16,
    pub requires_revalidation: bool,
    pub cargo_types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LogisticBridgeSummaryDto {
    pub bridge_id: Uuid,
    pub incident_id: Uuid,
    pub corridor_id: Uuid,
    pub cargo_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HumanitarianWindowSummaryDto {
    pub window_id: Uuid,
    pub incident_id: Uuid,
    pub vehicle_category_id: Uuid,
    pub vehicle_category_code: String,
    pub direction: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct VehicleCategoryDto {
    pub category_id: Uuid,
    pub code: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub max_slope_pct: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub min_road_width_m: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub max_weight_tons: Decimal,
    pub allows_dirt_track: bool,
}

#[derive(Debug, Serialize)]
pub struct BridgesOperationsResponse {
    pub transfer_corridors: Vec<TransferCorridorSummaryDto>,
    pub bridges: Vec<LogisticBridgeSummaryDto>,
    pub humanitarian_windows: Vec<HumanitarianWindowSummaryDto>,
    pub vehicle_categories: Vec<VehicleCategoryDto>,
}

pub async fn list_bridges_and_windows(
    State(state): State<AppState>,
) -> Result<Json<BridgesOperationsResponse>, ApiError> {
    let cap_rows = sqlx::query(
        "SELECT corridor_id, cargo_type FROM transfer_corridor_capabilities ORDER BY cargo_type ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut caps_map: HashMap<Uuid, Vec<String>> = HashMap::new();
    for r in cap_rows {
        let cid: Uuid = r.try_get("corridor_id").map_err(ApiError::from_db)?;
        let ctype: String = r.try_get("cargo_type").map_err(ApiError::from_db)?;
        caps_map.entry(cid).or_default().push(ctype);
    }

    let corr_rows = sqlx::query(
        "SELECT corridor_id, side_a_node_id, side_b_node_id, length_m, max_slope_pct,
                transfer_mode, max_safe_ivv, is_verified_safe, last_inspected_at,
                inspection_valid_days, requires_revalidation
         FROM transfer_corridors
         ORDER BY last_inspected_at DESC, corridor_id ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut transfer_corridors = Vec::with_capacity(corr_rows.len());
    for r in corr_rows {
        let cid: Uuid = r.try_get("corridor_id").map_err(ApiError::from_db)?;
        transfer_corridors.push(TransferCorridorSummaryDto {
            corridor_id: cid,
            side_a_node_id: r.try_get("side_a_node_id").map_err(ApiError::from_db)?,
            side_b_node_id: r.try_get("side_b_node_id").map_err(ApiError::from_db)?,
            length_m: r.try_get("length_m").map_err(ApiError::from_db)?,
            max_slope_pct: r.try_get("max_slope_pct").map_err(ApiError::from_db)?,
            transfer_mode: r.try_get("transfer_mode").map_err(ApiError::from_db)?,
            max_safe_ivv: r.try_get("max_safe_ivv").map_err(ApiError::from_db)?,
            is_verified_safe: r.try_get("is_verified_safe").map_err(ApiError::from_db)?,
            last_inspected_at: r.try_get("last_inspected_at").map_err(ApiError::from_db)?,
            inspection_valid_days: r
                .try_get("inspection_valid_days")
                .map_err(ApiError::from_db)?,
            requires_revalidation: r
                .try_get("requires_revalidation")
                .map_err(ApiError::from_db)?,
            cargo_types: caps_map.remove(&cid).unwrap_or_default(),
        });
    }

    let bridge_rows = sqlx::query(
        "SELECT bridge_id, incident_id, corridor_id, cargo_type, status, created_at
         FROM logistic_bridges
         ORDER BY created_at DESC, bridge_id ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut bridges = Vec::with_capacity(bridge_rows.len());
    for r in bridge_rows {
        bridges.push(LogisticBridgeSummaryDto {
            bridge_id: r.try_get("bridge_id").map_err(ApiError::from_db)?,
            incident_id: r.try_get("incident_id").map_err(ApiError::from_db)?,
            corridor_id: r.try_get("corridor_id").map_err(ApiError::from_db)?,
            cargo_type: r.try_get("cargo_type").map_err(ApiError::from_db)?,
            status: r.try_get("status").map_err(ApiError::from_db)?,
            created_at: r.try_get("created_at").map_err(ApiError::from_db)?,
        });
    }

    let win_rows = sqlx::query(
        "SELECT hw.window_id, hw.incident_id, hw.vehicle_category_id, vc.code AS vehicle_category_code,
                hw.direction, hw.starts_at, hw.ends_at, hw.status
         FROM humanitarian_windows hw
         JOIN vehicle_categories vc ON vc.category_id = hw.vehicle_category_id
         ORDER BY hw.starts_at DESC, hw.window_id ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut humanitarian_windows = Vec::with_capacity(win_rows.len());
    for r in win_rows {
        humanitarian_windows.push(HumanitarianWindowSummaryDto {
            window_id: r.try_get("window_id").map_err(ApiError::from_db)?,
            incident_id: r.try_get("incident_id").map_err(ApiError::from_db)?,
            vehicle_category_id: r
                .try_get("vehicle_category_id")
                .map_err(ApiError::from_db)?,
            vehicle_category_code: r
                .try_get("vehicle_category_code")
                .map_err(ApiError::from_db)?,
            direction: r.try_get("direction").map_err(ApiError::from_db)?,
            starts_at: r.try_get("starts_at").map_err(ApiError::from_db)?,
            ends_at: r.try_get("ends_at").map_err(ApiError::from_db)?,
            status: r.try_get("status").map_err(ApiError::from_db)?,
        });
    }

    let vc_rows = sqlx::query(
        "SELECT category_id, code, max_slope_pct, min_road_width_m, max_weight_tons, allows_dirt_track
         FROM vehicle_categories
         ORDER BY priority_tier ASC, code ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut vehicle_categories = Vec::with_capacity(vc_rows.len());
    for r in vc_rows {
        vehicle_categories.push(VehicleCategoryDto {
            category_id: r.try_get("category_id").map_err(ApiError::from_db)?,
            code: r.try_get("code").map_err(ApiError::from_db)?,
            max_slope_pct: r.try_get("max_slope_pct").map_err(ApiError::from_db)?,
            min_road_width_m: r.try_get("min_road_width_m").map_err(ApiError::from_db)?,
            max_weight_tons: r.try_get("max_weight_tons").map_err(ApiError::from_db)?,
            allows_dirt_track: r.try_get("allows_dirt_track").map_err(ApiError::from_db)?,
        });
    }

    Ok(Json(BridgesOperationsResponse {
        transfer_corridors,
        bridges,
        humanitarian_windows,
        vehicle_categories,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferCapabilityInput {
    pub cargo_type: CargoType,
    pub max_payload_kg: Decimal,
    pub requires_medical_supervision: bool,
    pub estimated_transfer_time_min: Decimal,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTransferCorridorRequest {
    pub side_a_node_id: Uuid,
    pub side_b_node_id: Uuid,
    pub length_m: i32,
    pub max_slope_pct: Decimal,
    pub transfer_mode: TransferMode,
    pub max_safe_ivv: Decimal,
    pub inspection_valid_days: Option<i16>,
    pub capabilities: Vec<TransferCapabilityInput>,
}

fn transfer_mode_str(mode: TransferMode) -> &'static str {
    match mode {
        TransferMode::PedestrianSafePath => "PEDESTRIAN_SAFE_PATH",
        TransferMode::ShuttleRelay => "SHUTTLE_RELAY",
    }
}

pub async fn create_transfer_corridor(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateTransferCorridorRequest>,
) -> Result<(StatusCode, Json<TransferCorridorSummaryDto>), ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin, RoleCode::CivilDefense])?;

    if body.side_a_node_id == body.side_b_node_id {
        return Err(ApiError::validation(
            "side_a_node_id and side_b_node_id must differ",
        ));
    }
    if body.length_m <= 0 {
        return Err(ApiError::validation("length_m must be > 0"));
    }
    if body.max_slope_pct < Decimal::ZERO {
        return Err(ApiError::validation("max_slope_pct must be >= 0"));
    }
    if body.max_safe_ivv < Decimal::ZERO || body.max_safe_ivv > Decimal::from(100) {
        return Err(ApiError::validation("max_safe_ivv must be in 0..=100"));
    }
    let valid_days = body.inspection_valid_days.unwrap_or(30);
    if !(1..=90).contains(&valid_days) {
        return Err(ApiError::validation("inspection_valid_days must be 1..=90"));
    }
    if body.capabilities.is_empty() {
        return Err(ApiError::validation(
            "At least one cargo capability must be specified",
        ));
    }
    for cap in &body.capabilities {
        if cap.max_payload_kg <= Decimal::ZERO || cap.estimated_transfer_time_min <= Decimal::ZERO {
            return Err(ApiError::validation(
                "max_payload_kg and estimated_transfer_time_min must be > 0",
            ));
        }
    }

    let now = Utc::now();
    let mode_str = transfer_mode_str(body.transfer_mode);
    let corridor_id = Uuid::new_v4();
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let nodes_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM road_nodes WHERE node_id IN ($1, $2)")
            .bind(body.side_a_node_id)
            .bind(body.side_b_node_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;
    if nodes_count != 2 {
        return Err(ApiError::validation(
            "Both side_a_node_id and side_b_node_id must exist in road_nodes",
        ));
    }

    sqlx::query(
        "INSERT INTO transfer_corridors (
            corridor_id, side_a_node_id, side_b_node_id, path_geom, length_m, max_slope_pct,
            transfer_mode, max_safe_ivv, is_verified_safe, last_inspected_at,
            inspection_valid_days, requires_revalidation, verified_by_user_id
         )
         SELECT
            $1, a.node_id, b.node_id,
            ST_MakeLine(a.location_geom, b.location_geom),
            $4, $5, $6, $7, TRUE, $8, $9, FALSE, $10
         FROM road_nodes a, road_nodes b
         WHERE a.node_id = $2 AND b.node_id = $3",
    )
    .bind(corridor_id)
    .bind(body.side_a_node_id)
    .bind(body.side_b_node_id)
    .bind(body.length_m)
    .bind(body.max_slope_pct)
    .bind(mode_str)
    .bind(body.max_safe_ivv)
    .bind(now)
    .bind(valid_days)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    let mut cargo_types = Vec::with_capacity(body.capabilities.len());
    for cap in &body.capabilities {
        let ctype = cargo_type_str(cap.cargo_type);
        sqlx::query(
            "INSERT INTO transfer_corridor_capabilities (
                corridor_id, cargo_type, max_payload_kg, requires_medical_supervision, estimated_transfer_time_min
             ) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (corridor_id, cargo_type) DO UPDATE SET
                max_payload_kg = EXCLUDED.max_payload_kg,
                requires_medical_supervision = EXCLUDED.requires_medical_supervision,
                estimated_transfer_time_min = EXCLUDED.estimated_transfer_time_min",
        )
        .bind(corridor_id)
        .bind(ctype)
        .bind(cap.max_payload_kg)
        .bind(cap.requires_medical_supervision)
        .bind(cap.estimated_transfer_time_min)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;
        cargo_types.push(ctype.to_string());
    }

    append_sync_change(
        &mut tx,
        "TRANSFER_CORRIDOR",
        corridor_id,
        "UPSERT",
        Some(json!({
            "corridor_id": corridor_id,
            "side_a_node_id": body.side_a_node_id,
            "side_b_node_id": body.side_b_node_id,
            "transfer_mode": mode_str,
            "is_verified_safe": true,
            "last_inspected_at": now,
        })),
    )
    .await?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "TRANSFER_CORRIDOR_CREATED",
        "TRANSFER_CORRIDOR",
        corridor_id,
        None,
        json!({
            "transfer_mode": mode_str,
            "length_m": body.length_m,
            "cargo_types": cargo_types,
        }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;

    publish_operational_event(
        &state,
        "BRIDGE_UPDATED",
        Some(corridor_id),
        None,
        format!("Corredor de transbordo certificado ({mode_str})"),
    );

    Ok((
        StatusCode::CREATED,
        Json(TransferCorridorSummaryDto {
            corridor_id,
            side_a_node_id: body.side_a_node_id,
            side_b_node_id: body.side_b_node_id,
            length_m: body.length_m,
            max_slope_pct: body.max_slope_pct,
            transfer_mode: mode_str.to_string(),
            max_safe_ivv: body.max_safe_ivv,
            is_verified_safe: true,
            last_inspected_at: now,
            inspection_valid_days: valid_days,
            requires_revalidation: false,
            cargo_types,
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectTransferCorridorRequest {
    pub is_verified_safe: bool,
    pub inspection_valid_days: Option<i16>,
    pub notes: Option<String>,
}

pub async fn inspect_transfer_corridor(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(corridor_id): ValidatedPath<Uuid>,
    ValidatedJson(body): ValidatedJson<InspectTransferCorridorRequest>,
) -> Result<Json<TransferCorridorSummaryDto>, ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin, RoleCode::CivilDefense])?;

    if let Some(days) = body.inspection_valid_days {
        if !(1..=90).contains(&days) {
            return Err(ApiError::validation("inspection_valid_days must be 1..=90"));
        }
    }

    let now = Utc::now();
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let row = sqlx::query(
        "UPDATE transfer_corridors
         SET is_verified_safe = $2,
             inspection_valid_days = COALESCE($3, inspection_valid_days),
             requires_revalidation = FALSE,
             last_inspected_at = $4,
             verified_by_user_id = $5
         WHERE corridor_id = $1
         RETURNING side_a_node_id, side_b_node_id, length_m, max_slope_pct, transfer_mode,
                   max_safe_ivv, is_verified_safe, last_inspected_at, inspection_valid_days, requires_revalidation",
    )
    .bind(corridor_id)
    .bind(body.is_verified_safe)
    .bind(body.inspection_valid_days)
    .bind(now)
    .bind(auth.user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::not_found("Transfer corridor not found"))?;

    let cap_rows = sqlx::query(
        "SELECT cargo_type FROM transfer_corridor_capabilities WHERE corridor_id = $1 ORDER BY cargo_type ASC",
    )
    .bind(corridor_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;
    let mut cargo_types = Vec::with_capacity(cap_rows.len());
    for cr in cap_rows {
        cargo_types.push(cr.try_get("cargo_type").map_err(ApiError::from_db)?);
    }

    append_sync_change(
        &mut tx,
        "TRANSFER_CORRIDOR",
        corridor_id,
        "UPSERT",
        Some(json!({
            "corridor_id": corridor_id,
            "is_verified_safe": body.is_verified_safe,
            "requires_revalidation": false,
            "last_inspected_at": now,
        })),
    )
    .await?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "TRANSFER_CORRIDOR_INSPECTED",
        "TRANSFER_CORRIDOR",
        corridor_id,
        None,
        json!({
            "is_verified_safe": body.is_verified_safe,
            "inspection_valid_days": body.inspection_valid_days,
            "notes": body.notes,
        }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;

    publish_operational_event(
        &state,
        "BRIDGE_UPDATED",
        Some(corridor_id),
        None,
        format!(
            "Inspeccion de corredor de transbordo ({})",
            if body.is_verified_safe {
                "APROBADO"
            } else {
                "OBSERVADO"
            }
        ),
    );

    Ok(Json(TransferCorridorSummaryDto {
        corridor_id,
        side_a_node_id: row.try_get("side_a_node_id").map_err(ApiError::from_db)?,
        side_b_node_id: row.try_get("side_b_node_id").map_err(ApiError::from_db)?,
        length_m: row.try_get("length_m").map_err(ApiError::from_db)?,
        max_slope_pct: row.try_get("max_slope_pct").map_err(ApiError::from_db)?,
        transfer_mode: row.try_get("transfer_mode").map_err(ApiError::from_db)?,
        max_safe_ivv: row.try_get("max_safe_ivv").map_err(ApiError::from_db)?,
        is_verified_safe: row.try_get("is_verified_safe").map_err(ApiError::from_db)?,
        last_inspected_at: row
            .try_get("last_inspected_at")
            .map_err(ApiError::from_db)?,
        inspection_valid_days: row
            .try_get("inspection_valid_days")
            .map_err(ApiError::from_db)?,
        requires_revalidation: row
            .try_get("requires_revalidation")
            .map_err(ApiError::from_db)?,
        cargo_types,
    }))
}
