#![forbid(unsafe_code)]

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Computes canonical `sha256=<hex>` signature used by Meta webhooks and `paradixe-api`.
pub fn compute_sha256_signature(secret: &[u8], payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(payload);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// Constant-time verification of `sha256=<hex>` header against raw body bytes.
pub fn verify_sha256_signature(secret: &[u8], payload: &[u8], header: &str) -> bool {
    let Some(sig_hex) = header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(provided_bytes) = hex::decode(sig_hex.trim()) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(payload);
    let expected = mac.finalize().into_bytes();
    if provided_bytes.len() != expected.len() {
        return false;
    }
    bool::from(expected.as_slice().ct_eq(&provided_bytes))
}

/// Constant-time equality for Meta webhook `hub.verify_token` handshakes.
pub fn verify_token_constant_time(expected: &str, provided: &str) -> bool {
    let expected_bytes = expected.as_bytes();
    let provided_bytes = provided.as_bytes();
    if expected_bytes.len() != provided_bytes.len() {
        return false;
    }
    bool::from(expected_bytes.ct_eq(provided_bytes))
}

/// Masks all but the last 4 digits of a phone number for SEC-07 log redaction.
pub fn mask_phone_e164(phone: &str) -> String {
    let trimmed = phone.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 4 {
        return "****".to_string();
    }
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("***{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_roundtrip_and_tamper_rejection() {
        let secret = b"paradixe_webhook_secret_test_1234";
        let payload = br#"{"channel":"WHATSAPP_BOT","ok":true}"#;
        let sig = compute_sha256_signature(secret, payload);
        assert!(verify_sha256_signature(secret, payload, &sig));
        assert!(!verify_sha256_signature(secret, b"{}", &sig));
        assert!(!verify_sha256_signature(secret, payload, "invalid-header"));
        assert!(!verify_sha256_signature(b"wrong", payload, &sig));
    }

    #[test]
    fn phone_masking_hides_pii() {
        assert_eq!(mask_phone_e164("+51962000100"), "***0100");
        assert_eq!(mask_phone_e164("123"), "****");
    }
}
