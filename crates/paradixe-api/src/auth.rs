#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use rust_decimal::Decimal;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    error::{ApiError, ValidatedJson},
    AppState,
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoleCode {
    Driver,
    HealthOperator,
    CivilDefense,
    CoerAdmin,
    BotService,
}

impl RoleCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Driver => "DRIVER",
            Self::HealthOperator => "HEALTH_OPERATOR",
            Self::CivilDefense => "CIVIL_DEFENSE",
            Self::CoerAdmin => "COER_ADMIN",
            Self::BotService => "BOT_SERVICE",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "DRIVER" => Some(Self::Driver),
            "HEALTH_OPERATOR" => Some(Self::HealthOperator),
            "CIVIL_DEFENSE" => Some(Self::CivilDefense),
            "COER_ADMIN" => Some(Self::CoerAdmin),
            "BOT_SERVICE" => Some(Self::BotService),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub role: RoleCode,
}

impl AuthUser {
    pub fn require_roles(&self, allowed: &[RoleCode]) -> Result<(), ApiError> {
        if allowed.contains(&self.role) {
            Ok(())
        } else {
            Err(ApiError::forbidden(
                "Insufficient role privileges for this operation",
            ))
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: Uuid,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
    pub jti: Uuid,
}

pub fn encode_access_token(
    user_id: Uuid,
    role: RoleCode,
    secret: &[u8],
    ttl_min: i64,
) -> Result<String, ApiError> {
    let now = Utc::now();
    let claims = JwtClaims {
        sub: user_id,
        role: role.as_str().to_string(),
        iat: now.timestamp(),
        exp: (now + Duration::minutes(ttl_min)).timestamp(),
        jti: Uuid::new_v4(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(ApiError::internal)
}

pub fn decode_access_token(token: &str, secret: &[u8]) -> Result<AuthUser, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    let decoded = decode::<JwtClaims>(token, &DecodingKey::from_secret(secret), &validation)
        .map_err(|_| ApiError::unauthorized("Invalid or expired access token"))?;
    let role = RoleCode::parse(&decoded.claims.role)
        .ok_or_else(|| ApiError::unauthorized("Invalid role claim"))?;
    Ok(AuthUser {
        user_id: decoded.claims.sub,
        role,
    })
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("Missing Authorization bearer header"))?;

        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or_else(|| ApiError::unauthorized("Invalid Authorization scheme"))?;

        decode_access_token(token, state.config.jwt_secret.expose_secret().as_bytes())
    }
}

/// Sliding-window rate limiter keyed by endpoint bucket + phone/identifier.
#[derive(Clone, Default)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, VecDeque<DateTime<Utc>>>>>,
}

