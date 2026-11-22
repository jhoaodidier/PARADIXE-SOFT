#![forbid(unsafe_code)]

use chrono::Utc;
use paradixe_domain::ProposedPassability;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::providers::MessagingConfig;
use crate::signature::compute_sha256_signature;

#[derive(Debug, Clone, Deserialize)]
struct MapFeatureCollection {
    features: Vec<MapFeature>,
}

#[derive(Debug, Clone, Deserialize)]
struct MapFeature {
    geometry: MapLineString,
    properties: MapProperties,
}

#[derive(Debug, Clone, Deserialize)]
struct MapLineString {
    coordinates: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
struct MapProperties {
    segment_id: Uuid,
    corridor_name: String,
    passability_state: String,
    ivv_score: Option<f64>,
    risk_level: String,
    is_stale: bool,
    is_attributes_verified: bool,
}

#[derive(Debug, Serialize)]
struct ApiMessagingWebhookRequest<'a> {
    channel: &'a str,
    sender_phone_e164: &'a str,
    segment_id: Uuid,
    proposed_passability: ProposedPassability,
    latitude: f64,
    longitude: f64,
    timestamp_utc: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ApiMessagingWebhookResponse {
    pub accepted: bool,
    pub reply_preview: String,
}

#[derive(Clone)]
pub struct CoreApiBridge {
    http: reqwest::Client,
    config: MessagingConfig,
}

impl CoreApiBridge {
    pub fn new(config: MessagingConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    pub async fn resolve_corridor_segment(
        &self,
        query: &str,
        region: &str,
    ) -> Result<Option<(Uuid, String, f64, f64)>, String> {
        let collection = self.fetch_corridors(region).await?;
        let best = pick_best_corridor(&collection.features, query);
        let Some(f) = best else {
            return Ok(None);
        };
        let (lon, lat) = midpoint(&f.geometry.coordinates);
        Ok(Some((
            f.properties.segment_id,
            f.properties.corridor_name.clone(),
            lat,
            lon,
        )))
    }

    pub async fn lookup_corridor_status(
        &self,
        query: &str,
        region: &str,
    ) -> Result<String, String> {
        let collection = self.fetch_corridors(region).await?;
        let Some(f) = pick_best_corridor(&collection.features, query) else {
            return Ok(format!(
                "PARADIXE: No hallamos tramos para '{query}' en {region}. Envia: ESTADO Carpish."
            ));
        };

        let p = &f.properties;
        let risk = if p.is_stale {
            "SIN CLIMA VIGENTE (no autoriza despacho)".to_string()
        } else if let Some(score) = p.ivv_score {
            format!("{} (IVV {:.0}/100)", p.risk_level, score)
        } else {
            p.risk_level.clone()
        };
        let state_txt = match p.passability_state.as_str() {
            "BLOCKED" => "BLOQUEADO",
            "PARTIAL_ONE_LANE" => "PASO 1 CARRIL",
            "CAUTION_SUSPECTED" => "PRECAUCION",
            _ => "TRANSITABLE",
        };
        let verif_txt = if p.is_attributes_verified {
            "Verificado"
        } else {
            "Sin verificacion fisica"
        };

        Ok(format!(
            "PARADIXE: {} -> {}. Riesgo: {}. {}. Confirmar siempre con COER.",
            p.corridor_name, state_txt, risk, verif_txt
        ))
    }

    pub async fn forward_signed_report(
        &self,
        channel: &str,
        sender_phone_e164: &str,
        segment_id: Uuid,
        proposed_passability: ProposedPassability,
        latitude: f64,
        longitude: f64,
    ) -> Result<ApiMessagingWebhookResponse, String> {
        let payload = ApiMessagingWebhookRequest {
            channel,
            sender_phone_e164,
            segment_id,
            proposed_passability,
            latitude,
            longitude,
            timestamp_utc: Utc::now(),
        };
        let raw_bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
        let signature = compute_sha256_signature(
            self.config.webhook_secret.expose_secret().as_bytes(),
            &raw_bytes,
        );

        let url = format!("{}/webhooks/messaging", self.config.api_base_url);
        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Signature", signature)
            .body(raw_bytes)
            .send()
            .await
            .map_err(|e| format!("Failed calling paradixe-api webhook: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("paradixe-api returned {status}: {text}"));
        }

        resp.json::<ApiMessagingWebhookResponse>()
            .await
            .map_err(|e| format!("Invalid JSON from paradixe-api: {e}"))
    }

    async fn fetch_corridors(&self, region: &str) -> Result<MapFeatureCollection, String> {
        let url = format!(
            "{}/corridors/map?region={}&limit=2500",
            self.config.api_base_url, region
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed querying corridors API: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Corridors API returned HTTP {}", resp.status()));
        }
        resp.json::<MapFeatureCollection>()
            .await
            .map_err(|e| format!("Invalid corridors GeoJSON: {e}"))
    }
}

fn pick_best_corridor<'a>(features: &'a [MapFeature], query: &str) -> Option<&'a MapFeature> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return features.first();
    }
    let q_words: Vec<&str> = q
        .split(|c: char| !c.is_alphanumeric() && c != '-')
        .filter(|w| w.len() >= 2)
        .collect();

    features
        .iter()
        .filter_map(|f| {
            let name = f.properties.corridor_name.to_ascii_lowercase();
            let mut score = 0usize;
            if name == q {
                score += 100;
            } else if name.contains(&q) {
                score += 50;
            } else {
                for w in &q_words {
                    if name.contains(w) {
                        score += 15;
                    }
                }
            }
            if score > 0 {
                Some((score, f))
            } else {
                None
            }
        })
        .max_by_key(|(s, _)| *s)
        .map(|(_, f)| f)
}

fn midpoint(coords: &[Vec<f64>]) -> (f64, f64) {
    if coords.is_empty() {
        return (-76.0910, -9.7120);
    }
    let mid = &coords[coords.len() / 2];
    if mid.len() >= 2 {
        (mid[0], mid[1])
    } else {
        (-76.0910, -9.7120)
    }
}
