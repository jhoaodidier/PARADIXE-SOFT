#![forbid(unsafe_code)]

use std::net::IpAddr;

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use paradixe_domain::RiskLevel;
use paradixe_ivv::evaluate_ivv_freshness;
use rand::RngCore;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::{write_audit_log, AuthUser, RoleCode},
    error::{ApiError, ValidatedJson, ValidatedPath},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct ApiKeySummaryDto {
    pub api_key_id: Uuid,
    pub organization_name: String,
    pub key_prefix: String,
    pub role_code: RoleCode,
    pub rate_limit_rpm: i32,
    pub is_active: bool,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeysListResponse {
    pub api_keys: Vec<ApiKeySummaryDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateApiKeyRequest {
    pub organization_name: String,
    pub role_code: Option<RoleCode>,
    pub rate_limit_rpm: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub api_key_id: Uuid,
    pub organization_name: String,
    pub key_prefix: String,
    pub raw_api_key: String,
    pub role_code: RoleCode,
    pub rate_limit_rpm: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct WebhookEndpointDto {
    pub webhook_id: Uuid,
    pub organization_name: String,
    pub target_url: String,
    pub masked_secret: String,
    pub region_iso: Option<String>,
    pub min_risk_level: String,
    pub is_active: bool,
    pub last_delivery_status: Option<i16>,
    pub last_delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct WebhooksListResponse {
    pub webhooks: Vec<WebhookEndpointDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateWebhookRequest {
    pub organization_name: String,
    pub target_url: String,
    pub region_iso: Option<String>,
    pub min_risk_level: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateWebhookResponse {
    pub webhook_id: Uuid,
    pub organization_name: String,
    pub target_url: String,
    pub signing_secret: String,
    pub region_iso: Option<String>,
    pub min_risk_level: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchCorridorStatusRequest {
    pub region: Option<String>,
    pub corridor_names: Option<Vec<String>>,
    pub segment_ids: Option<Vec<Uuid>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BatchStatusSummaryDto {
    pub open: i64,
    pub caution: i64,
    pub partial: i64,
    pub blocked: i64,
    pub verified_segments: i64,
}

#[derive(Debug, Serialize)]
pub struct BatchCorridorStatusItemDto {
    pub segment_id: Uuid,
    pub corridor_name: String,
    pub surface_type: String,
    pub passability_state: String,
    pub risk_level: RiskLevel,
    #[serde(with = "rust_decimal::serde::float_option")]
    pub ivv_score: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::float")]
    pub length_km: Decimal,
    pub is_attributes_verified: bool,
    pub is_stale: bool,
}

#[derive(Debug, Serialize)]
pub struct BatchCorridorStatusResponse {
    pub generated_at: DateTime<Utc>,
    pub region: Option<String>,
    pub total_matched: usize,
    pub summary: BatchStatusSummaryDto,
    pub items: Vec<BatchCorridorStatusItemDto>,
}

pub fn sha256_hex_bytes(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

fn generate_prefixed_secret(prefix: &str) -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let raw = format!("{prefix}{}", hex::encode(bytes));
    let shown_prefix: String = raw.chars().take(16).collect();
    (raw, shown_prefix)
}

fn mask_signing_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 6 {
        return "whsec_****".to_string();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("whsec_****{tail}")
}

fn is_forbidden_ip(ip: IpAddr, allow_local_dev: bool) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || (!allow_local_dev && (v4.is_loopback() || v4.is_private()))
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            let is_unique_local = (seg0 & 0xfe00) == 0xfc00;
            v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unicast_link_local()
                || (!allow_local_dev && (v6.is_loopback() || is_unique_local))
        }
    }
}

/// Validates an outbound B2B webhook URL against SSRF, credential smuggling, and plaintext transport.
pub fn validate_webhook_url(raw: &str, is_production: bool) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 512 {
        return Err(ApiError::validation(
            "target_url must be between 1 and 512 characters",
        ));
    }

    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| ApiError::validation("target_url is not a valid URL"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiError::validation(
            "target_url must not contain embedded credentials",
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| ApiError::validation("target_url must include a host"))?;
    let host_lower = host.trim_matches(['[', ']']).to_ascii_lowercase();

    // Always block cloud metadata and unroutable zero hosts even in development.
    if host_lower == "metadata.google.internal"
        || host_lower == "metadata"
        || host_lower == "169.254.169.254"
        || host_lower == "0.0.0.0"
        || (is_production && (host_lower.ends_with(".internal") || host_lower.ends_with(".local")))
    {
        return Err(ApiError::validation("target_url host is forbidden (SSRF)"));
    }

    let is_local_dev_host =
        !is_production && (host_lower == "127.0.0.1" || host_lower == "localhost");

    match parsed.scheme() {
        "https" => {}
        "http" if is_local_dev_host => {}
        "http" => {
            return Err(ApiError::validation(
                "target_url must use https:// (http:// is allowed only for localhost in non-production)",
            ));
        }
        _ => {
            return Err(ApiError::validation("target_url scheme must be https"));
        }
    }

    if let Ok(ip) = host_lower.parse::<IpAddr>() {
        if is_forbidden_ip(ip, is_local_dev_host) {
            return Err(ApiError::validation(
                "target_url resolves to a restricted network range",
            ));
        }
    }

    Ok(parsed.to_string())
}

pub async fn list_api_keys(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiKeysListResponse>, ApiError> {
    let is_admin = auth.role == RoleCode::CoerAdmin;
    let rows = sqlx::query(
        "SELECT api_key_id, organization_name, key_prefix, role_code, rate_limit_rpm,
                is_active, last_used_at, created_at, revoked_at
         FROM b2b_api_keys
         WHERE ($1 = TRUE OR owner_user_id = $2)
         ORDER BY created_at DESC",
    )
    .bind(is_admin)
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut api_keys = Vec::with_capacity(rows.len());
    for row in rows {
        let role_str: String = row.try_get("role_code").map_err(ApiError::from_db)?;
        let role_code =
            RoleCode::parse(&role_str).ok_or_else(|| ApiError::internal("Invalid role_code"))?;
        api_keys.push(ApiKeySummaryDto {
            api_key_id: row.try_get("api_key_id").map_err(ApiError::from_db)?,
            organization_name: row
                .try_get("organization_name")
                .map_err(ApiError::from_db)?,
            key_prefix: row.try_get("key_prefix").map_err(ApiError::from_db)?,
            role_code,
            rate_limit_rpm: row.try_get("rate_limit_rpm").map_err(ApiError::from_db)?,
            is_active: row.try_get("is_active").map_err(ApiError::from_db)?,
            last_used_at: row.try_get("last_used_at").map_err(ApiError::from_db)?,
            created_at: row.try_get("created_at").map_err(ApiError::from_db)?,
            revoked_at: row.try_get("revoked_at").map_err(ApiError::from_db)?,
        });
    }

    Ok(Json(ApiKeysListResponse { api_keys }))
}

pub async fn create_api_key(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin])?;

    let org = body.organization_name.trim().to_string();
    if org.is_empty() || org.len() > 120 {
        return Err(ApiError::validation(
            "organization_name must be 1..=120 chars",
        ));
    }

    let role_code = body.role_code.unwrap_or(RoleCode::Driver);
    if !matches!(
        role_code,
        RoleCode::Driver | RoleCode::HealthOperator | RoleCode::CivilDefense
    ) {
        return Err(ApiError::validation(
            "API key role_code must be DRIVER, HEALTH_OPERATOR, or CIVIL_DEFENSE",
        ));
    }

    let rate_limit_rpm = body.rate_limit_rpm.unwrap_or(120);
    if !(10..=6000).contains(&rate_limit_rpm) {
        return Err(ApiError::validation(
            "rate_limit_rpm must be between 10 and 6000",
        ));
    }

    let (raw_api_key, key_prefix) = generate_prefixed_secret("pxk_live_");
    let key_hash = sha256_hex_bytes(&raw_api_key);
    let api_key_id = Uuid::new_v4();

    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;
    let row = sqlx::query(
        "INSERT INTO b2b_api_keys (
            api_key_id, owner_user_id, organization_name, key_prefix, key_hash, role_code, rate_limit_rpm
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING created_at",
    )
    .bind(api_key_id)
    .bind(auth.user_id)
    .bind(&org)
    .bind(&key_prefix)
    .bind(&key_hash)
    .bind(role_code.as_str())
    .bind(rate_limit_rpm)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "B2B_API_KEY_CREATED",
        "B2B_API_KEY",
        api_key_id,
        None,
        json!({
            "organization_name": org,
            "key_prefix": key_prefix,
            "role_code": role_code.as_str(),
            "rate_limit_rpm": rate_limit_rpm,
        }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            api_key_id,
            organization_name: org,
            key_prefix,
            raw_api_key,
            role_code,
            rate_limit_rpm,
            created_at: row.try_get("created_at").map_err(ApiError::from_db)?,
        }),
    ))
}

pub async fn revoke_api_key(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(api_key_id): ValidatedPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let is_admin = auth.role == RoleCode::CoerAdmin;
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let res = sqlx::query(
        "UPDATE b2b_api_keys
         SET is_active = FALSE, revoked_at = COALESCE(revoked_at, NOW())
         WHERE api_key_id = $1 AND ($2 = TRUE OR owner_user_id = $3)",
    )
    .bind(api_key_id)
    .bind(is_admin)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("B2B API key not found"));
    }

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "B2B_API_KEY_REVOKED",
        "B2B_API_KEY",
        api_key_id,
        None,
        json!({ "api_key_id": api_key_id }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_webhooks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<WebhooksListResponse>, ApiError> {
    let is_admin = auth.role == RoleCode::CoerAdmin;
    let rows = sqlx::query(
        "SELECT webhook_id, organization_name, target_url, signing_secret, region_iso,
                min_risk_level, is_active, last_delivery_status, last_delivered_at, created_at
         FROM b2b_webhook_endpoints
         WHERE ($1 = TRUE OR owner_user_id = $2)
         ORDER BY created_at DESC",
    )
    .bind(is_admin)
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut webhooks = Vec::with_capacity(rows.len());
    for row in rows {
        let raw_secret: String = row.try_get("signing_secret").map_err(ApiError::from_db)?;
        webhooks.push(WebhookEndpointDto {
            webhook_id: row.try_get("webhook_id").map_err(ApiError::from_db)?,
            organization_name: row
                .try_get("organization_name")
                .map_err(ApiError::from_db)?,
            target_url: row.try_get("target_url").map_err(ApiError::from_db)?,
            masked_secret: mask_signing_secret(&raw_secret),
            region_iso: row.try_get("region_iso").map_err(ApiError::from_db)?,
            min_risk_level: row.try_get("min_risk_level").map_err(ApiError::from_db)?,
            is_active: row.try_get("is_active").map_err(ApiError::from_db)?,
            last_delivery_status: row
                .try_get("last_delivery_status")
                .map_err(ApiError::from_db)?,
            last_delivered_at: row
                .try_get("last_delivered_at")
                .map_err(ApiError::from_db)?,
            created_at: row.try_get("created_at").map_err(ApiError::from_db)?,
        });
    }

    Ok(Json(WebhooksListResponse { webhooks }))
}

pub async fn create_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<CreateWebhookResponse>), ApiError> {
    auth.require_roles(&[
        RoleCode::CoerAdmin,
        RoleCode::CivilDefense,
        RoleCode::HealthOperator,
        RoleCode::Driver,
    ])?;

    let org = body.organization_name.trim().to_string();
    if org.is_empty() || org.len() > 120 {
        return Err(ApiError::validation(
            "organization_name must be 1..=120 chars",
        ));
    }

    let target_url = validate_webhook_url(&body.target_url, state.config.is_production)?;
    let min_risk_level = body
        .min_risk_level
        .as_deref()
        .unwrap_or("RED")
        .trim()
        .to_ascii_uppercase();
    if !matches!(min_risk_level.as_str(), "AMBER" | "RED" | "BLOCKED") {
        return Err(ApiError::validation(
            "min_risk_level must be AMBER, RED, or BLOCKED",
        ));
    }

    let region_iso = body
        .region_iso
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase().chars().take(16).collect::<String>());

    let (signing_secret, _) = generate_prefixed_secret("whsec_");
    let webhook_id = Uuid::new_v4();

    let row = sqlx::query(
        "INSERT INTO b2b_webhook_endpoints (
            webhook_id, owner_user_id, organization_name, target_url, signing_secret, region_iso, min_risk_level
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING created_at",
    )
    .bind(webhook_id)
    .bind(auth.user_id)
    .bind(&org)
    .bind(&target_url)
    .bind(&signing_secret)
    .bind(&region_iso)
    .bind(&min_risk_level)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateWebhookResponse {
            webhook_id,
            organization_name: org,
            target_url,
            signing_secret,
            region_iso,
            min_risk_level,
            created_at: row.try_get("created_at").map_err(ApiError::from_db)?,
        }),
    ))
}

