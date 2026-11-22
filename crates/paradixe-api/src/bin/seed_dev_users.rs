#![forbid(unsafe_code)]

use std::time::Duration;

use paradixe_api::auth::hash_password;
use rust_decimal::Decimal;
use sqlx::{postgres::PgPoolOptions, Row};
use uuid::Uuid;

struct DevUserSeed {
    phone_e164: &'static str,
    full_name: &'static str,
    role_code: &'static str,
    reputation_cents: i64,
}

const DEV_SEEDS: &[DevUserSeed] = &[
    DevUserSeed {
        phone_e164: "+51962000100",
        full_name: "COER Huánuco (Admin)",
        role_code: "COER_ADMIN",
        reputation_cents: 100,
    },
    DevUserSeed {
        phone_e164: "+51962000200",
        full_name: "Defensa Civil Huánuco",
        role_code: "CIVIL_DEFENSE",
        reputation_cents: 90,
    },
    DevUserSeed {
        phone_e164: "+51962000300",
        full_name: "Red de Salud / SAMU Huánuco",
        role_code: "HEALTH_OPERATOR",
        reputation_cents: 85,
    },
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL env var is required")?;
    let dev_password =
        std::env::var("DEV_OPERATOR_PASSWORD").unwrap_or_else(|_| "ParadixeDev2026!".to_string());
    if dev_password.len() < 8 {
        return Err("DEV_OPERATOR_PASSWORD must be at least 8 characters".into());
    }

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;

    let password_hash = hash_password(&dev_password).map_err(|e| format!("{e:?}"))?;

    for seed in DEV_SEEDS {
        let role_row = sqlx::query("SELECT role_id FROM roles WHERE code = $1")
            .bind(seed.role_code)
            .fetch_one(&pool)
            .await?;
        let role_id: Uuid = role_row.try_get("role_id")?;

        sqlx::query(
            "INSERT INTO users (role_id, phone_e164, full_name, password_hash, reputation_score, is_active)
             VALUES ($1, $2, $3, $4, $5, TRUE)
             ON CONFLICT (phone_e164) DO UPDATE
             SET role_id = EXCLUDED.role_id,
                 full_name = EXCLUDED.full_name,
                 password_hash = EXCLUDED.password_hash,
                 reputation_score = EXCLUDED.reputation_score,
                 is_active = TRUE",
        )
        .bind(role_id)
        .bind(seed.phone_e164)
        .bind(seed.full_name)
        .bind(&password_hash)
        .bind(Decimal::new(seed.reputation_cents, 2))
        .execute(&pool)
        .await?;

        println!(
            "[DEV SEED ONLY] Upserted {} | {} | role={}",
            seed.phone_e164, seed.full_name, seed.role_code
        );
    }

    Ok(())
}
