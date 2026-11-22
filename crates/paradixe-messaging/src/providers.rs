#![forbid(unsafe_code)]

use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;

use crate::signature::mask_phone_e164;

#[derive(Clone)]
pub struct MessagingConfig {
    pub bind_addr: String,
    pub api_base_url: String,
    pub default_region: String,
    pub webhook_secret: SecretString,
    pub whatsapp_app_secret: SecretString,
    pub whatsapp_verify_token: SecretString,
    pub whatsapp_api_url: String,
    pub whatsapp_phone_number_id: String,
    pub whatsapp_access_token: Option<SecretString>,
    pub sms_provider_url: Option<String>,
    pub sms_provider_api_key: Option<SecretString>,
}

impl MessagingConfig {
    pub fn from_env() -> Result<Self, String> {
        let webhook_secret = std::env::var("WHATSAPP_WEBHOOK_SECRET")
            .unwrap_or_else(|_| "paradixe_dev_webhook_secret_only".to_string());
        if webhook_secret.len() < 16 {
            return Err("WHATSAPP_WEBHOOK_SECRET must be at least 16 bytes".to_string());
        }
        let whatsapp_app_secret = std::env::var("WHATSAPP_APP_SECRET")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| webhook_secret.clone());
        let whatsapp_verify_token = std::env::var("WHATSAPP_VERIFY_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "paradixe_verify_dev_token".to_string());

        let bind_addr =
            std::env::var("MESSAGING_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8090".to_string());
        let api_base_url = std::env::var("PARADIXE_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080/api/v1".to_string())
            .trim_end_matches('/')
            .to_string();
        let default_region =
            std::env::var("PARADIXE_DEFAULT_REGION").unwrap_or_else(|_| "PE-HUC".to_string());

        let whatsapp_api_url = std::env::var("WHATSAPP_API_URL")
            .unwrap_or_else(|_| "https://graph.facebook.com/v21.0".to_string())
            .trim_end_matches('/')
            .to_string();
        let whatsapp_phone_number_id =
            std::env::var("WHATSAPP_PHONE_NUMBER_ID").unwrap_or_default();
        let whatsapp_access_token = std::env::var("WHATSAPP_ACCESS_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(SecretString::from);

        let sms_provider_url = std::env::var("SMS_PROVIDER_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());
        let sms_provider_api_key = std::env::var("SMS_PROVIDER_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(SecretString::from);

        Ok(Self {
            bind_addr,
            api_base_url,
            default_region,
            webhook_secret: SecretString::from(webhook_secret),
            whatsapp_app_secret: SecretString::from(whatsapp_app_secret),
            whatsapp_verify_token: SecretString::from(whatsapp_verify_token),
            whatsapp_api_url,
            whatsapp_phone_number_id,
            whatsapp_access_token,
            sms_provider_url,
            sms_provider_api_key,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchResult {
    pub channel_used: &'static str,
    pub mode: &'static str,
    pub masked_recipient: String,
    pub delivered: bool,
}

#[derive(Clone)]
pub struct OutboundDispatcher {
    http: reqwest::Client,
    config: MessagingConfig,
}

impl OutboundDispatcher {
    pub fn new(config: MessagingConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    pub async fn send_whatsapp_with_sms_fallback(
        &self,
        phone_e164: &str,
        message_text: &str,
    ) -> DispatchResult {
        let masked = mask_phone_e164(phone_e164);

        if let Some(token) = &self.config.whatsapp_access_token {
            if !self.config.whatsapp_phone_number_id.is_empty() {
                let url = format!(
                    "{}/{}/messages",
                    self.config.whatsapp_api_url, self.config.whatsapp_phone_number_id
                );
                let payload = serde_json::json!({
                    "messaging_product": "whatsapp",
                    "to": phone_e164.trim_start_matches('+'),
                    "type": "text",
                    "text": { "body": message_text }
                });
                match self
                    .http
                    .post(&url)
                    .bearer_auth(token.expose_secret())
                    .json(&payload)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!(recipient = %masked, "WhatsApp Cloud API message sent");
                        return DispatchResult {
                            channel_used: "WHATSAPP_BOT",
                            mode: "LIVE",
                            masked_recipient: masked,
                            delivered: true,
                        };
                    }
                    Ok(resp) => {
                        tracing::warn!(status = %resp.status(), recipient = %masked, "WhatsApp API rejected; attempting SMS fallback");
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, recipient = %masked, "WhatsApp API unreachable; attempting SMS fallback");
                    }
                }
            }
        }

        let sms_res = self.send_sms(phone_e164, message_text).await;
        if sms_res.delivered || sms_res.mode == "DRY_RUN" {
            return DispatchResult {
                channel_used: if self.config.whatsapp_access_token.is_none()
                    && self.config.sms_provider_url.is_none()
                {
                    "WHATSAPP_BOT"
                } else {
                    sms_res.channel_used
                },
                mode: sms_res.mode,
                masked_recipient: masked,
                delivered: sms_res.delivered,
            };
        }
        sms_res
    }

    pub async fn send_sms(&self, phone_e164: &str, message_text: &str) -> DispatchResult {
        let masked = mask_phone_e164(phone_e164);
        let sms_body = crate::parser::truncate_sms_160(message_text);

        if let (Some(url), Some(api_key)) = (
            self.config.sms_provider_url.as_deref(),
            self.config.sms_provider_api_key.as_ref(),
        ) {
            let payload = serde_json::json!({
                "to": phone_e164,
                "message": sms_body,
            });
            match self
                .http
                .post(url)
                .bearer_auth(api_key.expose_secret())
                .json(&payload)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!(recipient = %masked, "SMS fallback dispatched");
                    return DispatchResult {
                        channel_used: "SMS",
                        mode: "LIVE",
                        masked_recipient: masked,
                        delivered: true,
                    };
                }
                Ok(resp) => {
                    tracing::error!(status = %resp.status(), recipient = %masked, "SMS provider HTTP failure");
                }
                Err(err) => {
                    tracing::error!(error = %err, recipient = %masked, "SMS provider network error");
                }
            }
            return DispatchResult {
                channel_used: "SMS",
                mode: "LIVE",
                masked_recipient: masked,
                delivered: false,
            };
        }

        tracing::info!(recipient = %masked, chars = sms_body.chars().count(), "Messaging dry-run preview prepared");
        DispatchResult {
            channel_used: "SMS",
            mode: "DRY_RUN",
            masked_recipient: masked,
            delivered: true,
        }
    }
}
