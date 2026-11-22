#![forbid(unsafe_code)]

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use paradixe_domain::RiskLevel;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{ApiError, ValidatedJson, ValidatedPath, ValidatedQuery},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct SubscriptionDto {
    pub subscription_id: Uuid,
    pub label: String,
    pub region_iso: Option<String>,
    pub corridor_name: Option<String>,
    pub min_risk_level: String,
    pub channel: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SubscriptionsListResponse {
    pub subscriptions: Vec<SubscriptionDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSubscriptionRequest {
    pub label: String,
    pub region_iso: Option<String>,
    pub corridor_name: Option<String>,
    pub min_risk_level: String,
    pub channel: String,
}

pub async fn list_subscriptions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<SubscriptionsListResponse>, ApiError> {
    let rows = sqlx::query(
        "SELECT subscription_id, label, region_iso, corridor_name, min_risk_level, channel, is_active, created_at
         FROM corridor_alert_subscriptions
         WHERE user_id = $1
         ORDER BY created_at DESC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut subscriptions = Vec::with_capacity(rows.len());
    for row in rows {
        subscriptions.push(SubscriptionDto {
            subscription_id: row.try_get("subscription_id").map_err(ApiError::from_db)?,
            label: row.try_get("label").map_err(ApiError::from_db)?,
            region_iso: row.try_get("region_iso").map_err(ApiError::from_db)?,
            corridor_name: row.try_get("corridor_name").map_err(ApiError::from_db)?,
            min_risk_level: row.try_get("min_risk_level").map_err(ApiError::from_db)?,
            channel: row.try_get("channel").map_err(ApiError::from_db)?,
            is_active: row.try_get("is_active").map_err(ApiError::from_db)?,
            created_at: row.try_get("created_at").map_err(ApiError::from_db)?,
        });
    }

    Ok(Json(SubscriptionsListResponse { subscriptions }))
}

pub async fn create_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateSubscriptionRequest>,
) -> Result<(StatusCode, Json<SubscriptionDto>), ApiError> {
    let label = body.label.trim();
    if label.is_empty() || label.len() > 120 {
        return Err(ApiError::validation("label must be 1..=120 chars"));
    }
    if !matches!(body.min_risk_level.as_str(), "AMBER" | "RED" | "BLOCKED") {
        return Err(ApiError::validation(
            "min_risk_level must be AMBER, RED, or BLOCKED",
        ));
    }
    if !matches!(body.channel.as_str(), "WHATSAPP_BOT" | "SMS") {
        return Err(ApiError::validation("channel must be WHATSAPP_BOT or SMS"));
    }

    let region_iso = body
        .region_iso
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase().chars().take(16).collect::<String>());
    let corridor_name = body
        .corridor_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(100).collect::<String>());

    let subscription_id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO corridor_alert_subscriptions (
            subscription_id, user_id, label, region_iso, corridor_name, min_risk_level, channel
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING created_at",
    )
    .bind(subscription_id)
    .bind(auth.user_id)
    .bind(label)
    .bind(&region_iso)
    .bind(&corridor_name)
    .bind(&body.min_risk_level)
    .bind(&body.channel)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok((
        StatusCode::CREATED,
        Json(SubscriptionDto {
            subscription_id,
            label: label.to_string(),
            region_iso,
            corridor_name,
            min_risk_level: body.min_risk_level,
            channel: body.channel,
            is_active: true,
            created_at: row.try_get("created_at").map_err(ApiError::from_db)?,
        }),
    ))
}

