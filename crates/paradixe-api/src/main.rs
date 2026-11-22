#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;

use paradixe_api::{
    auth::RateLimiter, build_router, config::AppConfig, incidents::DailyTelemetryKeyStore,
    AppState, OperationalPreviewGateway, SimulationNotificationGateway,
};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,paradixe_api=debug,sqlx=warn")),
        )
        .init();

    let config = Arc::new(AppConfig::from_env()?);

    let pool = PgPoolOptions::new()
        .max_connections(16)
        .acquire_timeout(Duration::from_secs(5))
        .connect(config.database_url.expose_secret())
        .await?;

    let sim_pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect(config.simulator_database_url.expose_secret())
        .await?;

    let telemetry_keys = DailyTelemetryKeyStore::new(config.telemetry_key_store_dir.clone())?;

    let state = AppState {
        pool,
        sim_pool,
        config: config.clone(),
        rate_limiter: RateLimiter::default(),
        telemetry_keys,
        gateway: Arc::new(OperationalPreviewGateway),
        sim_gateway: Arc::new(SimulationNotificationGateway::default()),
    };

    let addr = config.api_bind_addr.clone();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(bind_addr = %addr, "paradixe-api listening");

    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
