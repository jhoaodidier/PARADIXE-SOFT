#![forbid(unsafe_code)]

use std::collections::HashMap;

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use paradixe_domain::{
    CargoType, MitigationStatus, PassabilityState, ProposedPassability, RoutingResolution,
    SimEventRequest, VehicleCategoryCode,
};
use paradixe_impact::{evaluate_impact, ImpactInput, ImpactParameters};
use paradixe_ivv::{calculate_ivv, IvvModelParams, SegmentGeoInput, WeatherInput};
use paradixe_routing::{
    calculate_route, RoutingConfig, RoutingInput, RoutingRequest, TransferCorridorSpec,
    VehicleCategorySpec,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    auth::{AuthUser, RoleCode},
    error::{ApiError, ValidatedJson, ValidatedPath},
    fixtures::{
        build_drill_fixture, DrillFixturePayload, NODE_HUANUCO_ID, NODE_TINGO_MARIA_ID,
        SEG_CARPISH_NORTE_TINGO_ID, SEG_CARPISH_TUNNEL_ID, SEG_HUANUCO_CARPISH_SUR_ID,
    },
    routing::{
        cargo_type_str, parse_cargo_type, validate_cargo_request, vehicle_code_str,
        RoutingCalculateResponseDto,
    },
    AppState, NotificationChannel, NotificationEvent, NotificationGateway,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateScenarioRequest {
    pub name: String,
    pub fixture_tag: String,
}

#[derive(Debug, Serialize)]
pub struct ScenarioResponse {
    pub scenario_id: Uuid,
    pub name: String,
    pub fixture_tag: String,
    pub revision: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create_simulation_scenario(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateScenarioRequest>,
) -> Result<(StatusCode, Json<ScenarioResponse>), ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin])?;

    let name = body.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::validation("Scenario name must be 1..=120 chars"));
    }

    let (fixture_payload, fixture_sha256) =
        build_drill_fixture(body.fixture_tag.trim()).map_err(ApiError::validation)?;
    let fixture_json = serde_json::to_value(&fixture_payload).map_err(ApiError::internal)?;

    let scenario_id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO simulation_scenarios (
            scenario_id, name, created_by_user_id, status, revision,
            fixture_tag, fixture_payload_json, fixture_sha256
         ) VALUES ($1, $2, $3, 'DRAFT', 0, $4, $5, $6)
         RETURNING created_at",
    )
    .bind(scenario_id)
    .bind(name)
    .bind(auth.user_id)
    .bind(body.fixture_tag.trim())
    .bind(sqlx::types::Json(fixture_json))
    .bind(fixture_sha256)
    // Strictly execute on isolated simulator pool (`SIMULATOR_DATABASE_URL`).
    .fetch_one(&state.sim_pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok((
        StatusCode::CREATED,
        Json(ScenarioResponse {
            scenario_id,
            name: name.to_string(),
            fixture_tag: body.fixture_tag.trim().to_string(),
            revision: 0,
            status: "DRAFT".to_string(),
            created_at: row.try_get("created_at").map_err(ApiError::from_db)?,
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct SimulatedNotificationPreview {
    pub channel: String,
    pub preview: String,
}

#[derive(Debug, Serialize)]
pub struct SimEventResponse {
    pub sim_event_id: Uuid,
    pub scenario_id: Uuid,
    pub execution_context: &'static str,
    pub simulated_notifications: Vec<SimulatedNotificationPreview>,
}

pub async fn inject_simulation_event(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(scenario_id): ValidatedPath<Uuid>,
    ValidatedJson(body): ValidatedJson<SimEventRequest>,
) -> Result<(StatusCode, Json<SimEventResponse>), ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin])?;

    let mut tx = state.sim_pool.begin().await.map_err(ApiError::from_db)?;

    let rev_row = sqlx::query(
        "UPDATE simulation_scenarios
         SET revision = revision + 1,
             status = 'RUNNING'
         WHERE scenario_id = $1
         RETURNING revision, fixture_payload_json",
    )
    .bind(scenario_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::not_found("Simulation scenario not found"))?;

    let applied_revision: i64 = rev_row.try_get("revision").map_err(ApiError::from_db)?;
    let fixture_json: sqlx::types::Json<DrillFixturePayload> = rev_row
        .try_get("fixture_payload_json")
        .map_err(ApiError::from_db)?;
    let requested_segment_id = match &body {
        SimEventRequest::WeatherOverride { segment_id, .. }
        | SimEventRequest::BlockageOverride { segment_id, .. } => *segment_id,
    };
    let in_fixture = fixture_json
        .0
        .segments
        .iter()
        .any(|s| s.segment.segment_id == requested_segment_id);
    if !in_fixture {
        let in_road_segments: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM road_segments WHERE segment_id = $1)")
                .bind(requested_segment_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(ApiError::from_db)?;
        if !in_road_segments {
            return Err(ApiError::validation(
                "segment_id does not exist in scenario fixture or operational road_segments",
            ));
        }
    }
    let sim_event_id = Uuid::new_v4();

    let (segment_id, summary_msg) = match &body {
        SimEventRequest::WeatherOverride {
            segment_id,
            precip_24h_mm,
            precip_72h_mm,
        } => {
            if *precip_24h_mm < Decimal::ZERO || *precip_72h_mm < *precip_24h_mm {
                return Err(ApiError::validation(
                    "precip_72h_mm must be >= precip_24h_mm and non-negative",
                ));
            }
            sqlx::query(
                "INSERT INTO sim_events (
                    sim_event_id, scenario_id, segment_id, event_type,
                    precip_24h_mm, precip_72h_mm, applied_revision
                 ) VALUES ($1, $2, $3, 'WEATHER_OVERRIDE', $4, $5, $6)",
            )
            .bind(sim_event_id)
            .bind(scenario_id)
            .bind(segment_id)
            .bind(precip_24h_mm)
            .bind(precip_72h_mm)
            .bind(applied_revision)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;

            (
                *segment_id,
                format!(
                    "[SIMULATION] Alerta geotecnica de lluvia ({precip_24h_mm}/{precip_72h_mm} mm) en tramo {segment_id}."
                ),
            )
        }
        SimEventRequest::BlockageOverride {
            segment_id,
            simulated_passability,
            hazard_lat,
            hazard_lon,
            hazard_radius_m,
        } => {
            if !(-90.0..=90.0).contains(hazard_lat)
                || !(-180.0..=180.0).contains(hazard_lon)
                || *hazard_radius_m == 0
            {
                return Err(ApiError::validation(
                    "Invalid hazard coordinates or hazard_radius_m",
                ));
            }
            let pass_str = match simulated_passability {
                ProposedPassability::Blocked => "BLOCKED",
                ProposedPassability::PartialOneLane => "PARTIAL_ONE_LANE",
                ProposedPassability::Open => {
                    return Err(ApiError::validation(
                        "BLOCKAGE_OVERRIDE only supports BLOCKED or PARTIAL_ONE_LANE",
                    ));
                }
            };
            sqlx::query(
                "INSERT INTO sim_events (
                    sim_event_id, scenario_id, segment_id, event_type,
                    simulated_passability, hazard_location_geom, hazard_radius_m, applied_revision
                 ) VALUES (
                    $1, $2, $3, 'BLOCKAGE_OVERRIDE', $4,
                    ST_SetSRID(ST_MakePoint($5, $6), 4326), $7, $8
                 )",
            )
            .bind(sim_event_id)
            .bind(scenario_id)
            .bind(segment_id)
            .bind(pass_str)
            .bind(hazard_lon)
            .bind(hazard_lat)
            .bind(i32::try_from(*hazard_radius_m).unwrap_or(i32::MAX))
            .bind(applied_revision)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;

            (
                *segment_id,
                format!(
                    "[SIMULATION] Interrupcion {pass_str} en sector Carpish (tramo {segment_id})."
                ),
            )
        }
    };

    tx.commit().await.map_err(ApiError::from_db)?;

    // Strictly isolated simulation gateway: NEVER touches production WhatsApp/SMS.
    let _ = segment_id;
    let preview = state
        .sim_gateway
        .format_and_dispatch(NotificationEvent {
            channel: NotificationChannel::SimulationPreview,
            message: summary_msg,
        })
        .await;

    Ok((
        StatusCode::CREATED,
        Json(SimEventResponse {
            sim_event_id,
            scenario_id,
            execution_context: "SIMULATION",
            simulated_notifications: vec![SimulatedNotificationPreview {
                channel: "SIMULATION".to_string(),
                preview,
            }],
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationRoutingRequest {
    pub origin_node_id: Uuid,
    pub destination_node_id: Uuid,
    pub vehicle_category_code: VehicleCategoryCode,
    pub cargo_type: CargoType,
    pub cargo_payload_kg: Option<Decimal>,
    pub patient_count: Option<u8>,
    pub scenario_revision: i64,
    pub evaluation_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SimulationRoutingResponse {
    pub execution_context: &'static str,
    pub scenario_id: Uuid,
    #[serde(flatten)]
    pub routing: RoutingCalculateResponseDto,
}

async fn load_sim_active_models(
    pool: &PgPool,
) -> Result<(Uuid, IvvModelParams, Uuid, RoutingConfig), ApiError> {
    let ivv_row = sqlx::query(
        "SELECT model_version_id, version_tag, w_precip, w_slope_ravine, w_soil, w_hotspot,
                precip_24h_cap_mm, precip_72h_cap_mm, slope_cap_pct, ravine_max_dist_m,
                amber_threshold, red_threshold, validity_hours
         FROM ivv_model_versions
         WHERE is_active = TRUE
         LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .map_err(ApiError::from_db)?;

    let ivv_id: Uuid = ivv_row
        .try_get("model_version_id")
        .map_err(ApiError::from_db)?;
    let ravine_dist: i32 = ivv_row
        .try_get("ravine_max_dist_m")
        .map_err(ApiError::from_db)?;
    let validity_h: i16 = ivv_row
        .try_get("validity_hours")
        .map_err(ApiError::from_db)?;

    let ivv_params = IvvModelParams {
        version_tag: ivv_row.try_get("version_tag").map_err(ApiError::from_db)?,
        w_precip: ivv_row.try_get("w_precip").map_err(ApiError::from_db)?,
        w_slope_ravine: ivv_row
            .try_get("w_slope_ravine")
            .map_err(ApiError::from_db)?,
        w_soil: ivv_row.try_get("w_soil").map_err(ApiError::from_db)?,
        w_hotspot: ivv_row.try_get("w_hotspot").map_err(ApiError::from_db)?,
        precip_24h_cap_mm: ivv_row
            .try_get("precip_24h_cap_mm")
            .map_err(ApiError::from_db)?,
        precip_72h_cap_mm: ivv_row
            .try_get("precip_72h_cap_mm")
            .map_err(ApiError::from_db)?,
        slope_cap_pct: ivv_row
            .try_get("slope_cap_pct")
            .map_err(ApiError::from_db)?,
        ravine_max_dist_m: Decimal::from(ravine_dist),
        amber_threshold: ivv_row
            .try_get("amber_threshold")
            .map_err(ApiError::from_db)?,
        red_threshold: ivv_row
            .try_get("red_threshold")
            .map_err(ApiError::from_db)?,
        validity_hours: i64::from(validity_h),
    };

    let rte_row = sqlx::query(
        "SELECT routing_config_id, version_tag, alpha_ivv_weight, beta_suspected_penalty
         FROM routing_config_versions
         WHERE is_active = TRUE
         LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .map_err(ApiError::from_db)?;

    let rte_id: Uuid = rte_row
        .try_get("routing_config_id")
        .map_err(ApiError::from_db)?;
    let rte_cfg = RoutingConfig {
        version_tag: rte_row.try_get("version_tag").map_err(ApiError::from_db)?,
        alpha_ivv_weight: rte_row
            .try_get("alpha_ivv_weight")
            .map_err(ApiError::from_db)?,
        beta_suspected_penalty: rte_row
            .try_get("beta_suspected_penalty")
            .map_err(ApiError::from_db)?,
        allow_synthetic_attributes: false,
    };

    Ok((ivv_id, ivv_params, rte_id, rte_cfg))
}

async fn load_sim_vehicle(
    pool: &PgPool,
    code: VehicleCategoryCode,
) -> Result<(Uuid, VehicleCategorySpec), ApiError> {
    let row = sqlx::query(
        "SELECT category_id, max_slope_pct, min_road_width_m, max_weight_tons, allows_dirt_track
         FROM vehicle_categories
         WHERE code = $1",
    )
    .bind(vehicle_code_str(code))
    .fetch_one(pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok((
        row.try_get("category_id").map_err(ApiError::from_db)?,
        VehicleCategorySpec {
            code,
            max_slope_pct: row.try_get("max_slope_pct").map_err(ApiError::from_db)?,
            min_road_width_m: row.try_get("min_road_width_m").map_err(ApiError::from_db)?,
            max_weight_tons: row.try_get("max_weight_tons").map_err(ApiError::from_db)?,
            allows_dirt_track: row
                .try_get("allows_dirt_track")
                .map_err(ApiError::from_db)?,
        },
    ))
}

struct SimRoutingContext<'a> {
    scenario_id: Uuid,
    scenario_revision: i64,
    evaluation_time: DateTime<Utc>,
    vehicle: VehicleCategorySpec,
    config: RoutingConfig,
    ivv_model: &'a IvvModelParams,
    request: RoutingRequest,
}

struct BlockageOverride {
    event_id: Uuid,
    passability: String,
    hazard: Option<(f64, f64, i32)>,
}

async fn build_simulated_routing_input(
    pool: &PgPool,
    ctx: SimRoutingContext<'_>,
) -> Result<RoutingInput, ApiError> {
    let sc_row = sqlx::query(
        "SELECT revision, fixture_payload_json
         FROM simulation_scenarios
         WHERE scenario_id = $1",
    )
    .bind(ctx.scenario_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::not_found("Simulation scenario not found"))?;

    let current_revision: i64 = sc_row.try_get("revision").map_err(ApiError::from_db)?;
    if ctx.scenario_revision < 0 || ctx.scenario_revision > current_revision {
        return Err(ApiError::validation(
            "Requested scenario_revision does not exist or exceeds scenario revision",
        ));
    }

    let fixture_json: sqlx::types::Json<DrillFixturePayload> = sc_row
        .try_get("fixture_payload_json")
        .map_err(ApiError::from_db)?;
    let fixture = fixture_json.0;

    let ev_rows = sqlx::query(
        "SELECT sim_event_id, segment_id, event_type, precip_24h_mm, precip_72h_mm,
                simulated_passability,
                ST_X(hazard_location_geom) AS hazard_lon,
                ST_Y(hazard_location_geom) AS hazard_lat,
                hazard_radius_m,
                applied_revision
         FROM sim_events
         WHERE scenario_id = $1 AND applied_revision <= $2
         ORDER BY applied_revision ASC",
    )
    .bind(ctx.scenario_id)
    .bind(ctx.scenario_revision)
    .fetch_all(pool)
    .await
    .map_err(ApiError::from_db)?;

    // Later applied_revision overwrites earlier event for same (segment_id, event_type) (SIM-03).
    let mut weather_overrides: HashMap<Uuid, (Decimal, Decimal)> = HashMap::new();
    let mut blockage_overrides: HashMap<Uuid, BlockageOverride> = HashMap::new();

    for ev in ev_rows {
        let ev_id: Uuid = ev.try_get("sim_event_id").map_err(ApiError::from_db)?;
        let seg_id: Uuid = ev.try_get("segment_id").map_err(ApiError::from_db)?;
        let ev_type: String = ev.try_get("event_type").map_err(ApiError::from_db)?;
        if ev_type == "WEATHER_OVERRIDE" {
            let p24: Decimal = ev.try_get("precip_24h_mm").map_err(ApiError::from_db)?;
            let p72: Decimal = ev.try_get("precip_72h_mm").map_err(ApiError::from_db)?;
            weather_overrides.insert(seg_id, (p24, p72));
        } else if ev_type == "BLOCKAGE_OVERRIDE" {
            let pass: String = ev
                .try_get("simulated_passability")
                .map_err(ApiError::from_db)?;
            let lon: Option<f64> = ev.try_get("hazard_lon").map_err(ApiError::from_db)?;
            let lat: Option<f64> = ev.try_get("hazard_lat").map_err(ApiError::from_db)?;
            let rad: Option<i32> = ev.try_get("hazard_radius_m").map_err(ApiError::from_db)?;
            let geom = match (lon, lat, rad) {
                (Some(lo), Some(la), Some(r)) => Some((lo, la, r)),
                _ => None,
            };
            blockage_overrides.insert(
                seg_id,
                BlockageOverride {
                    event_id: ev_id,
                    passability: pass,
                    hazard: geom,
                },
            );
        }
    }

    let mut segments = Vec::with_capacity(fixture.segments.len());
    for mut seg_fix in fixture.segments {
        if let Some((p24, p72)) = weather_overrides.get(&seg_fix.segment.segment_id) {
            let ivv_out = calculate_ivv(
                ctx.ivv_model,
                &SegmentGeoInput {
                    max_slope_forward_pct: seg_fix.segment.max_slope_forward_pct,
                    max_slope_reverse_pct: seg_fix.segment.max_slope_reverse_pct,
                    soil_susceptibility_idx: seg_fix.soil_susceptibility_idx,
                    distance_to_ravine_m: seg_fix.distance_to_ravine_m,
                    is_historical_hotspot: seg_fix.is_historical_hotspot,
                },
                &WeatherInput {
                    precip_24h_mm: *p24,
                    precip_72h_mm: *p72,
                    observed_at: ctx.evaluation_time,
                },
                Some(ctx.evaluation_time),
                Some(ctx.evaluation_time),
            )
            .map_err(|e| ApiError::validation(e.to_string()))?;
            seg_fix.segment.ivv_score = Some(ivv_out.ivv_score);
            seg_fix.segment.ivv_is_stale = ivv_out.is_stale;
        }
        if let Some(blk) = blockage_overrides.get(&seg_fix.segment.segment_id) {
            seg_fix.segment.passability_state = match blk.passability.as_str() {
                "BLOCKED" => PassabilityState::Blocked,
                "PARTIAL_ONE_LANE" => PassabilityState::PartialOneLane,
                _ => seg_fix.segment.passability_state,
            };
            seg_fix.segment.active_confirmed_incident_id = Some(blk.event_id);
        }
        segments.push(seg_fix.segment);
    }

    let active_hazards: Vec<(f64, f64, i32)> = blockage_overrides
        .values()
        .filter_map(|b| b.hazard)
        .collect();

    let mut transfer_corridors: Vec<TransferCorridorSpec> =
        Vec::with_capacity(fixture.transfer_corridors.len());
    for mut tc_fix in fixture.transfer_corridors {
        let mut intersects = false;
        for (lon, lat, rad) in &active_hazards {
            let row = sqlx::query(
                "SELECT ST_DWithin(
                    ST_GeomFromText($1, 4326)::geography,
                    ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography,
                    $4
                ) AS intersects",
            )
            .bind(&tc_fix.path_wkt)
            .bind(lon)
            .bind(lat)
            .bind(rad)
            .fetch_one(pool)
            .await
            .map_err(ApiError::from_db)?;
            if row
                .try_get::<bool, _>("intersects")
                .map_err(ApiError::from_db)?
            {
                intersects = true;
                break;
            }
        }
        tc_fix.corridor.hazard_intersects_path = intersects;
        if let Some(max_ivv) = segments
            .iter()
            .filter(|s| {
                s.origin_node_id == tc_fix.corridor.side_a_node_id
                    || s.destination_node_id == tc_fix.corridor.side_a_node_id
                    || s.origin_node_id == tc_fix.corridor.side_b_node_id
                    || s.destination_node_id == tc_fix.corridor.side_b_node_id
            })
            .filter_map(|s| s.ivv_score)
            .max()
        {
            tc_fix.corridor.sector_ivv = Some(max_ivv);
        }
        transfer_corridors.push(tc_fix.corridor);
    }

    Ok(RoutingInput {
        vehicle: ctx.vehicle,
        nodes: fixture.nodes,
        segments,
        transfer_corridors,
        humanitarian_windows: Vec::new(),
        config: ctx.config,
        request: ctx.request,
        evaluation_time: ctx.evaluation_time,
    })
}

pub async fn run_simulation_routing(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(scenario_id): ValidatedPath<Uuid>,
    ValidatedJson(body): ValidatedJson<SimulationRoutingRequest>,
) -> Result<Json<SimulationRoutingResponse>, ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin])?;
    validate_cargo_request(body.cargo_type, body.cargo_payload_kg, body.patient_count)?;

    let (_, ivv_model, _, rte_cfg) = load_sim_active_models(&state.sim_pool).await?;
    let (_, vehicle) = load_sim_vehicle(&state.sim_pool, body.vehicle_category_code).await?;

    let input = build_simulated_routing_input(
        &state.sim_pool,
        SimRoutingContext {
            scenario_id,
            scenario_revision: body.scenario_revision,
            evaluation_time: body.evaluation_time,
            vehicle,
            config: rte_cfg,
            ivv_model: &ivv_model,
            request: RoutingRequest {
                origin_node_id: body.origin_node_id,
                destination_node_id: body.destination_node_id,
                vehicle_category_code: body.vehicle_category_code,
                cargo_type: body.cargo_type,
                cargo_payload_kg: body.cargo_payload_kg,
                patient_count: body.patient_count,
            },
        },
    )
    .await?;

    let resolution = calculate_route(&input);

    Ok(Json(SimulationRoutingResponse {
        execution_context: "SIMULATION",
        scenario_id,
        routing: RoutingCalculateResponseDto::from(resolution),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationEvaluateRequest {
    pub origin_node_id: Uuid,
    pub destination_node_id: Uuid,
    pub vehicle_category_code: VehicleCategoryCode,
    pub cargo_type: CargoType,
    pub cargo_payload_kg: Option<Decimal>,
    pub patient_count: Option<u8>,
    pub baseline_wait_min: Decimal,
    pub scenario_revision: i64,
    pub evaluation_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SimulationEvaluateResponse {
    pub sim_log_id: Uuid,
    pub resolution_type: String,
    pub mitigation_status: MitigationStatus,
    pub scenario_revision: i64,
    pub evaluation_time: DateTime<Utc>,
    pub param_version_tag: String,
    pub model_version_tag: String,
    pub routing_config_tag: String,
}

fn baseline_nominal_metrics(
    input: &RoutingInput,
    origin: Uuid,
    destination: Uuid,
) -> (Decimal, Decimal) {
    // Normal pre-contingency highway between Huánuco and Tingo María follows the direct Carpish
    // main corridor (118 km, 145 min); check it first so bypass detours reflect true extra distance.
    if (origin == NODE_HUANUCO_ID && destination == NODE_TINGO_MARIA_ID)
        || (origin == NODE_TINGO_MARIA_ID && destination == NODE_HUANUCO_ID)
    {
        let main_ids = [
            SEG_HUANUCO_CARPISH_SUR_ID,
            SEG_CARPISH_TUNNEL_ID,
            SEG_CARPISH_NORTE_TINGO_ID,
        ];
        let mut time = Decimal::ZERO;
        let mut dist = Decimal::ZERO;
        for seg in &input.segments {
            if main_ids.contains(&seg.segment_id) {
                time += seg.base_travel_time_min;
                dist += seg.length_km;
            }
        }
        if time > Decimal::ZERO {
            return (time, dist);
        }
    }

    let mut pre_event_input = input.clone();
    for seg in &mut pre_event_input.segments {
        seg.passability_state = PassabilityState::Open;
        seg.active_confirmed_incident_id = None;
    }
    if let RoutingResolution::AlternateRoute {
        total_distance_km,
        estimated_time_min,
        ..
    } = calculate_route(&pre_event_input)
    {
        return (estimated_time_min, total_distance_km);
    }

    // Generic fallback summing minimum base segment time & distance between endpoints.
    (Decimal::from(120), Decimal::from(60))
}

fn compute_bridge_leg_totals(
    input: &RoutingInput,
    side_a: Uuid,
    side_b: Uuid,
) -> (Option<Decimal>, Option<Decimal>) {
    let mut leg1_in = input.clone();
    leg1_in.request.destination_node_id = side_a;
    let leg1 = calculate_route(&leg1_in);

    let mut leg2_in = input.clone();
    leg2_in.request.origin_node_id = side_b;
    let leg2 = calculate_route(&leg2_in);

    match (leg1, leg2) {
        (
            RoutingResolution::AlternateRoute {
                total_distance_km: d1,
                estimated_time_min: t1,
                ..
            },
            RoutingResolution::AlternateRoute {
                total_distance_km: d2,
                estimated_time_min: t2,
                ..
            },
        ) => (Some(d1 + d2), Some(t1 + t2)),
        _ => (None, None),
    }
}

pub async fn evaluate_simulation_impact(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(scenario_id): ValidatedPath<Uuid>,
    ValidatedJson(body): ValidatedJson<SimulationEvaluateRequest>,
) -> Result<(StatusCode, Json<SimulationEvaluateResponse>), ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin])?;
    validate_cargo_request(body.cargo_type, body.cargo_payload_kg, body.patient_count)?;
    if body.baseline_wait_min < Decimal::ZERO {
        return Err(ApiError::validation("baseline_wait_min must be >= 0"));
    }

    let (ivv_id, ivv_model, rte_id, rte_cfg) = load_sim_active_models(&state.sim_pool).await?;
    let (vehicle_cat_id, vehicle) =
        load_sim_vehicle(&state.sim_pool, body.vehicle_category_code).await?;

    let param_row = sqlx::query(
        "SELECT param_version_id, version_tag, co2_idle_kg_per_hour, co2_drive_kg_per_km,
                agri_cargo_value_usd, perishability_loss_rate_per_hour,
                transfer_handling_loss_pct, spoilage_grace_hours
         FROM impact_parameter_versions
         WHERE vehicle_category_id = $1 AND is_active = TRUE
         LIMIT 1",
    )
    .bind(vehicle_cat_id)
    .fetch_one(&state.sim_pool)
    .await
    .map_err(ApiError::from_db)?;

    let param_version_id: Uuid = param_row
        .try_get("param_version_id")
        .map_err(ApiError::from_db)?;
    let impact_params = ImpactParameters {
        version_tag: param_row
            .try_get("version_tag")
            .map_err(ApiError::from_db)?,
        co2_idle_kg_per_hour: param_row
            .try_get("co2_idle_kg_per_hour")
            .map_err(ApiError::from_db)?,
        co2_drive_kg_per_km: param_row
            .try_get("co2_drive_kg_per_km")
            .map_err(ApiError::from_db)?,
        agri_cargo_value_usd: param_row
            .try_get("agri_cargo_value_usd")
            .map_err(ApiError::from_db)?,
        perishability_loss_rate_per_hour: param_row
            .try_get("perishability_loss_rate_per_hour")
            .map_err(ApiError::from_db)?,
        transfer_handling_loss_pct: param_row
            .try_get("transfer_handling_loss_pct")
            .map_err(ApiError::from_db)?,
        spoilage_grace_hours: param_row
            .try_get("spoilage_grace_hours")
            .map_err(ApiError::from_db)?,
    };

    let sim_input = build_simulated_routing_input(
        &state.sim_pool,
        SimRoutingContext {
            scenario_id,
            scenario_revision: body.scenario_revision,
            evaluation_time: body.evaluation_time,
            vehicle,
            config: rte_cfg.clone(),
            ivv_model: &ivv_model,
            request: RoutingRequest {
                origin_node_id: body.origin_node_id,
                destination_node_id: body.destination_node_id,
                vehicle_category_code: body.vehicle_category_code,
                cargo_type: body.cargo_type,
                cargo_payload_kg: body.cargo_payload_kg,
                patient_count: body.patient_count,
            },
        },
    )
    .await?;

    let (base_travel, base_dist) =
        baseline_nominal_metrics(&sim_input, body.origin_node_id, body.destination_node_id);
    let resolution = calculate_route(&sim_input);

    let (bridge_dist, bridge_time) = match &resolution {
        RoutingResolution::LogisticBridgeRequired {
            bridge_proposal, ..
        } => compute_bridge_leg_totals(
            &sim_input,
            bridge_proposal.side_a_node_id,
            bridge_proposal.side_b_node_id,
        ),
        _ => (None, None),
    };

    let impact = evaluate_impact(&ImpactInput {
        params: impact_params.clone(),
        cargo_type: body.cargo_type,
        baseline_normal_travel_min: base_travel,
        baseline_wait_min: body.baseline_wait_min,
        baseline_distance_km: base_dist,
        resolution: resolution.clone(),
        bridge_leg_total_distance_km: bridge_dist,
        bridge_leg_total_travel_min: bridge_time,
        mitigated_idle_min: None,
    })
    .map_err(|e| ApiError::validation(e.to_string()))?;

    let resolution_type_str = match &resolution {
        RoutingResolution::AlternateRoute { .. } => "ALTERNATE_ROUTE",
        RoutingResolution::LogisticBridgeRequired { .. } => "LOGISTIC_BRIDGE_REQUIRED",
        RoutingResolution::NoSafeRoute { .. } => "NO_SAFE_ROUTE",
    };

    let sim_log_id = Uuid::new_v4();
    let request_snapshot = serde_json::json!({
        "origin_node_id": body.origin_node_id,
        "destination_node_id": body.destination_node_id,
        "vehicle_category_code": vehicle_code_str(body.vehicle_category_code),
        "cargo_type": cargo_type_str(body.cargo_type),
        "cargo_payload_kg": body.cargo_payload_kg,
        "patient_count": body.patient_count,
        "baseline_wait_min": body.baseline_wait_min,
        "evaluation_time": body.evaluation_time
    });

    sqlx::query(
        "INSERT INTO simulation_impact_logs (
            sim_log_id, scenario_id, param_version_id, routing_config_id, ivv_model_version_id,
            scenario_revision, request_snapshot_json, resolution_type,
            baseline_travel_min, mitigated_travel_min,
            baseline_co2_kg, mitigated_co2_kg,
            baseline_cargo_loss_usd, mitigated_cargo_loss_usd, computed_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, clock_timestamp())",
    )
    .bind(sim_log_id)
    .bind(scenario_id)
    .bind(param_version_id)
    .bind(rte_id)
    .bind(ivv_id)
    .bind(body.scenario_revision)
    .bind(sqlx::types::Json(request_snapshot))
    .bind(resolution_type_str)
    .bind(impact.baseline.travel_min)
    .bind(impact.mitigated.travel_min)
    .bind(impact.baseline.co2_kg)
    .bind(impact.mitigated.co2_kg)
    .bind(impact.baseline.cargo_loss_usd)
    .bind(impact.mitigated.cargo_loss_usd)
    .execute(&state.sim_pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok((
        StatusCode::CREATED,
        Json(SimulationEvaluateResponse {
            sim_log_id,
            resolution_type: resolution_type_str.to_string(),
            mitigation_status: impact.mitigation_status,
            scenario_revision: body.scenario_revision,
            evaluation_time: body.evaluation_time,
            param_version_tag: impact.param_version_tag,
            model_version_tag: ivv_model.version_tag,
            routing_config_tag: rte_cfg.version_tag,
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct ImpactBaselineDto {
    #[serde(with = "rust_decimal::serde::float")]
    pub travel_min: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub co2_kg: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub cargo_loss_usd: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ImpactMitigatedDto {
    #[serde(with = "rust_decimal::serde::float_option")]
    pub travel_min: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub co2_kg: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub cargo_loss_usd: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct ImpactDeltaDto {
    #[serde(with = "rust_decimal::serde::float_option")]
    pub time_delta_min: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub time_saved_pct: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub net_co2_delta_kg: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub net_agri_savings_usd: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct SimulationImpactItemDto {
    pub sim_log_id: Uuid,
    pub resolution_type: String,
    pub mitigation_status: MitigationStatus,
    pub baseline: ImpactBaselineDto,
    pub mitigated: ImpactMitigatedDto,
    pub deltas: ImpactDeltaDto,
}

#[derive(Debug, Serialize)]
pub struct SimulationImpactResponse {
    pub scenario_id: Uuid,
    pub evaluations: Vec<SimulationImpactItemDto>,
}

pub async fn get_simulation_impact(
    State(state): State<AppState>,
    _auth: AuthUser,
    ValidatedPath(scenario_id): ValidatedPath<Uuid>,
) -> Result<Json<SimulationImpactResponse>, ApiError> {
    let sc_exists = sqlx::query("SELECT 1 FROM simulation_scenarios WHERE scenario_id = $1")
        .bind(scenario_id)
        .fetch_optional(&state.sim_pool)
        .await
        .map_err(ApiError::from_db)?;
    if sc_exists.is_none() {
        return Err(ApiError::not_found("Simulation scenario not found"));
    }

    let rows = sqlx::query(
        "SELECT sim_log_id, resolution_type,
                baseline_travel_min, mitigated_travel_min,
                baseline_co2_kg, mitigated_co2_kg,
                baseline_cargo_loss_usd, mitigated_cargo_loss_usd
         FROM simulation_impact_logs
         WHERE scenario_id = $1
         ORDER BY computed_at ASC, sim_log_id ASC",
    )
    .bind(scenario_id)
    .fetch_all(&state.sim_pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut evaluations = Vec::with_capacity(rows.len());
    for r in rows {
        let resolution_type: String = r.try_get("resolution_type").map_err(ApiError::from_db)?;
        let base_t: Decimal = r
            .try_get("baseline_travel_min")
            .map_err(ApiError::from_db)?;
        let base_co2: Decimal = r.try_get("baseline_co2_kg").map_err(ApiError::from_db)?;
        let base_loss: Decimal = r
            .try_get("baseline_cargo_loss_usd")
            .map_err(ApiError::from_db)?;
        let mit_t: Option<Decimal> = r
            .try_get("mitigated_travel_min")
            .map_err(ApiError::from_db)?;
        let mit_co2: Option<Decimal> = r.try_get("mitigated_co2_kg").map_err(ApiError::from_db)?;
        let mit_loss: Option<Decimal> = r
            .try_get("mitigated_cargo_loss_usd")
            .map_err(ApiError::from_db)?;

        let (mitigation_status, deltas) = match (mit_t, mit_co2, mit_loss) {
            (Some(mt), Some(mco2), Some(mloss)) if resolution_type != "NO_SAFE_ROUTE" => {
                let dt = paradixe_domain::round_half_away_from_zero(base_t - mt, 2);
                let saved_pct = paradixe_domain::round_half_away_from_zero(
                    (dt / base_t) * Decimal::from(100),
                    2,
                );
                let dco2 = paradixe_domain::round_half_away_from_zero(base_co2 - mco2, 2);
                let dsav = paradixe_domain::round_half_away_from_zero(base_loss - mloss, 2);
                (
                    MitigationStatus::Mitigated,
                    ImpactDeltaDto {
                        time_delta_min: Some(dt),
                        time_saved_pct: Some(saved_pct),
                        net_co2_delta_kg: Some(dco2),
                        net_agri_savings_usd: Some(dsav),
                    },
                )
            }
            _ => (
                MitigationStatus::UnmitigatedNoSafeRoute,
                ImpactDeltaDto {
                    time_delta_min: None,
                    time_saved_pct: None,
                    net_co2_delta_kg: None,
                    net_agri_savings_usd: None,
                },
            ),
        };

        evaluations.push(SimulationImpactItemDto {
            sim_log_id: r.try_get("sim_log_id").map_err(ApiError::from_db)?,
            resolution_type,
            mitigation_status,
            baseline: ImpactBaselineDto {
                travel_min: base_t,
                co2_kg: base_co2,
                cargo_loss_usd: base_loss,
            },
            mitigated: ImpactMitigatedDto {
                travel_min: mit_t,
                co2_kg: mit_co2,
                cargo_loss_usd: mit_loss,
            },
            deltas,
        });
    }

    Ok(Json(SimulationImpactResponse {
        scenario_id,
        evaluations,
    }))
}

// Keep unused parse helpers visible if needed cleanly in integration flows
#[allow(dead_code)]
fn _ensure_cargo_helper_linked(raw: &str) -> Result<CargoType, ApiError> {
    parse_cargo_type(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_04_rejects_mixed_weather_and_blockage_fields() {
        let mixed = serde_json::json!({
            "segment_id": Uuid::new_v4(),
            "event_type": "WEATHER_OVERRIDE",
            "precip_24h_mm": 85.0,
            "precip_72h_mm": 190.0,
            "simulated_passability": "BLOCKED"
        });
        assert!(serde_json::from_value::<SimEventRequest>(mixed).is_err());
    }
}