pub async fn delete_subscription(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(subscription_id): ValidatedPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let res = sqlx::query(
        "DELETE FROM corridor_alert_subscriptions
         WHERE subscription_id = $1 AND user_id = $2",
    )
    .bind(subscription_id)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("Alert subscription not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForecastQuery {
    pub region: Option<String>,
    pub horizon_hours: Option<i16>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ForecastSummaryCounts {
    pub green: usize,
    pub amber: usize,
    pub red: usize,
}

#[derive(Debug, Serialize)]
pub struct CorridorForecastItemDto {
    pub segment_id: Uuid,
    pub corridor_name: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub precip_24h_mm: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub precip_72h_mm: Decimal,
    #[serde(with = "rust_decimal::serde::float")]
    pub predicted_ivv_score: Decimal,
    pub predicted_risk_level: RiskLevel,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CorridorForecastResponse {
    pub region: Option<String>,
    pub horizon_hours: i16,
    pub advisory_only: bool,
    pub generated_at: Option<DateTime<Utc>>,
    pub summary: ForecastSummaryCounts,
    pub items: Vec<CorridorForecastItemDto>,
}

pub async fn get_corridor_forecast(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<ForecastQuery>,
) -> Result<Json<CorridorForecastResponse>, ApiError> {
    let horizon_hours = query.horizon_hours.unwrap_or(24);
    if !matches!(horizon_hours, 24 | 48 | 72) {
        return Err(ApiError::validation("horizon_hours must be 24, 48, or 72"));
    }
    let limit = query.limit.unwrap_or(2000).clamp(1, 5000);
    let region = query
        .region
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let rows = sqlx::query(
        "SELECT wf.segment_id, rs.corridor_name, wf.precip_24h_mm, wf.precip_72h_mm,
                wf.predicted_ivv_score, wf.predicted_risk_level, wf.generated_at
         FROM weather_forecasts wf
         JOIN road_segments rs ON rs.segment_id = wf.segment_id
         WHERE wf.horizon_hours = $1
           AND rs.is_topology_verified = TRUE
           AND (
               $2::text IS NULL
               OR EXISTS (
                   SELECT 1 FROM territorial_units tu
                   WHERE (UPPER(tu.iso_code) = UPPER($2) OR UPPER(tu.ubigeo_code) = UPPER($2))
                     AND ST_Intersects(rs.geom, tu.bbox_geom)
               )
           )
         ORDER BY wf.predicted_ivv_score DESC, rs.length_km DESC, wf.segment_id ASC
         LIMIT $3",
    )
    .bind(horizon_hours)
    .bind(&region)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut summary = ForecastSummaryCounts {
        green: 0,
        amber: 0,
        red: 0,
    };
    let mut generated_at: Option<DateTime<Utc>> = None;
    let mut items = Vec::with_capacity(rows.len());

    for row in rows {
        let risk_str: String = row
            .try_get("predicted_risk_level")
            .map_err(ApiError::from_db)?;
        let predicted_risk_level = match risk_str.as_str() {
            "GREEN" => {
                summary.green += 1;
                RiskLevel::Green
            }
            "AMBER" => {
                summary.amber += 1;
                RiskLevel::Amber
            }
            "RED" => {
                summary.red += 1;
                RiskLevel::Red
            }
            _ => RiskLevel::Unknown,
        };
        let gen_ts: DateTime<Utc> = row.try_get("generated_at").map_err(ApiError::from_db)?;
        if generated_at.is_none_or(|prev| gen_ts > prev) {
            generated_at = Some(gen_ts);
        }

        items.push(CorridorForecastItemDto {
            segment_id: row.try_get("segment_id").map_err(ApiError::from_db)?,
            corridor_name: row.try_get("corridor_name").map_err(ApiError::from_db)?,
            precip_24h_mm: row.try_get("precip_24h_mm").map_err(ApiError::from_db)?,
            precip_72h_mm: row.try_get("precip_72h_mm").map_err(ApiError::from_db)?,
            predicted_ivv_score: row
                .try_get("predicted_ivv_score")
                .map_err(ApiError::from_db)?,
            predicted_risk_level,
            generated_at: gen_ts,
        });
    }

    Ok(Json(CorridorForecastResponse {
        region,
        horizon_hours,
        advisory_only: true,
        generated_at,
        summary,
        items,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsQuery {
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RiskBreakdownDto {
    pub green: i64,
    pub amber: i64,
    pub red: i64,
    pub unknown: i64,
}

#[derive(Debug, Serialize)]
pub struct AnalyticsSummaryResponse {
    pub region: Option<String>,
    pub generated_at: DateTime<Utc>,
    pub total_segments: i64,
    pub certified_segments: i64,
    pub certified_pct: f64,
    pub risk_counts: RiskBreakdownDto,
    pub active_incidents_total: i64,
    pub blocked_segments: i64,
    pub partial_segments: i64,
    pub active_bridges: i64,
    pub active_windows: i64,
    pub verified_transfer_corridors: i64,
    pub hospitals_count: i64,
    pub regions_count: i64,
    pub provinces_count: i64,
    pub districts_count: i64,
}

pub async fn get_analytics_summary(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<AnalyticsQuery>,
) -> Result<Json<AnalyticsSummaryResponse>, ApiError> {
    let region = query
        .region
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let seg_row = sqlx::query(
        "WITH scoped AS (
            SELECT rs.segment_id, rs.attributes_verified_at
            FROM road_segments rs
            WHERE rs.is_topology_verified = TRUE
              AND (
                  $1::text IS NULL
                  OR EXISTS (
                      SELECT 1 FROM territorial_units tu
                      WHERE (UPPER(tu.iso_code) = UPPER($1) OR UPPER(tu.ubigeo_code) = UPPER($1))
                        AND ST_Intersects(rs.geom, tu.bbox_geom)
                  )
              )
        ),
        enriched AS (
            SELECT
                s.segment_id,
                s.attributes_verified_at,
                latest_ivv.ivv_score,
                latest_ivv.calculated_at,
                latest_ivv.amber_threshold,
                latest_ivv.red_threshold,
                latest_ivv.validity_hours,
                active_inc.impact_passability
            FROM scoped s
            LEFT JOIN LATERAL (
                SELECT ia.ivv_score, ia.calculated_at, imv.amber_threshold, imv.red_threshold, imv.validity_hours
                FROM weather_observations wo
                JOIN ivv_assessments ia ON ia.observation_id = wo.observation_id
                JOIN ivv_model_versions imv ON imv.model_version_id = ia.model_version_id
                WHERE wo.segment_id = s.segment_id
                ORDER BY wo.observed_at DESC, ia.calculated_at DESC
                LIMIT 1
            ) latest_ivv ON TRUE
            LEFT JOIN LATERAL (
                SELECT i.impact_passability
                FROM incidents i
                WHERE i.segment_id = s.segment_id
                  AND i.verification_state IN ('SUSPECTED', 'CONFIRMED')
                ORDER BY i.detected_at DESC
                LIMIT 1
            ) active_inc ON TRUE
        )
        SELECT
            count(*)::bigint AS total_segments,
            count(*) FILTER (WHERE attributes_verified_at IS NOT NULL)::bigint AS certified_segments,
            count(*) FILTER (
                WHERE ivv_score IS NOT NULL
                  AND calculated_at + (validity_hours * INTERVAL '1 hour') >= NOW()
                  AND ivv_score < amber_threshold
            )::bigint AS green_cnt,
            count(*) FILTER (
                WHERE ivv_score IS NOT NULL
                  AND calculated_at + (validity_hours * INTERVAL '1 hour') >= NOW()
                  AND ivv_score >= amber_threshold
                  AND ivv_score < red_threshold
            )::bigint AS amber_cnt,
            count(*) FILTER (
                WHERE ivv_score IS NOT NULL
                  AND calculated_at + (validity_hours * INTERVAL '1 hour') >= NOW()
                  AND ivv_score >= red_threshold
            )::bigint AS red_cnt,
            count(*) FILTER (
                WHERE ivv_score IS NULL
                   OR calculated_at + (validity_hours * INTERVAL '1 hour') < NOW()
            )::bigint AS unknown_cnt,
            count(*) FILTER (WHERE impact_passability IS NOT NULL)::bigint AS active_incidents_total,
            count(*) FILTER (WHERE impact_passability = 'BLOCKED')::bigint AS blocked_segments,
            count(*) FILTER (WHERE impact_passability = 'PARTIAL_ONE_LANE')::bigint AS partial_segments
        FROM enriched",
    )
    .bind(&region)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let ops_row = sqlx::query(
        "SELECT
            (SELECT count(*)::bigint FROM logistic_bridges WHERE status = 'ACTIVE') AS active_bridges,
            (SELECT count(*)::bigint FROM humanitarian_windows WHERE status = 'ACTIVE') AS active_windows,
            (SELECT count(*)::bigint FROM transfer_corridors WHERE is_verified_safe = TRUE AND requires_revalidation = FALSE) AS verified_corridors,
            (SELECT count(*)::bigint FROM road_nodes rn WHERE rn.node_type = 'HOSPITAL'
              AND ($1::text IS NULL OR EXISTS (
                  SELECT 1 FROM territorial_units tu
                  WHERE (UPPER(tu.iso_code) = UPPER($1) OR UPPER(tu.ubigeo_code) = UPPER($1))
                    AND ST_Intersects(rn.location_geom, tu.bbox_geom)
              ))) AS hospitals_count,
            (SELECT count(*)::bigint FROM territorial_units WHERE admin_level = 4 AND is_enabled = TRUE) AS regions_count,
            (SELECT count(*)::bigint FROM territorial_units WHERE admin_level = 6 AND is_enabled = TRUE) AS provinces_count,
            (SELECT count(*)::bigint FROM territorial_units WHERE admin_level = 8 AND is_enabled = TRUE) AS districts_count",
    )
    .bind(&region)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let total_segments: i64 = seg_row
        .try_get("total_segments")
        .map_err(ApiError::from_db)?;
    let certified_segments: i64 = seg_row
        .try_get("certified_segments")
        .map_err(ApiError::from_db)?;
    let certified_pct = if total_segments > 0 {
        ((certified_segments as f64 / total_segments as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    Ok(Json(AnalyticsSummaryResponse {
        region,
        generated_at: Utc::now(),
        total_segments,
        certified_segments,
        certified_pct,
        risk_counts: RiskBreakdownDto {
            green: seg_row.try_get("green_cnt").map_err(ApiError::from_db)?,
            amber: seg_row.try_get("amber_cnt").map_err(ApiError::from_db)?,
            red: seg_row.try_get("red_cnt").map_err(ApiError::from_db)?,
            unknown: seg_row.try_get("unknown_cnt").map_err(ApiError::from_db)?,
        },
        active_incidents_total: seg_row
            .try_get("active_incidents_total")
            .map_err(ApiError::from_db)?,
        blocked_segments: seg_row
            .try_get("blocked_segments")
            .map_err(ApiError::from_db)?,
        partial_segments: seg_row
            .try_get("partial_segments")
            .map_err(ApiError::from_db)?,
        active_bridges: ops_row
            .try_get("active_bridges")
            .map_err(ApiError::from_db)?,
        active_windows: ops_row
            .try_get("active_windows")
            .map_err(ApiError::from_db)?,
        verified_transfer_corridors: ops_row
            .try_get("verified_corridors")
            .map_err(ApiError::from_db)?,
        hospitals_count: ops_row
            .try_get("hospitals_count")
            .map_err(ApiError::from_db)?,
        regions_count: ops_row
            .try_get("regions_count")
            .map_err(ApiError::from_db)?,
        provinces_count: ops_row
            .try_get("provinces_count")
            .map_err(ApiError::from_db)?,
        districts_count: ops_row
            .try_get("districts_count")
            .map_err(ApiError::from_db)?,
    }))
}
