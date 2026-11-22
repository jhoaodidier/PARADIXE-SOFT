#![forbid(unsafe_code)]

pub mod auth;
pub mod config;
pub mod corridors;
pub mod error;
pub mod fixtures;
pub mod incidents;
pub mod routing;
pub mod simulator;
pub mod sync;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use serde_json::json;
use sqlx::PgPool;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::{auth::RateLimiter, config::AppConfig, incidents::DailyTelemetryKeyStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationChannel {
    WhatsappBot,
    Sms,
    SimulationPreview,
}

#[derive(Debug, Clone)]
pub struct NotificationEvent {
    pub channel: NotificationChannel,
    pub message: String,
}

pub trait NotificationGateway: Send + Sync {
    fn format_and_dispatch(
        &self,
        event: NotificationEvent,
    ) -> Pin<Box<dyn Future<Output = String> + Send + '_>>;
}

/// Non-production HTTP-less fallback gateway used while WhatsApp/SMS transport (`paradixe-messaging`)
/// is deployed as an external channel worker; returns sanitized preview confirmation.
#[derive(Default)]
pub struct OperationalPreviewGateway;

impl NotificationGateway for OperationalPreviewGateway {
    fn format_and_dispatch(
        &self,
        event: NotificationEvent,
    ) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(async move { event.message })
    }
}

/// Strictly isolated simulator gateway: never issues network requests and records previews only
/// in memory with a `[SIMULATION]` boundary guarantee (`SIM-05`).
#[derive(Default)]
pub struct SimulationNotificationGateway {
    dispatched: Mutex<Vec<String>>,
}

impl SimulationNotificationGateway {
    pub fn captured_messages(&self) -> Vec<String> {
        self.dispatched
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}

impl NotificationGateway for SimulationNotificationGateway {
    fn format_and_dispatch(
        &self,
        event: NotificationEvent,
    ) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(async move {
            let formatted = if event.message.starts_with("[SIMULATION]") {
                event.message
            } else {
                format!("[SIMULATION] {}", event.message)
            };
            if let Ok(mut guard) = self.dispatched.lock() {
                guard.push(formatted.clone());
            }
            formatted
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub sim_pool: PgPool,
    pub config: Arc<AppConfig>,
    pub rate_limiter: RateLimiter,
    pub telemetry_keys: DailyTelemetryKeyStore,
    pub gateway: Arc<dyn NotificationGateway>,
    pub sim_gateway: Arc<SimulationNotificationGateway>,
}

const OPENAPI_SPEC_JSON: &str = include_str!("../../../openapi/openapi-v1.json");

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();
    let sim_ok = sqlx::query("SELECT 1")
        .execute(&state.sim_pool)
        .await
        .is_ok();
    let status = if db_ok && sim_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "service": "paradixe-api",
            "status": if db_ok && sim_ok { "healthy" } else { "degraded" },
            "postgis_pool": db_ok,
            "simulator_ro_pool": sim_ok
        })),
    )
}

async fn openapi_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        OPENAPI_SPEC_JSON,
    )
}

pub fn build_router(state: AppState) -> Router {
    let api_v1 = Router::new()
        .route("/openapi.json", get(openapi_handler))
        // Auth (6)
        .route("/auth/otp/request", post(auth::request_otp))
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh_session))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/revoke-all", post(auth::revoke_all_sessions))
        .route("/auth/me", get(auth::get_me))
        // Sync (2)
        .route("/sync/snapshot", get(sync::get_sync_snapshot))
        .route("/sync/batch", post(sync::post_sync_batch))
        // Corridors, Regions & IVV
        .route("/corridors/map", get(corridors::get_corridors_map))
        .route("/regions", get(corridors::get_regions))
        .route("/ivv/recalculate", post(corridors::recalculate_ivv))
        // Telemetry, Field Reports, Webhooks & Incidents (5)
        .route("/telemetry/pings", post(incidents::ingest_telemetry_pings))
        .route("/reports", post(incidents::create_field_report))
        .route(
            "/webhooks/messaging",
            post(incidents::receive_messaging_webhook),
        )
        .route("/incidents", get(incidents::list_incidents))
        .route(
            "/incidents/{incident_id}/verification",
            patch(incidents::verify_incident),
        )
        // Routing, Logistic Bridges & Humanitarian Windows (5)
        .route("/routing/calculate", post(routing::calculate_route_handler))
        .route(
            "/operations/bridges",
            get(routing::list_bridges_and_windows),
        )
        .route("/bridges", post(routing::propose_logistic_bridge))
        .route(
            "/bridges/{bridge_id}/status",
            patch(routing::update_bridge_status),
        )
        .route(
            "/incidents/{incident_id}/humanitarian-windows",
            post(routing::create_humanitarian_window),
        )
        // Isolated Simulator (5)
        .route(
            "/simulator/scenarios",
            post(simulator::create_simulation_scenario),
        )
        .route(
            "/simulator/scenarios/{scenario_id}/events",
            post(simulator::inject_simulation_event),
        )
        .route(
            "/simulator/scenarios/{scenario_id}/routing",
            post(simulator::run_simulation_routing),
        )
        .route(
            "/simulator/scenarios/{scenario_id}/evaluate",
            post(simulator::evaluate_simulation_impact),
        )
        .route(
            "/simulator/scenarios/{scenario_id}/impact",
            get(simulator::get_simulation_impact),
        );

    Router::new()
        .route("/health", get(health_handler))
        .nest("/api/v1", api_v1)
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024))
        // Privacy guard (SEC-07): log method and path only — never headers, JWTs, query params, or bodies.
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &http::Request<_>| {
                tracing::info_span!(
                    "http_request",
                    method = %req.method(),
                    path = %req.uri().path()
                )
            }),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