pub async fn delete_webhook(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(webhook_id): ValidatedPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    let is_admin = auth.role == RoleCode::CoerAdmin;
    let res = sqlx::query(
        "DELETE FROM b2b_webhook_endpoints
         WHERE webhook_id = $1 AND ($2 = TRUE OR owner_user_id = $3)",
    )
    .bind(webhook_id)
    .bind(is_admin)
    .bind(auth.user_id)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    if res.rows_affected() == 0 {
        return Err(ApiError::not_found("B2B webhook endpoint not found"));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn batch_corridors_status(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<BatchCorridorStatusRequest>,
) -> Result<Json<BatchCorridorStatusResponse>, ApiError> {
    state.rate_limiter.check_and_record(
        &format!("b2b_status:{}", auth.user_id),
        120,
        chrono::Duration::minutes(1),
    )?;

    let region = body
        .region
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let corridor_names: Option<Vec<String>> = body
        .corridor_names
        .map(|names| {
            names
                .into_iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .take(40)
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty());
    let segment_ids = body
        .segment_ids
        .map(|ids| ids.into_iter().take(100).collect::<Vec<_>>())
        .filter(|v| !v.is_empty());
    let limit = body.limit.unwrap_or(100).clamp(1, 200);
    let now = Utc::now();

    let rows = sqlx::query(
        "SELECT rs.segment_id, rs.corridor_name, rs.surface_type, rs.length_km,
                rs.attributes_verified_at,
                latest_ivv.ivv_score, latest_ivv.calculated_at, latest_ivv.validity_hours,
                latest_ivv.amber_threshold, latest_ivv.red_threshold,
                active_inc.verification_state AS active_state,
                active_inc.impact_passability AS active_pass
         FROM road_segments rs
         LEFT JOIN LATERAL (
            SELECT ia.ivv_score, ia.calculated_at, imv.validity_hours,
                   imv.amber_threshold, imv.red_threshold
            FROM weather_observations wo
            JOIN ivv_assessments ia ON ia.observation_id = wo.observation_id
            JOIN ivv_model_versions imv ON imv.model_version_id = ia.model_version_id
            WHERE wo.segment_id = rs.segment_id
            ORDER BY wo.observed_at DESC, ia.calculated_at DESC
            LIMIT 1
         ) latest_ivv ON TRUE
         LEFT JOIN LATERAL (
            SELECT i.verification_state, i.impact_passability
            FROM incidents i
            WHERE i.segment_id = rs.segment_id
              AND i.verification_state IN ('SUSPECTED', 'CONFIRMED')
            ORDER BY i.detected_at DESC
            LIMIT 1
         ) active_inc ON TRUE
         WHERE rs.is_topology_verified = TRUE
           AND ($1::text IS NULL OR EXISTS (
               SELECT 1 FROM territorial_units tu
               WHERE (UPPER(tu.iso_code) = UPPER($1) OR UPPER(tu.ubigeo_code) = UPPER($1))
                 AND ST_Intersects(rs.geom, tu.bbox_geom)
           ))
           AND ($2::text[] IS NULL OR LOWER(TRIM(rs.corridor_name)) = ANY($2))
           AND ($3::uuid[] IS NULL OR rs.segment_id = ANY($3))
         ORDER BY CASE
             WHEN rs.attributes_verified_at IS NOT NULL THEN 0 ELSE 1
         END ASC, rs.length_km DESC, rs.segment_id ASC
         LIMIT $4",
    )
    .bind(&region)
    .bind(&corridor_names)
    .bind(&segment_ids)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let mut summary = BatchStatusSummaryDto {
        open: 0,
        caution: 0,
        partial: 0,
        blocked: 0,
        verified_segments: 0,
    };
    let mut items = Vec::with_capacity(rows.len());

    for row in rows {
        let active_state: Option<String> =
            row.try_get("active_state").map_err(ApiError::from_db)?;
        let active_pass: Option<String> = row.try_get("active_pass").map_err(ApiError::from_db)?;
        let passability_state = match (active_state.as_deref(), active_pass.as_deref()) {
            (Some("CONFIRMED"), Some("BLOCKED")) => {
                summary.blocked += 1;
                "BLOCKED".to_string()
            }
            (Some("CONFIRMED"), Some("PARTIAL_ONE_LANE")) => {
                summary.partial += 1;
                "PARTIAL_ONE_LANE".to_string()
            }
            (Some("SUSPECTED"), _) | (Some("CONFIRMED"), Some("CAUTION_SUSPECTED")) => {
                summary.caution += 1;
                "CAUTION_SUSPECTED".to_string()
            }
            _ => {
                summary.open += 1;
                "OPEN".to_string()
            }
        };

        let ivv_score: Option<Decimal> = row.try_get("ivv_score").map_err(ApiError::from_db)?;
        let calc_at: Option<DateTime<Utc>> =
            row.try_get("calculated_at").map_err(ApiError::from_db)?;
        let validity_hours: Option<i16> =
            row.try_get("validity_hours").map_err(ApiError::from_db)?;
        let amber_t: Option<Decimal> = row.try_get("amber_threshold").map_err(ApiError::from_db)?;
        let red_t: Option<Decimal> = row.try_get("red_threshold").map_err(ApiError::from_db)?;
        let is_stale = match (ivv_score, calc_at, validity_hours) {
            (Some(_), Some(c), Some(vh)) => evaluate_ivv_freshness(c, i64::from(vh), now),
            _ => true,
        };
        let risk_level = if is_stale {
            RiskLevel::Unknown
        } else {
            match (ivv_score, amber_t, red_t) {
                (Some(sc), Some(_), Some(red)) if sc >= red => RiskLevel::Red,
                (Some(sc), Some(amber), Some(_)) if sc >= amber => RiskLevel::Amber,
                (Some(_), Some(_), Some(_)) => RiskLevel::Green,
                _ => RiskLevel::Unknown,
            }
        };

        let attrs_verified: Option<DateTime<Utc>> = row
            .try_get("attributes_verified_at")
            .map_err(ApiError::from_db)?;
        let is_attributes_verified = attrs_verified.is_some();
        if is_attributes_verified {
            summary.verified_segments += 1;
        }

        items.push(BatchCorridorStatusItemDto {
            segment_id: row.try_get("segment_id").map_err(ApiError::from_db)?,
            corridor_name: row.try_get("corridor_name").map_err(ApiError::from_db)?,
            surface_type: row.try_get("surface_type").map_err(ApiError::from_db)?,
            passability_state,
            risk_level,
            ivv_score,
            length_km: row.try_get("length_km").map_err(ApiError::from_db)?,
            is_attributes_verified,
            is_stale,
        });
    }

    Ok(Json(BatchCorridorStatusResponse {
        generated_at: now,
        region,
        total_matched: items.len(),
        summary,
        items,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_prefix_and_sha256_are_deterministic() {
        let (raw, prefix) = generate_prefixed_secret("pxk_live_");
        assert!(raw.starts_with("pxk_live_"));
        assert_eq!(prefix.len(), 16);
        assert_eq!(raw.chars().take(16).collect::<String>(), prefix);
        assert_eq!(sha256_hex_bytes(&raw).len(), 64);
        assert_eq!(sha256_hex_bytes(&raw), sha256_hex_bytes(&raw));
    }

    #[test]
    fn webhook_url_blocks_ssrf_and_invalid_schemes() {
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data/", false).is_err());
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data/", true).is_err());
        assert!(validate_webhook_url("https://10.0.0.5/hook", false).is_err());
        assert!(validate_webhook_url("javascript:alert(1)", false).is_err());
        assert!(validate_webhook_url("https://user:pass@evil.com/hook", true).is_err());
        assert!(validate_webhook_url("http://example.com/hook", true).is_err());
        assert!(validate_webhook_url("http://127.0.0.1:9999/hook", true).is_err());

        assert!(validate_webhook_url("http://127.0.0.1:19999/hook", false).is_ok());
        assert!(validate_webhook_url("https://erp.flota.pe/hooks/paradixe", true).is_ok());
    }
}
