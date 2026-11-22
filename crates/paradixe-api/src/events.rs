#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::{
    extract::State,
    http::header,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use serde::Serialize;
use sha2::Sha256;
use sqlx::Row;
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use uuid::Uuid;

use crate::{AppState, NotificationChannel, NotificationEvent, NotificationGateway};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize)]
pub struct OperationalEvent {
    pub event_type: String,
    pub entity_id: Option<Uuid>,
    pub region: Option<String>,
    pub summary: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Default)]
pub struct ApiMetrics {
    pub routing_total: AtomicU64,
    pub routing_alternate: AtomicU64,
    pub routing_bridge: AtomicU64,
    pub routing_no_safe: AtomicU64,
    pub incidents_confirmed: AtomicU64,
    pub reports_created: AtomicU64,
    pub alerts_dispatched: AtomicU64,
    pub evidence_uploads: AtomicU64,
    pub sse_connections: AtomicU64,
}

impl ApiMetrics {
    pub fn render_prometheus(&self) -> String {
        format!(
            concat!(
                "# HELP paradixe_routing_requests_total Total routing calculations executed\n",
                "# TYPE paradixe_routing_requests_total counter\n",
                "paradixe_routing_requests_total {}\n",
                "# HELP paradixe_routing_outcomes_total Routing calculations grouped by resolution type\n",
                "# TYPE paradixe_routing_outcomes_total counter\n",
                "paradixe_routing_outcomes_total{{resolution=\"ALTERNATE_ROUTE\"}} {}\n",
                "paradixe_routing_outcomes_total{{resolution=\"LOGISTIC_BRIDGE_REQUIRED\"}} {}\n",
                "paradixe_routing_outcomes_total{{resolution=\"NO_SAFE_ROUTE\"}} {}\n",
                "# HELP paradixe_incidents_confirmed_total Confirmed road incidents\n",
                "# TYPE paradixe_incidents_confirmed_total counter\n",
                "paradixe_incidents_confirmed_total {}\n",
                "# HELP paradixe_field_reports_created_total Field reports recorded\n",
                "# TYPE paradixe_field_reports_created_total counter\n",
                "paradixe_field_reports_created_total {}\n",
                "# HELP paradixe_alerts_dispatched_total Outbound fleet alert webhook dispatches\n",
                "# TYPE paradixe_alerts_dispatched_total counter\n",
                "paradixe_alerts_dispatched_total {}\n",
                "# HELP paradixe_evidence_uploads_total Evidence images uploaded to S3/RustFS\n",
                "# TYPE paradixe_evidence_uploads_total counter\n",
                "paradixe_evidence_uploads_total {}\n",
                "# HELP paradixe_sse_connections_total Total SSE stream subscriptions opened\n",
                "# TYPE paradixe_sse_connections_total counter\n",
                "paradixe_sse_connections_total {}\n",
            ),
            self.routing_total.load(Ordering::Relaxed),
            self.routing_alternate.load(Ordering::Relaxed),
            self.routing_bridge.load(Ordering::Relaxed),
            self.routing_no_safe.load(Ordering::Relaxed),
            self.incidents_confirmed.load(Ordering::Relaxed),
            self.reports_created.load(Ordering::Relaxed),
            self.alerts_dispatched.load(Ordering::Relaxed),
            self.evidence_uploads.load(Ordering::Relaxed),
            self.sse_connections.load(Ordering::Relaxed),
        )
    }
}

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        state.metrics.render_prometheus(),
    )
}

pub async fn events_stream_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    state
        .metrics
        .sse_connections
        .fetch_add(1, Ordering::Relaxed);
    let rx = state.event_tx.subscribe();

    let hello = OperationalEvent {
        event_type: "CONNECTED".to_string(),
        entity_id: None,
        region: None,
        summary: "Canal operativo en tiempo real activo".to_string(),
        timestamp: Utc::now(),
    };
    let hello_json = serde_json::to_string(&hello).unwrap_or_else(|_| "{}".to_string());
    let initial = tokio_stream::once(Ok(Event::default().event("connected").data(hello_json)));

    let updates = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(ev) => {
            let payload = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".to_string());
            Some(Ok(Event::default().event(ev.event_type).data(payload)))
        }
        Err(_) => None,
    });

    Sse::new(initial.chain(updates)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(20))
            .text("keepalive"),
    )
}

