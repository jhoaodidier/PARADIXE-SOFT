#![forbid(unsafe_code)]

pub mod auth;
pub mod config;
pub mod corridors;
pub mod error;
pub mod events;
pub mod fixtures;
pub mod incidents;
pub mod ops;
pub mod routing;
pub mod simulator;
pub mod sync;
pub mod uploads;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};

use crate::{
    auth::RateLimiter,
    config::AppConfig,
    events::{ApiMetrics, OperationalEvent},
    incidents::DailyTelemetryKeyStore,
};

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
    pub event_tx: broadcast::Sender<OperationalEvent>,
    pub metrics: Arc<ApiMetrics>,
    pub http_client: reqwest::Client,
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

fn build_cors_layer(allowed_origins: &[String]) -> CorsLayer {
    if allowed_origins.is_empty() {
        CorsLayer::permissive()
    } else {
        let parsed: Vec<HeaderValue> = allowed_origins
            .iter()
            .filter_map(|origin| HeaderValue::from_str(origin).ok())
            .collect();
        if parsed.is_empty() {
            CorsLayer::permissive()
        } else {
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(parsed))
                .allow_methods([
                    http::Method::GET,
                    http::Method::POST,
                    http::Method::PATCH,
                    http::Method::DELETE,
                    http::Method::OPTIONS,
                ])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    let cors = build_cors_layer(&state.config.cors_allowed_origins);

    let api_v1 = Router::new()
        .route("/openapi.json", get(openapi_handler))
        // Auth & Admin Users
        .route("/auth/otp/request", post(auth::request_otp))
        .route("/auth/register", post(auth::register_driver))
        .route("/auth/login", post(auth::login))
        .route("/auth/refresh", post(auth::refresh_session))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/revoke-all", post(auth::revoke_all_sessions))
        .route("/auth/me", get(auth::get_me))
        .route(
            "/admin/users",
            get(auth::list_users).post(auth::create_admin_user),
        )
        // Sync & Realtime Events
        .route("/sync/snapshot", get(sync::get_sync_snapshot))
        .route("/sync/batch", post(sync::post_sync_batch))
        .route("/events/stream", get(events::events_stream_handler))
        // Corridors, Regions, Nodes, Forecasts & Analytics
        .route("/corridors/map", get(corridors::get_corridors_map))
        .route("/corridors/forecast", get(ops::get_corridor_forecast))
        .route("/analytics/summary", get(ops::get_analytics_summary))
        .route("/regions", get(corridors::get_regions))
        .route("/nodes", get(corridors::list_nodes))
        .route(
            "/segments/{segment_id}/attributes",
            patch(corridors::verify_segment_attributes),
        )
        .route(
            "/corridors/verify-attributes",
            post(corridors::verify_corridor_batch_attributes),
        )
        .route("/ivv/recalculate", post(corridors::recalculate_ivv))
        // Evidence Uploads
        .route("/uploads/presign", post(uploads::presign_upload))
        .route("/uploads/evidence", post(uploads::upload_evidence))
        // Fleet Alert Subscriptions
        .route(
            "/subscriptions",
            get(ops::list_subscriptions).post(ops::create_subscription),
        )
        .route(
            "/subscriptions/{subscription_id}",
            delete(ops::delete_subscription),
        )
        // Telemetry, Field Reports, Webhooks & Incidents
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
        // Routing, Transfer Corridors, Logistic Bridges & Humanitarian Windows
        .route("/routing/calculate", post(routing::calculate_route_handler))
        .route(
            "/operations/bridges",
            get(routing::list_bridges_and_windows),
        )
        .route(
            "/transfer-corridors",
            post(routing::create_transfer_corridor),
        )
        .route(
            "/transfer-corridors/{corridor_id}/inspect",
            patch(routing::inspect_transfer_corridor),
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
        // Isolated Simulator
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
        .route("/metrics", get(events::metrics_handler))
        .nest("/api/v1", api_v1)
        .layer(RequestBodyLimitLayer::new(3 * 1024 * 1024))
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
        .layer(cors)
        .with_state(state)
}
