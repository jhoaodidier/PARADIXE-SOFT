#![forbid(unsafe_code)]

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use paradixe_domain::RiskLevel;
use paradixe_ivv::{
    calculate_ivv, evaluate_ivv_freshness, IvvError, IvvModelParams, SegmentGeoInput, WeatherInput,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    auth::{AuthUser, RoleCode},
    error::{ApiError, ValidatedJson, ValidatedQuery},
    sync::append_sync_change,
    AppState,
};

#[derive(Debug, Serialize)]
pub struct GeoJsonLineString {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub coordinates: Vec<Vec<f64>>,
}

#[derive(Debug, Serialize)]
pub struct CorridorSegmentPropertiesDto {
    pub segment_id: Uuid,
    pub origin_node_id: Uuid,
    pub destination_node_id: Uuid,
    pub corridor_name: String,
    pub direction_type: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub length_km: Decimal,
    pub surface_type: String,
    pub passability_state: String,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub ivv_score: Option<Decimal>,
    pub risk_level: RiskLevel,
    pub observed_at: Option<DateTime<Utc>>,
    pub calculated_at: Option<DateTime<Utc>>,
    pub model_version_tag: Option<String>,
    pub is_stale: bool,
    pub is_attributes_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct CorridorSegmentFeatureDto {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub geometry: GeoJsonLineString,
    pub properties: CorridorSegmentPropertiesDto,
}

#[derive(Debug, Serialize)]
pub struct CorridorMapFeatureCollectionDto {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub generated_at: DateTime<Utc>,
    pub features: Vec<CorridorSegmentFeatureDto>,
}

#[derive(Debug, Deserialize)]
struct ParsedGeoJsonLineString {
    coordinates: Vec<Vec<f64>>,
}

pub async fn load_active_ivv_model(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<(Uuid, IvvModelParams), ApiError> {
    let row = sqlx::query(
        "SELECT model_version_id, version_tag, w_precip, w_slope_ravine, w_soil, w_hotspot,
                precip_24h_cap_mm, precip_72h_cap_mm, slope_cap_pct, ravine_max_dist_m,
                amber_threshold, red_threshold, validity_hours
         FROM ivv_model_versions
         WHERE is_active = TRUE
         LIMIT 1",
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::internal("No active ivv_model_versions row"))?;

    let model_version_id: Uuid = row.try_get("model_version_id").map_err(ApiError::from_db)?;
    let ravine_max_dist_m: i32 = row
        .try_get("ravine_max_dist_m")
        .map_err(ApiError::from_db)?;
    let validity_hours: i16 = row.try_get("validity_hours").map_err(ApiError::from_db)?;

    let params = IvvModelParams {
        version_tag: row.try_get("version_tag").map_err(ApiError::from_db)?,
        w_precip: row.try_get("w_precip").map_err(ApiError::from_db)?,
        w_slope_ravine: row.try_get("w_slope_ravine").map_err(ApiError::from_db)?,
        w_soil: row.try_get("w_soil").map_err(ApiError::from_db)?,
        w_hotspot: row.try_get("w_hotspot").map_err(ApiError::from_db)?,
        precip_24h_cap_mm: row
            .try_get("precip_24h_cap_mm")
            .map_err(ApiError::from_db)?,
        precip_72h_cap_mm: row
            .try_get("precip_72h_cap_mm")
            .map_err(ApiError::from_db)?,
        slope_cap_pct: row.try_get("slope_cap_pct").map_err(ApiError::from_db)?,
        ravine_max_dist_m: Decimal::from(ravine_max_dist_m),
        amber_threshold: row.try_get("amber_threshold").map_err(ApiError::from_db)?,
        red_threshold: row.try_get("red_threshold").map_err(ApiError::from_db)?,
        validity_hours: i64::from(validity_hours),
    };

    Ok((model_version_id, params))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorridorsMapQuery {
    pub region: Option<String>,
    pub bbox: Option<String>,
    pub limit: Option<i64>,
}

fn parse_bbox_filter(raw: &str) -> Result<(f64, f64, f64, f64), ApiError> {
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(ApiError::validation(
            "bbox must have format min_lon,min_lat,max_lon,max_lat",
        ));
    }
    let min_lon: f64 = parts[0]
        .parse()
        .map_err(|_| ApiError::validation("Invalid bbox min_lon"))?;
    let min_lat: f64 = parts[1]
        .parse()
        .map_err(|_| ApiError::validation("Invalid bbox min_lat"))?;
    let max_lon: f64 = parts[2]
        .parse()
        .map_err(|_| ApiError::validation("Invalid bbox max_lon"))?;
    let max_lat: f64 = parts[3]
        .parse()
        .map_err(|_| ApiError::validation("Invalid bbox max_lat"))?;
    if !(-180.0..=180.0).contains(&min_lon)
        || !(-180.0..=180.0).contains(&max_lon)
        || !(-90.0..=90.0).contains(&min_lat)
        || !(-90.0..=90.0).contains(&max_lat)
        || min_lon >= max_lon
        || min_lat >= max_lat
    {
        return Err(ApiError::validation("Invalid bbox bounds"));
    }
    Ok((min_lon, min_lat, max_lon, max_lat))
}

#[derive(Debug, Serialize)]
pub struct TerritorialUnitDto {
    pub unit_id: Uuid,
    pub ubigeo_code: String,
    pub iso_code: Option<String>,
    pub parent_unit_id: Option<Uuid>,
    pub name: String,
    pub admin_level: i16,
    pub center_lon: f64,
    pub center_lat: f64,
    pub pmtiles_uri: Option<String>,
    pub segment_count: i64,
}

#[derive(Debug, Serialize)]
pub struct RegionsListResponse {
    pub regions: Vec<TerritorialUnitDto>,
}

pub async fn get_regions(
    State(state): State<AppState>,
) -> Result<Json<RegionsListResponse>, ApiError> {
    let rows = sqlx::query(
        "SELECT
            tu.unit_id,
            tu.ubigeo_code,
            tu.iso_code,
            tu.parent_unit_id,
            tu.name,
            tu.admin_level,
            ST_X(tu.centroid_geom) AS center_lon,
            ST_Y(tu.centroid_geom) AS center_lat,
            tu.pmtiles_uri,
            (
                SELECT count(*)
                FROM road_segments rs
                WHERE rs.is_topology_verified = TRUE
                  AND (
                      rs.territorial_unit_id = tu.unit_id
                      OR ST_Intersects(rs.geom, tu.bbox_geom)
                  )
            ) AS segment_count
         FROM territorial_units tu
         WHERE tu.is_enabled = TRUE
         ORDER BY tu.admin_level ASC, tu.ubigeo_code ASC, tu.name ASC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut regions = Vec::with_capacity(rows.len());
    for row in rows {
        regions.push(TerritorialUnitDto {
            unit_id: row.try_get("unit_id").map_err(ApiError::from_db)?,
            ubigeo_code: row.try_get("ubigeo_code").map_err(ApiError::from_db)?,
            iso_code: row.try_get("iso_code").map_err(ApiError::from_db)?,
            parent_unit_id: row.try_get("parent_unit_id").map_err(ApiError::from_db)?,
            name: row.try_get("name").map_err(ApiError::from_db)?,
            admin_level: row.try_get("admin_level").map_err(ApiError::from_db)?,
            center_lon: row.try_get("center_lon").map_err(ApiError::from_db)?,
            center_lat: row.try_get("center_lat").map_err(ApiError::from_db)?,
            pmtiles_uri: row.try_get("pmtiles_uri").map_err(ApiError::from_db)?,
            segment_count: row.try_get("segment_count").map_err(ApiError::from_db)?,
        });
    }

    Ok(Json(RegionsListResponse { regions }))
}

pub async fn get_corridors_map(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<CorridorsMapQuery>,
) -> Result<Json<CorridorMapFeatureCollectionDto>, ApiError> {
    let now = Utc::now();
    let limit = query.limit.unwrap_or(2000).clamp(1, 10_000);
    let bbox = query.bbox.as_deref().map(parse_bbox_filter).transpose()?;
    let region_code = query
        .region
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let (min_lon, min_lat, max_lon, max_lat, has_bbox) = match bbox {
        Some((x0, y0, x1, y1)) => (x0, y0, x1, y1, true),
        None => (0.0, 0.0, 0.0, 0.0, false),
    };

    let rows = sqlx::query(
        "SELECT
            rs.segment_id,
            rs.origin_node_id,
            rs.destination_node_id,
            rs.corridor_name,
            rs.direction_type,
            rs.length_km,
            rs.surface_type,
            (rs.attributes_verified_at IS NOT NULL) AS is_attributes_verified,
            ST_AsGeoJSON(rs.geom) AS geom_geojson,
            COALESCE(active_inc.impact_passability, 'OPEN') AS passability_state,
            latest_ivv.ivv_score,
            latest_ivv.observed_at,
            latest_ivv.calculated_at,
            latest_ivv.version_tag AS model_version_tag,
            latest_ivv.amber_threshold,
            latest_ivv.red_threshold,
            latest_ivv.validity_hours
         FROM road_segments rs
         LEFT JOIN LATERAL (
            SELECT i.impact_passability
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
            SELECT
                ia.ivv_score,
                ia.calculated_at,
                wo.observed_at,
                imv.version_tag,
                imv.amber_threshold,
                imv.red_threshold,
                imv.validity_hours
            FROM weather_observations wo
            JOIN ivv_assessments ia ON ia.observation_id = wo.observation_id
            JOIN ivv_model_versions imv ON imv.model_version_id = ia.model_version_id
            WHERE wo.segment_id = rs.segment_id
            ORDER BY wo.observed_at DESC, ia.calculated_at DESC
            LIMIT 1
         ) latest_ivv ON TRUE
         WHERE rs.is_topology_verified = TRUE
           AND (
               NOT $1
               OR rs.geom && ST_MakeEnvelope($2, $3, $4, $5, 4326)
           )
           AND (
               $6::text IS NULL
               OR EXISTS (
                   SELECT 1 FROM territorial_units tu
                   WHERE (UPPER(tu.iso_code) = UPPER($6) OR UPPER(tu.ubigeo_code) = UPPER($6))
                     AND ST_Intersects(rs.geom, tu.bbox_geom)
               )
           )
         ORDER BY rs.corridor_name ASC, rs.segment_id ASC
         LIMIT $7",
    )
    .bind(has_bbox)
    .bind(min_lon)
    .bind(min_lat)
    .bind(max_lon)
    .bind(max_lat)
    .bind(region_code)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut features = Vec::with_capacity(rows.len());
    for row in rows {
        let geom_raw: String = row.try_get("geom_geojson").map_err(ApiError::from_db)?;
        let parsed_geom: ParsedGeoJsonLineString =
            serde_json::from_str(&geom_raw).map_err(ApiError::internal)?;

        let raw_ivv_score: Option<Decimal> = row.try_get("ivv_score").map_err(ApiError::from_db)?;
        let observed_at: Option<DateTime<Utc>> =
            row.try_get("observed_at").map_err(ApiError::from_db)?;
        let calculated_at: Option<DateTime<Utc>> =
            row.try_get("calculated_at").map_err(ApiError::from_db)?;
        let model_version_tag: Option<String> = row
            .try_get("model_version_tag")
            .map_err(ApiError::from_db)?;
        let amber_threshold: Option<Decimal> =
            row.try_get("amber_threshold").map_err(ApiError::from_db)?;
        let red_threshold: Option<Decimal> =
            row.try_get("red_threshold").map_err(ApiError::from_db)?;
        let validity_hours: Option<i16> =
            row.try_get("validity_hours").map_err(ApiError::from_db)?;

        let (ivv_score, risk_level, is_stale) = match (
            raw_ivv_score,
            calculated_at,
            amber_threshold,
            red_threshold,
            validity_hours,
        ) {
            (Some(score), Some(calc_at), Some(amber), Some(red), Some(valid_h)) => {
                let stale = evaluate_ivv_freshness(calc_at, i64::from(valid_h), now);
                if stale {
                    (None, RiskLevel::Unknown, true)
                } else if score < amber {
                    (Some(score), RiskLevel::Green, false)
                } else if score < red {
                    (Some(score), RiskLevel::Amber, false)
                } else {
                    (Some(score), RiskLevel::Red, false)
                }
            }
            _ => (None, RiskLevel::Unknown, true),
        };

        features.push(CorridorSegmentFeatureDto {
            kind: "Feature",
            geometry: GeoJsonLineString {
                kind: "LineString",
                coordinates: parsed_geom.coordinates,
            },
            properties: CorridorSegmentPropertiesDto {
                segment_id: row.try_get("segment_id").map_err(ApiError::from_db)?,
                origin_node_id: row.try_get("origin_node_id").map_err(ApiError::from_db)?,
                destination_node_id: row
                    .try_get("destination_node_id")
                    .map_err(ApiError::from_db)?,
                corridor_name: row.try_get("corridor_name").map_err(ApiError::from_db)?,
                direction_type: row.try_get("direction_type").map_err(ApiError::from_db)?,
                length_km: row.try_get("length_km").map_err(ApiError::from_db)?,
                surface_type: row.try_get("surface_type").map_err(ApiError::from_db)?,
                passability_state: row
                    .try_get("passability_state")
                    .map_err(ApiError::from_db)?,
                ivv_score,
                risk_level,
                observed_at,
                calculated_at,
                model_version_tag,
                is_stale,
                is_attributes_verified: row
                    .try_get("is_attributes_verified")
                    .map_err(ApiError::from_db)?,
            },
        });
    }

    Ok(Json(CorridorMapFeatureCollectionDto {
        kind: "FeatureCollection",
        generated_at: now,
        features,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IvvRecalculateObservationInput {
    pub segment_id: Uuid,
    pub source_name: String,
    pub precip_24h_mm: Decimal,
    pub precip_72h_mm: Decimal,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IvvRecalculateRequest {
    pub observations: Vec<IvvRecalculateObservationInput>,
}

#[derive(Debug, Serialize)]
pub struct IvvRecalculateResponse {
    pub model_version_tag: String,
    pub assessments_created: usize,
    pub revalidations_triggered: usize,
}

fn map_ivv_error(err: IvvError) -> ApiError {
    match err {
        IvvError::InvalidObservation(msg) | IvvError::InvalidSegment(msg) => {
            ApiError::validation(msg)
        }
        IvvError::InvalidWeightsSum(_) | IvvError::InvalidModelConfig(_) => ApiError::internal(err),
    }
}

pub async fn recalculate_ivv(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<IvvRecalculateRequest>,
) -> Result<(StatusCode, Json<IvvRecalculateResponse>), ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin, RoleCode::BotService])?;
    if body.observations.is_empty() {
        return Err(ApiError::validation("observations must not be empty"));
    }

    let now = Utc::now();
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;
    let (model_version_id, model_params) = load_active_ivv_model(&mut tx).await?;

    let mut assessments_created = 0usize;
    let mut revalidations_triggered = 0usize;

    for obs in &body.observations {
        if !matches!(
            obs.source_name.as_str(),
            "SENAMHI" | "NASA_GPM" | "OPEN_METEO" | "LOCAL_STATION"
        ) {
            return Err(ApiError::validation("Unsupported weather source_name"));
        }

        let seg = sqlx::query(
            "SELECT origin_node_id, destination_node_id, max_slope_forward_pct, max_slope_reverse_pct,
                    soil_susceptibility_idx, distance_to_ravine_m, is_historical_hotspot
             FROM road_segments
             WHERE segment_id = $1",
        )
        .bind(obs.segment_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::from_db)?
        .ok_or_else(|| ApiError::validation("Unknown segment_id in weather observation"))?;

        let origin_node_id: Uuid = seg.try_get("origin_node_id").map_err(ApiError::from_db)?;
        let destination_node_id: Uuid = seg
            .try_get("destination_node_id")
            .map_err(ApiError::from_db)?;
        let distance_to_ravine_m: i32 = seg
            .try_get("distance_to_ravine_m")
            .map_err(ApiError::from_db)?;

        let geo_input = SegmentGeoInput {
            max_slope_forward_pct: seg
                .try_get("max_slope_forward_pct")
                .map_err(ApiError::from_db)?,
            max_slope_reverse_pct: seg
                .try_get("max_slope_reverse_pct")
                .map_err(ApiError::from_db)?,
            soil_susceptibility_idx: seg
                .try_get("soil_susceptibility_idx")
                .map_err(ApiError::from_db)?,
            distance_to_ravine_m: Decimal::from(distance_to_ravine_m),
            is_historical_hotspot: seg
                .try_get("is_historical_hotspot")
                .map_err(ApiError::from_db)?,
        };
        let weather_input = WeatherInput {
            precip_24h_mm: obs.precip_24h_mm,
            precip_72h_mm: obs.precip_72h_mm,
            observed_at: obs.observed_at,
        };

        let ivv_out = calculate_ivv(
            &model_params,
            &geo_input,
            &weather_input,
            Some(now),
            Some(now),
        )
        .map_err(map_ivv_error)?;

        let observation_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO weather_observations (observation_id, segment_id, source_name, precip_24h_mm, precip_72h_mm, observed_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(observation_id)
        .bind(obs.segment_id)
        .bind(&obs.source_name)
        .bind(obs.precip_24h_mm)
        .bind(obs.precip_72h_mm)
        .bind(obs.observed_at)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        let assessment_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO ivv_assessments (assessment_id, observation_id, model_version_id, ivv_score, calculated_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(assessment_id)
        .bind(observation_id)
        .bind(model_version_id)
        .bind(ivv_out.ivv_score)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        append_sync_change(
            &mut tx,
            "IVV_ASSESSMENT",
            assessment_id,
            "UPSERT",
            Some(json!({
                "assessment_id": assessment_id,
                "segment_id": obs.segment_id,
                "ivv_score": ivv_out.ivv_score,
                "risk_level": ivv_out.risk_level,
                "model_version_tag": model_params.version_tag,
                "observed_at": obs.observed_at,
                "calculated_at": now,
            })),
        )
        .await?;

        assessments_created += 1;

        if ivv_out.risk_level == RiskLevel::Red {
            let reval_rows = sqlx::query(
                "UPDATE transfer_corridors tc
                 SET requires_revalidation = TRUE
                 FROM road_segments rs
                 WHERE rs.segment_id = $1
                   AND tc.requires_revalidation = FALSE
                   AND (
                       tc.side_a_node_id IN ($2, $3)
                       OR tc.side_b_node_id IN ($2, $3)
                       OR ST_DWithin(tc.path_geom::geography, rs.geom::geography, 500)
                   )
                 RETURNING tc.corridor_id",
            )
            .bind(obs.segment_id)
            .bind(origin_node_id)
            .bind(destination_node_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;

            for r in reval_rows {
                let corridor_id: Uuid = r.try_get("corridor_id").map_err(ApiError::from_db)?;
                append_sync_change(
                    &mut tx,
                    "TRANSFER_CORRIDOR",
                    corridor_id,
                    "UPSERT",
                    Some(json!({
                        "corridor_id": corridor_id,
                        "requires_revalidation": true,
                        "trigger_segment_id": obs.segment_id,
                    })),
                )
                .await?;
                revalidations_triggered += 1;
            }
        }
    }

    tx.commit().await.map_err(ApiError::from_db)?;

    Ok((
        StatusCode::CREATED,
        Json(IvvRecalculateResponse {
            model_version_tag: model_params.version_tag,
            assessments_created,
            revalidations_triggered,
        }),
    ))
}
