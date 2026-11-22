#![forbid(unsafe_code)]

use std::path::PathBuf;

use secrecy::SecretString;

#[derive(Clone)]
pub struct AppConfig {
    pub api_bind_addr: String,
    pub database_url: SecretString,
    pub simulator_database_url: SecretString,
    pub jwt_secret: SecretString,
    pub jwt_access_ttl_min: i64,
    pub jwt_refresh_ttl_days: i64,
    pub otp_secret: SecretString,
    pub webhook_secret: SecretString,
    pub telemetry_key_store_dir: PathBuf,
    pub cors_allowed_origins: Vec<String>,
    pub messaging_internal_url: Option<String>,
    pub s3_endpoint: String,
    pub s3_region: String,
    pub s3_access_key: String,
    pub s3_secret_key: SecretString,
    pub s3_bucket_evidence: String,
    pub s3_public_evidence_base: String,
}

impl AppConfig {
    /// Builds API configuration strictly from environment variables (no hardcoded prod secrets).
    pub fn from_env() -> Result<Self, String> {
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required".to_string())?;
        let simulator_database_url = std::env::var("SIMULATOR_DATABASE_URL")
            .map_err(|_| "SIMULATOR_DATABASE_URL is required".to_string())?;
        let jwt_secret =
            std::env::var("JWT_SECRET").map_err(|_| "JWT_SECRET is required".to_string())?;
        if jwt_secret.len() < 32 {
            return Err("JWT_SECRET must be at least 32 bytes".to_string());
        }
        let otp_secret =
            std::env::var("OTP_SECRET").map_err(|_| "OTP_SECRET is required".to_string())?;
        if otp_secret.len() < 32 {
            return Err("OTP_SECRET must be at least 32 bytes".to_string());
        }
        let webhook_secret = std::env::var("WHATSAPP_WEBHOOK_SECRET")
            .map_err(|_| "WHATSAPP_WEBHOOK_SECRET is required".to_string())?;
        if webhook_secret.len() < 16 {
            return Err("WHATSAPP_WEBHOOK_SECRET must be at least 16 bytes".to_string());
        }

        let api_bind_addr =
            std::env::var("API_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let jwt_access_ttl_min = std::env::var("JWT_ACCESS_TTL_MIN")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(15);
        let jwt_refresh_ttl_days = std::env::var("JWT_REFRESH_TTL_DAYS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(30);
        let telemetry_key_store_dir = PathBuf::from(
            std::env::var("TELEMETRY_KEY_STORE_DIR")
                .unwrap_or_else(|_| "./telemetry_keys".to_string()),
        );

        let cors_allowed_origins = std::env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let messaging_internal_url = std::env::var("MESSAGING_INTERNAL_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());

        let s3_endpoint = std::env::var("S3_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string())
            .trim()
            .trim_end_matches('/')
            .to_string();
        let s3_region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let s3_access_key =
            std::env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "rustfsadmin".to_string());
        let s3_secret_key = SecretString::from(
            std::env::var("S3_SECRET_KEY").unwrap_or_else(|_| "rustfsadmin_dev_only".to_string()),
        );
        let s3_bucket_evidence = std::env::var("S3_BUCKET_EVIDENCE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "paradixe-evidence".to_string());
        let s3_public_evidence_base = std::env::var("S3_PUBLIC_EVIDENCE_BASE")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/evidence".to_string());

        Ok(Self {
            api_bind_addr,
            database_url: SecretString::from(database_url),
            simulator_database_url: SecretString::from(simulator_database_url),
            jwt_secret: SecretString::from(jwt_secret),
            jwt_access_ttl_min,
            jwt_refresh_ttl_days,
            otp_secret: SecretString::from(otp_secret),
            webhook_secret: SecretString::from(webhook_secret),
            telemetry_key_store_dir,
            cors_allowed_origins,
            messaging_internal_url,
            s3_endpoint,
            s3_region,
            s3_access_key,
            s3_secret_key,
            s3_bucket_evidence,
            s3_public_evidence_base,
        })
    }
}
