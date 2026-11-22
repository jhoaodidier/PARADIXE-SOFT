#![forbid(unsafe_code)]

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use paradixe_api::{
    auth::RateLimiter,
    build_router,
    config::AppConfig,
    events::{ApiMetrics, HttpMessagingGateway, OperationalEvent},
    incidents::DailyTelemetryKeyStore,
    AppState, NotificationGateway, OperationalPreviewGateway, SimulationNotificationGateway,
};
use secrecy::ExposeSecret;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

fn spawn_telemetry_retention_worker(pool: sqlx::PgPool, key_dir: std::path::PathBuf) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Err(e) = sqlx::query(
                "DELETE FROM telemetry_pings WHERE synced_at < NOW() - INTERVAL '24 hours'",
            )
            .execute(&pool)
            .await
            {
                tracing::warn!(error = %e, "Failed 24h telemetry_pings retention cleanup");
            }

            let cutoff_date = (Utc::now() - chrono::Duration::days(2)).date_naive();
            if let Ok(entries) = fs::read_dir(&key_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let s = name.to_string_lossy();
                    if let Some(date_part) =
                        s.strip_prefix("key_").and_then(|v| v.strip_suffix(".bin"))
                    {
                        if let Ok(file_date) =
                            chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
                        {
                            if file_date < cutoff_date {
                                let _ = fs::remove_file(entry.path());
                            }
                        }
                    }
                }
            }
        }
    });
}

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
    let gateway: Arc<dyn NotificationGateway> = match &config.messaging_internal_url {
        Some(url) => Arc::new(HttpMessagingGateway::new(url.clone())),
        None => Arc::new(OperationalPreviewGateway),
    };
    let (event_tx, _) = broadcast::channel::<OperationalEvent>(256);
    let metrics = Arc::new(ApiMetrics::default());
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    spawn_telemetry_retention_worker(pool.clone(), config.telemetry_key_store_dir.clone());

    let state = AppState {
        pool,
        sim_pool,
        config: config.clone(),
        rate_limiter: RateLimiter::default(),
        telemetry_keys,
        gateway,
        sim_gateway: Arc::new(SimulationNotificationGateway::default()),
        event_tx,
        metrics,
        http_client,
    };

    let addr = config.api_bind_addr.clone();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(bind_addr = %addr, "paradixe-api listening");

    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