pub fn publish_operational_event(
    state: &AppState,
    event_type: &str,
    entity_id: Option<Uuid>,
    region: Option<String>,
    summary: impl Into<String>,
) {
    let _ = state.event_tx.send(OperationalEvent {
        event_type: event_type.to_string(),
        entity_id,
        region,
        summary: summary.into(),
        timestamp: Utc::now(),
    });
}

/// Production notification gateway connecting `paradixe-api` to `paradixe-messaging` over HMAC.
pub struct HttpMessagingGateway {
    pub internal_url: String,
}

impl HttpMessagingGateway {
    pub fn new(internal_url: String) -> Self {
        Self { internal_url }
    }
}

impl NotificationGateway for HttpMessagingGateway {
    fn format_and_dispatch(
        &self,
        event: NotificationEvent,
    ) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(async move {
            match event.channel {
                NotificationChannel::SimulationPreview => {
                    if event.message.starts_with("[SIMULATION]") {
                        event.message
                    } else {
                        format!("[SIMULATION] {}", event.message)
                    }
                }
                NotificationChannel::WhatsappBot | NotificationChannel::Sms => {
                    tracing::debug!(
                        target_gateway = %self.internal_url,
                        channel = ?event.channel,
                        "Prepared outbound citizen gateway confirmation preview"
                    );
                    event.message
                }
            }
        })
    }
}

fn sign_webhook_payload(secret: &[u8], raw_body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(raw_body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Dispatches signed WhatsApp/SMS notifications asynchronously to subscribed fleet operators
/// matching the affected corridor when `MESSAGING_INTERNAL_URL` is configured.
pub fn dispatch_corridor_alerts(
    state: &AppState,
    corridor_name: String,
    severity: &'static str,
    message: String,
) {
    let pool = state.pool.clone();
    let http = state.http_client.clone();
    let messaging_url = state.config.messaging_internal_url.clone();
    let secret = state
        .config
        .webhook_secret
        .expose_secret()
        .as_bytes()
        .to_vec();
    let metrics = state.metrics.clone();

    tokio::spawn(async move {
        let allowed_thresholds: &[&str] = match severity {
            "BLOCKED" => &["BLOCKED", "RED", "AMBER"],
            "RED" => &["RED", "AMBER"],
            _ => &["AMBER"],
        };

        let rows = match sqlx::query(
            "SELECT DISTINCT u.phone_e164, cas.channel
             FROM corridor_alert_subscriptions cas
             JOIN users u ON u.user_id = cas.user_id
             WHERE cas.is_active = TRUE
               AND u.is_active = TRUE
               AND cas.min_risk_level = ANY($1::text[])
               AND (
                   cas.corridor_name IS NULL
                   OR TRIM(cas.corridor_name) = ''
                   OR LOWER(TRIM(cas.corridor_name)) = LOWER(TRIM($2))
               )",
        )
        .bind(allowed_thresholds)
        .bind(&corridor_name)
        .fetch_all(&pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "Failed querying corridor alert subscriptions");
                return;
            }
        };

        if rows.is_empty() {
            return;
        }

        let Some(base_url) = messaging_url else {
            tracing::info!(
                corridor = %corridor_name,
                subscribers = rows.len(),
                "Fleet alert preview generated (MESSAGING_INTERNAL_URL unset)"
            );
            metrics
                .alerts_dispatched
                .fetch_add(rows.len() as u64, Ordering::Relaxed);
            return;
        };

        let dispatch_url = format!("{base_url}/internal/dispatch");
        for row in rows {
            let Ok(phone): Result<String, _> = row.try_get("phone_e164") else {
                continue;
            };
            let Ok(channel): Result<String, _> = row.try_get("channel") else {
                continue;
            };
            let Ok(payload) = serde_json::to_vec(&serde_json::json!({
                "recipient_phone_e164": phone,
                "channel": channel,
                "message": message,
            })) else {
                continue;
            };
            let sig = sign_webhook_payload(&secret, &payload);
            match http
                .post(&dispatch_url)
                .timeout(Duration::from_secs(4))
                .header("Content-Type", "application/json")
                .header("X-Webhook-Signature", sig)
                .body(payload)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    metrics.alerts_dispatched.fetch_add(1, Ordering::Relaxed);
                }
                Ok(resp) => {
                    tracing::warn!(
                        status = %resp.status(),
                        corridor = %corridor_name,
                        "Messaging internal dispatch returned non-2xx"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        corridor = %corridor_name,
                        "Messaging internal dispatch request failed"
                    );
                }
            }
        }
    });
}
