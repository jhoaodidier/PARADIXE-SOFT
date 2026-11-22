#![forbid(unsafe_code)]

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use paradixe_domain::ProposedPassability;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};

use crate::{
    bridge::CoreApiBridge,
    parser::{parse_citizen_message, truncate_sms_160, CitizenIntent},
    providers::{DispatchResult, MessagingConfig, OutboundDispatcher},
    signature::{mask_phone_e164, verify_sha256_signature, verify_token_constant_time},
};

#[derive(Clone)]
pub struct MessagingState {
    pub config: MessagingConfig,
    pub dispatcher: OutboundDispatcher,
    pub bridge: CoreApiBridge,
}

impl MessagingState {
    pub fn from_config(config: MessagingConfig) -> Self {
        Self {
            dispatcher: OutboundDispatcher::new(config.clone()),
            bridge: CoreApiBridge::new(config.clone()),
            config,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WhatsappVerifyQuery {
    #[serde(rename = "hub.mode")]
    pub mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub challenge: Option<String>,
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "paradixe-messaging",
        "status": "healthy"
    }))
}

async fn whatsapp_verify_handler(
    State(state): State<MessagingState>,
    Query(query): Query<WhatsappVerifyQuery>,
) -> impl IntoResponse {
    let mode_ok = query.mode.as_deref() == Some("subscribe");
    let token_ok = query.verify_token.as_deref().is_some_and(|provided| {
        verify_token_constant_time(state.config.whatsapp_verify_token.expose_secret(), provided)
    });

    if mode_ok && token_ok {
        let challenge = query.challenge.unwrap_or_default();
        (StatusCode::OK, challenge)
    } else {
        (StatusCode::FORBIDDEN, "Verification failed".to_string())
    }
}

#[derive(Debug, Deserialize)]
struct MetaWebhookPayload {
    #[serde(default)]
    entry: Vec<MetaEntry>,
}

#[derive(Debug, Deserialize)]
struct MetaEntry {
    #[serde(default)]
    changes: Vec<MetaChange>,
}

#[derive(Debug, Deserialize)]
struct MetaChange {
    value: Option<MetaValue>,
}

#[derive(Debug, Deserialize)]
struct MetaValue {
    #[serde(default)]
    messages: Vec<MetaMessage>,
}

#[derive(Debug, Deserialize)]
struct MetaMessage {
    from: Option<String>,
    text: Option<MetaTextBody>,
    location: Option<MetaLocation>,
}

#[derive(Debug, Deserialize)]
struct MetaTextBody {
    body: String,
}

#[derive(Debug, Deserialize)]
struct MetaLocation {
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, Serialize)]
pub struct WhatsappWebhookSummary {
    pub processed: usize,
    pub dispatches: Vec<DispatchResult>,
}

fn help_message() -> &'static str {
    "PARADIXE Huánuco: Envia ESTADO <via> (ej. ESTADO Carpish) o reporta DERRUMBE / 1 CARRIL / LIBRE <via>."
}

async fn handle_citizen_turn(
    state: &MessagingState,
    channel: &str,
    phone_e164: &str,
    text: &str,
    coords: Option<(f64, f64)>,
) -> String {
    match parse_citizen_message(text) {
        CitizenIntent::Help => help_message().to_string(),
        CitizenIntent::QueryCorridor { corridor_query } => {
            match state
                .bridge
                .lookup_corridor_status(&corridor_query, &state.config.default_region)
                .await
            {
                Ok(msg) => msg,
                Err(_) => "PARADIXE: Servicio de consulta temporalmente no disponible.".to_string(),
            }
        }
        CitizenIntent::ReportRoad {
            passability,
            corridor_query,
        } => {
            match state
                .bridge
                .resolve_corridor_segment(&corridor_query, &state.config.default_region)
                .await
            {
                Ok(Some((segment_id, corridor_name, mid_lat, mid_lon))) => {
                    let (lat, lon) = coords.unwrap_or((mid_lat, mid_lon));
                    match state
                        .bridge
                        .forward_signed_report(
                            channel,
                            phone_e164,
                            segment_id,
                            passability,
                            lat,
                            lon,
                        )
                        .await
                    {
                        Ok(res) => res.reply_preview,
                        Err(_) => {
                            let label = match passability {
                                ProposedPassability::Blocked => "bloqueo",
                                ProposedPassability::PartialOneLane => "paso restringido",
                                ProposedPassability::Open => "via despejada",
                            };
                            format!(
                                "PARADIXE: Reporte de {label} recibido para {corridor_name}. Solo cuentas registradas validan consenso."
                            )
                        }
                    }
                }
                Ok(None) => format!(
                    "PARADIXE: No identificamos el corredor '{corridor_query}'. Envia el nombre exacto de la via."
                ),
                Err(_) => "PARADIXE: Error conectando con servidor de reportes.".to_string(),
            }
        }
    }
}