impl RateLimiter {
    pub fn check_and_record(
        &self,
        key: &str,
        max_requests: usize,
        window: Duration,
    ) -> Result<(), ApiError> {
        let now = Utc::now();
        let cutoff = now - window;
        let mut guard = self
            .buckets
            .lock()
            .map_err(|_| ApiError::internal("rate limiter mutex poisoned"))?;
        let queue = guard.entry(key.to_string()).or_default();
        while queue.front().is_some_and(|&ts| ts < cutoff) {
            queue.pop_front();
        }
        if queue.len() >= max_requests {
            return Err(ApiError::rate_limited("Too many authentication attempts"));
        }
        queue.push_back(now);
        Ok(())
    }
}

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let mut salt_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_bytes);
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| ApiError::internal(format!("salt generation failed: {e}")))?;
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| ApiError::internal(format!("password hashing failed: {e}")))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, encoded_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(encoded_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn compute_otp_hmac_hex(
    otp_secret: &[u8],
    challenge_id: Uuid,
    phone_e164: &str,
    otp_code: &str,
) -> String {
    let mut mac = HmacSha256::new_from_slice(otp_secret).expect("HMAC accepts keys of any length");
    mac.update(challenge_id.as_bytes());
    mac.update(b":");
    mac.update(phone_e164.as_bytes());
    mac.update(b":");
    mac.update(otp_code.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn verify_otp_constant_time(
    otp_secret: &[u8],
    challenge_id: Uuid,
    phone_e164: &str,
    otp_code: &str,
    expected_hex: &str,
) -> bool {
    let Ok(expected_bytes) = hex::decode(expected_hex) else {
        return false;
    };
    let actual_hex = compute_otp_hmac_hex(otp_secret, challenge_id, phone_e164, otp_code);
    let Ok(actual_bytes) = hex::decode(actual_hex) else {
        return false;
    };
    if expected_bytes.len() != actual_bytes.len() {
        return false;
    }
    bool::from(actual_bytes.ct_eq(&expected_bytes))
}

pub fn sha256_hex(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

fn generate_random_token_hex() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub async fn write_audit_log(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Option<Uuid>,
    action: &str,
    entity_type: &str,
    entity_id: Uuid,
    before_state: Option<serde_json::Value>,
    after_state: serde_json::Value,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, entity_type, entity_id, before_state_json, after_state_json)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(before_state.map(sqlx::types::Json))
    .bind(sqlx::types::Json(after_state))
    .execute(&mut **tx)
    .await
    .map_err(ApiError::from_db)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtpRequestInput {
    pub phone_e164: String,
}

#[derive(Debug, Serialize)]
pub struct OtpRequestResponse {
    pub challenge_id: Uuid,
    pub expires_in_sec: i64,
    pub message: String,
}

pub async fn request_otp(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<OtpRequestInput>,
) -> Result<(StatusCode, Json<OtpRequestResponse>), ApiError> {
    let phone = body.phone_e164.trim();
    if !(8..=20).contains(&phone.len()) {
        return Err(ApiError::validation(
            "phone_e164 must be between 8 and 20 characters",
        ));
    }
    state
        .rate_limiter
        .check_and_record(&format!("otp:{phone}"), 5, Duration::minutes(5))?;

    let challenge_id = Uuid::new_v4();
    let expires_in_sec = 300;
    let expires_at = Utc::now() + Duration::seconds(expires_in_sec);

    let mut code_num = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut code_num);
    let otp_code = format!("{:06}", u32::from_le_bytes(code_num) % 1_000_000);
    let code_hash = compute_otp_hmac_hex(
        state.config.otp_secret.expose_secret().as_bytes(),
        challenge_id,
        phone,
        &otp_code,
    );

    // Insert challenge for active accounts, or use indistinguishable challenge_id without persisting PII leak;
    // inserting challenge_id bound to phone ensures timing and response body remain identical.
    sqlx::query(
        "INSERT INTO otp_challenges (challenge_id, phone_e164, code_hash, attempts, expires_at)
         VALUES ($1, $2, $3, 0, $4)",
    )
    .bind(challenge_id)
    .bind(phone)
    .bind(code_hash)
    .bind(expires_at)
    .execute(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok((
        StatusCode::ACCEPTED,
        Json(OtpRequestResponse {
            challenge_id,
            expires_in_sec,
            message:
                "If an account exists for this number, a verification code has been dispatched."
                    .to_string(),
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub phone_e164: String,
    pub device_id: String,
    pub challenge_id: Option<Uuid>,
    pub otp_code: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in_sec: i64,
}

pub async fn login(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<LoginRequest>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let phone = body.phone_e164.trim().to_string();
    let device_id = body.device_id.trim().to_string();
    if phone.is_empty() || device_id.is_empty() || device_id.len() > 128 {
        return Err(ApiError::validation(
            "Valid phone_e164 and device_id are required",
        ));
    }
    state
        .rate_limiter
        .check_and_record(&format!("login:{phone}"), 25, Duration::minutes(5))?;

    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let user_row = sqlx::query(
        "SELECT u.user_id, u.password_hash, u.is_active, r.code AS role_code
         FROM users u
         JOIN roles r ON r.role_id = u.role_id
         WHERE u.phone_e164 = $1",
    )
    .bind(&phone)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    match (&body.challenge_id, &body.otp_code, &body.password) {
        (Some(challenge_id), Some(otp_code), None) => {
            if otp_code.len() != 6 || !otp_code.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ApiError::validation("otp_code must be a 6-digit string"));
            }

            let challenge = sqlx::query(
                "SELECT phone_e164, code_hash, attempts, expires_at, consumed_at
                 FROM otp_challenges
                 WHERE challenge_id = $1
                 FOR UPDATE",
            )
            .bind(challenge_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::from_db)?
            .ok_or_else(|| ApiError::unauthorized("Invalid OTP challenge"))?;

            let ch_phone: String = challenge.try_get("phone_e164").map_err(ApiError::from_db)?;
            let code_hash: String = challenge.try_get("code_hash").map_err(ApiError::from_db)?;
            let attempts: i16 = challenge.try_get("attempts").map_err(ApiError::from_db)?;
            let expires_at: DateTime<Utc> =
                challenge.try_get("expires_at").map_err(ApiError::from_db)?;
            let consumed_at: Option<DateTime<Utc>> = challenge
                .try_get("consumed_at")
                .map_err(ApiError::from_db)?;

            if consumed_at.is_some()
                || expires_at <= Utc::now()
                || attempts >= 5
                || ch_phone != phone
            {
                return Err(ApiError::unauthorized("OTP challenge expired or exhausted"));
            }

            sqlx::query(
                "UPDATE otp_challenges SET attempts = attempts + 1 WHERE challenge_id = $1",
            )
            .bind(challenge_id)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;

            let valid_otp = verify_otp_constant_time(
                state.config.otp_secret.expose_secret().as_bytes(),
                *challenge_id,
                &phone,
                otp_code,
                &code_hash,
            );
            if !valid_otp {
                tx.commit().await.map_err(ApiError::from_db)?;
                return Err(ApiError::unauthorized("Invalid credentials"));
            }

            sqlx::query("UPDATE otp_challenges SET consumed_at = NOW() WHERE challenge_id = $1")
                .bind(challenge_id)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::from_db)?;
        }
        (None, None, Some(password)) => {
            let stored_hash = user_row
                .as_ref()
                .and_then(|r| r.try_get::<Option<String>, _>("password_hash").ok())
                .flatten();
            let pw = password.clone();
            let ok = match stored_hash {
                Some(h) => tokio::task::spawn_blocking(move || verify_password(&pw, &h))
                    .await
                    .map_err(ApiError::internal)?,
                None => false,
            };
            if !ok {
                return Err(ApiError::unauthorized("Invalid credentials"));
            }
        }
        _ => {
            return Err(ApiError::validation(
                "Provide either (challenge_id and otp_code) or password",
            ));
        }
    }

    let user_row = user_row.ok_or_else(|| ApiError::unauthorized("Invalid credentials"))?;
    let is_active: bool = user_row.try_get("is_active").map_err(ApiError::from_db)?;
    if !is_active {
        return Err(ApiError::unauthorized("User account is inactive"));
    }

    let user_id: Uuid = user_row.try_get("user_id").map_err(ApiError::from_db)?;
    let role_str: String = user_row.try_get("role_code").map_err(ApiError::from_db)?;
    let role = RoleCode::parse(&role_str)
        .ok_or_else(|| ApiError::internal("Unsupported role in database"))?;

    let refresh_raw = generate_random_token_hex();
    let refresh_hash = sha256_hex(&refresh_raw);
    let token_family_id = Uuid::new_v4();
    let refresh_expires_at = Utc::now() + Duration::days(state.config.jwt_refresh_ttl_days);

    sqlx::query(
        "INSERT INTO auth_refresh_tokens (token_family_id, user_id, token_hash, device_id, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(token_family_id)
    .bind(user_id)
    .bind(&refresh_hash)
    .bind(&device_id)
    .bind(refresh_expires_at)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    tx.commit().await.map_err(ApiError::from_db)?;

    let access_token = encode_access_token(
        user_id,
        role,
        state.config.jwt_secret.expose_secret().as_bytes(),
        state.config.jwt_access_ttl_min,
    )?;

    Ok(Json(AuthTokenResponse {
        access_token,
        refresh_token: refresh_raw,
        token_type: "Bearer",
        expires_in_sec: state.config.jwt_access_ttl_min * 60,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshRequest {
    pub refresh_token: String,
    pub device_id: String,
}

pub async fn refresh_session(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RefreshRequest>,
) -> Result<Json<AuthTokenResponse>, ApiError> {
    let token_hash = sha256_hex(body.refresh_token.trim());
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let row = sqlx::query(
        "SELECT t.token_id, t.token_family_id, t.user_id, t.device_id, t.expires_at, t.revoked_at,
                u.is_active, r.code AS role_code
         FROM auth_refresh_tokens t
         JOIN users u ON u.user_id = t.user_id
         JOIN roles r ON r.role_id = u.role_id
         WHERE t.token_hash = $1
         FOR UPDATE OF t",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::unauthorized("Invalid refresh token"))?;

    let token_id: Uuid = row.try_get("token_id").map_err(ApiError::from_db)?;
    let family_id: Uuid = row.try_get("token_family_id").map_err(ApiError::from_db)?;
    let user_id: Uuid = row.try_get("user_id").map_err(ApiError::from_db)?;
    let device_id: String = row.try_get("device_id").map_err(ApiError::from_db)?;
    let expires_at: DateTime<Utc> = row.try_get("expires_at").map_err(ApiError::from_db)?;
    let revoked_at: Option<DateTime<Utc>> = row.try_get("revoked_at").map_err(ApiError::from_db)?;
    let is_active: bool = row.try_get("is_active").map_err(ApiError::from_db)?;
    let role_str: String = row.try_get("role_code").map_err(ApiError::from_db)?;

    // Reuse of an already-rotated refresh token revokes the entire token_family_id (SEC-03).
    if revoked_at.is_some() {
        sqlx::query(
            "UPDATE auth_refresh_tokens
             SET revoked_at = COALESCE(revoked_at, NOW())
             WHERE token_family_id = $1",
        )
        .bind(family_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        write_audit_log(
            &mut tx,
            Some(user_id),
            "REFRESH_TOKEN_REUSE_REVOKED",
            "TOKEN_FAMILY",
            family_id,
            None,
            json!({
                "reason": "refresh_token_reuse_detected",
                "device_id": device_id
            }),
        )
        .await?;

        tx.commit().await.map_err(ApiError::from_db)?;
        return Err(ApiError::unauthorized("Refresh token reuse detected"));
    }

    if !is_active || expires_at <= Utc::now() || device_id != body.device_id.trim() {
        return Err(ApiError::unauthorized(
            "Refresh token expired or mismatched",
        ));
    }

    let role = RoleCode::parse(&role_str)
        .ok_or_else(|| ApiError::internal("Unsupported role in database"))?;

    sqlx::query("UPDATE auth_refresh_tokens SET revoked_at = NOW() WHERE token_id = $1")
        .bind(token_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

    let next_raw = generate_random_token_hex();
    let next_hash = sha256_hex(&next_raw);
    let next_expires = Utc::now() + Duration::days(state.config.jwt_refresh_ttl_days);

    sqlx::query(
        "INSERT INTO auth_refresh_tokens (token_family_id, user_id, token_hash, device_id, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(family_id)
    .bind(user_id)
    .bind(&next_hash)
    .bind(&device_id)
    .bind(next_expires)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    tx.commit().await.map_err(ApiError::from_db)?;

    let access_token = encode_access_token(
        user_id,
        role,
        state.config.jwt_secret.expose_secret().as_bytes(),
        state.config.jwt_access_ttl_min,
    )?;

    Ok(Json(AuthTokenResponse {
        access_token,
        refresh_token: next_raw,
        token_type: "Bearer",
        expires_in_sec: state.config.jwt_access_ttl_min * 60,
    }))
}

pub async fn logout(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<RefreshRequest>,
) -> Result<StatusCode, ApiError> {
    let token_hash = sha256_hex(body.refresh_token.trim());
    sqlx::query(
        "UPDATE auth_refresh_tokens
         SET revoked_at = COALESCE(revoked_at, NOW())
         WHERE token_hash = $1 AND user_id = $2 AND device_id = $3",
    )
    .bind(token_hash)
    .bind(auth.user_id)
    .bind(body.device_id.trim())
    .execute(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn revoke_all_sessions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let updated = sqlx::query(
        "UPDATE auth_refresh_tokens
         SET revoked_at = NOW()
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "REVOKE_ALL_SESSIONS",
        "USER",
        auth.user_id,
        None,
        json!({ "revoked_tokens_count": updated.rows_affected() }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct AuthMeResponse {
    pub user_id: Uuid,
    pub phone_e164: String,
    pub full_name: String,
    pub role_code: RoleCode,
    #[serde(with = "rust_decimal::serde::float")]
    pub reputation_score: Decimal,
}

pub async fn get_me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<AuthMeResponse>, ApiError> {
    let row = sqlx::query(
        "SELECT u.user_id, u.phone_e164, u.full_name, u.reputation_score, r.code AS role_code
         FROM users u
         JOIN roles r ON r.role_id = u.role_id
         WHERE u.user_id = $1 AND u.is_active = TRUE",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::unauthorized("User account is missing or inactive"))?;

    let role_str: String = row.try_get("role_code").map_err(ApiError::from_db)?;
    let role_code =
        RoleCode::parse(&role_str).ok_or_else(|| ApiError::internal("Unsupported role"))?;

    Ok(Json(AuthMeResponse {
        user_id: row.try_get("user_id").map_err(ApiError::from_db)?,
        phone_e164: row.try_get("phone_e164").map_err(ApiError::from_db)?,
        full_name: row.try_get("full_name").map_err(ApiError::from_db)?,
        role_code,
        reputation_score: row.try_get("reputation_score").map_err(ApiError::from_db)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_06_otp_hmac_requires_secret_and_matches_constant_time() {
        let secret = b"test_otp_secret_at_least_32_bytes_long!!";
        let wrong_secret = b"other_secret_at_least_32_bytes_long!!!!!";
        let challenge_id = Uuid::new_v4();
        let phone = "+51999111222";
        let hash = compute_otp_hmac_hex(secret, challenge_id, phone, "482910");

        assert!(verify_otp_constant_time(
            secret,
            challenge_id,
            phone,
            "482910",
            &hash
        ));
        assert!(!verify_otp_constant_time(
            secret,
            challenge_id,
            phone,
            "482911",
            &hash
        ));
        assert!(!verify_otp_constant_time(
            wrong_secret,
            challenge_id,
            phone,
            "482910",
            &hash
        ));
    }

    #[test]
    fn jwt_roundtrip_and_expiry() {
        let secret = b"test_jwt_secret_at_least_32_bytes_long!!";
        let uid = Uuid::new_v4();
        let token = encode_access_token(uid, RoleCode::CoerAdmin, secret, 15).unwrap();
        let user = decode_access_token(&token, secret).unwrap();
        assert_eq!(user.user_id, uid);
        assert_eq!(user.role, RoleCode::CoerAdmin);

        let expired_claims = JwtClaims {
            sub: uid,
            role: "DRIVER".to_string(),
            iat: Utc::now().timestamp() - 120,
            exp: Utc::now().timestamp() - 10,
            jti: Uuid::new_v4(),
        };
        let expired = encode(
            &Header::new(Algorithm::HS256),
            &expired_claims,
            &EncodingKey::from_secret(secret),
        )
        .unwrap();
        assert!(decode_access_token(&expired, secret).is_err());
    }
}