async fn whatsapp_webhook_handler(
    State(state): State<MessagingState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<WhatsappWebhookSummary>, (StatusCode, &'static str)> {
    let sig = headers
        .get("X-Hub-Signature-256")
        .or_else(|| headers.get("X-Webhook-Signature"))
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing webhook HMAC signature"))?;

    if !verify_sha256_signature(
        state.config.whatsapp_app_secret.expose_secret().as_bytes(),
        &raw_body,
        sig,
    ) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid HMAC signature"));
    }

    let payload: MetaWebhookPayload = serde_json::from_slice(&raw_body)
        .map_err(|_| (StatusCode::UNPROCESSABLE_ENTITY, "Malformed WhatsApp JSON"))?;

    let mut processed = 0usize;
    let mut dispatches = Vec::new();

    for entry in payload.entry {
        for change in entry.changes {
            let Some(val) = change.value else { continue };
            for msg in val.messages {
                let Some(raw_from) = msg.from else { continue };
                let phone_e164 = if raw_from.starts_with('+') {
                    raw_from
                } else {
                    format!("+{raw_from}")
                };
                let body = msg
                    .text
                    .map(|t| t.body)
                    .unwrap_or_else(|| "ayuda".to_string());
                let coords = msg.location.map(|l| (l.latitude, l.longitude));

                let reply =
                    handle_citizen_turn(&state, "WHATSAPP_BOT", &phone_e164, &body, coords).await;
                let res = state
                    .dispatcher
                    .send_whatsapp_with_sms_fallback(&phone_e164, &reply)
                    .await;
                processed += 1;
                dispatches.push(res);
            }
        }
    }

    Ok(Json(WhatsappWebhookSummary {
        processed,
        dispatches,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundSmsPayload {
    pub sender_phone_e164: String,
    pub body: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct InboundSmsResponse {
    pub accepted: bool,
    pub reply_sms: String,
    pub dispatch: DispatchResult,
}

async fn sms_webhook_handler(
    State(state): State<MessagingState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<InboundSmsResponse>, (StatusCode, &'static str)> {
    let sig = headers
        .get("X-Webhook-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing X-Webhook-Signature"))?;

    if !verify_sha256_signature(
        state.config.webhook_secret.expose_secret().as_bytes(),
        &raw_body,
        sig,
    ) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid X-Webhook-Signature"));
    }

    let payload: InboundSmsPayload = serde_json::from_slice(&raw_body)
        .map_err(|_| (StatusCode::UNPROCESSABLE_ENTITY, "Malformed SMS JSON"))?;

    let coords = payload.latitude.zip(payload.longitude);
    let reply = handle_citizen_turn(
        &state,
        "SMS",
        payload.sender_phone_e164.trim(),
        &payload.body,
        coords,
    )
    .await;
    let sms_reply = truncate_sms_160(&reply);
    let dispatch = state
        .dispatcher
        .send_sms(payload.sender_phone_e164.trim(), &sms_reply)
        .await;

    tracing::info!(sender = %mask_phone_e164(&payload.sender_phone_e164), "Inbound SMS handled");

    Ok(Json(InboundSmsResponse {
        accepted: true,
        reply_sms: sms_reply,
        dispatch,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InternalDispatchRequest {
    pub recipient_phone_e164: String,
    pub channel: String,
    pub message: String,
}

async fn internal_dispatch_handler(
    State(state): State<MessagingState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<DispatchResult>, (StatusCode, &'static str)> {
    let sig = headers
        .get("X-Webhook-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing X-Webhook-Signature"))?;

    if !verify_sha256_signature(
        state.config.webhook_secret.expose_secret().as_bytes(),
        &raw_body,
        sig,
    ) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid X-Webhook-Signature"));
    }

    let body: InternalDispatchRequest = serde_json::from_slice(&raw_body)
        .map_err(|_| (StatusCode::UNPROCESSABLE_ENTITY, "Malformed dispatch JSON"))?;

    let result = if body.channel.eq_ignore_ascii_case("SMS") {
        state
            .dispatcher
            .send_sms(&body.recipient_phone_e164, &body.message)
            .await
    } else {
        state
            .dispatcher
            .send_whatsapp_with_sms_fallback(&body.recipient_phone_e164, &body.message)
            .await
    };

    Ok(Json(result))
}

pub fn build_router(state: MessagingState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route(
            "/webhooks/whatsapp",
            get(whatsapp_verify_handler).post(whatsapp_webhook_handler),
        )
        .route("/webhooks/sms", post(sms_webhook_handler))
        .route("/internal/dispatch", post(internal_dispatch_handler))
        .layer(RequestBodyLimitLayer::new(512 * 1024))
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &http::Request<_>| {
                tracing::info_span!(
                    "messaging_request",
                    method = %req.method(),
                    path = %req.uri().path()
                )
            }),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http::Request;
    use secrecy::SecretString;
    use tower::ServiceExt;

    use crate::signature::compute_sha256_signature;

    fn test_config() -> MessagingConfig {
        MessagingConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            api_base_url: "http://127.0.0.1:1".to_string(),
            default_region: "PE-HUC".to_string(),
            webhook_secret: SecretString::from("test_webhook_secret_32_bytes_long!".to_string()),
            whatsapp_app_secret: SecretString::from(
                "test_webhook_secret_32_bytes_long!".to_string(),
            ),
            whatsapp_verify_token: SecretString::from("verify_token_12345".to_string()),
            whatsapp_api_url: "https://graph.facebook.com/v21.0".to_string(),
            whatsapp_phone_number_id: String::new(),
            whatsapp_access_token: None,
            sms_provider_url: None,
            sms_provider_api_key: None,
        }
    }

    #[tokio::test]
    async fn whatsapp_verify_handshake_checks_token_constant_time() {
        let app = build_router(MessagingState::from_config(test_config()));

        let ok_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=verify_token_12345&hub.challenge=987654")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok_res.status(), StatusCode::OK);

        let bad_res = app
            .oneshot(
                Request::builder()
                    .uri("/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=wrong&hub.challenge=987654")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn sms_webhook_rejects_unsigned_and_accepts_signed_help() {
        let cfg = test_config();
        let secret = cfg.webhook_secret.expose_secret().as_bytes().to_vec();
        let app = build_router(MessagingState::from_config(cfg));

        let raw_body = serde_json::to_vec(&serde_json::json!({
            "sender_phone_e164": "+51962000100",
            "body": "ayuda"
        }))
        .unwrap();

        let unsigned_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhooks/sms")
                    .header("Content-Type", "application/json")
                    .body(Body::from(raw_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unsigned_res.status(), StatusCode::UNAUTHORIZED);

        let sig = compute_sha256_signature(&secret, &raw_body);
        let signed_res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhooks/sms")
                    .header("Content-Type", "application/json")
                    .header("X-Webhook-Signature", sig)
                    .body(Body::from(raw_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(signed_res.status(), StatusCode::OK);
    }
}
