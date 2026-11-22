#![forbid(unsafe_code)]

use std::sync::atomic::Ordering;

use axum::{body::Bytes, extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{ApiError, ValidatedJson},
    AppState,
};

type HmacSha256 = Hmac<Sha256>;

const MAX_EVIDENCE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresignUploadRequest {
    pub content_type: String,
    pub size_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct PresignUploadResponse {
    pub object_key: String,
    pub presigned_put_url: String,
    pub photo_uri: String,
    pub expires_in_secs: u32,
}

#[derive(Debug, Serialize)]
pub struct EvidenceUploadResponse {
    pub object_key: String,
    pub photo_uri: String,
    pub content_type: String,
    pub size_bytes: usize,
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn aws_uri_encode(raw: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(raw.len() * 2);
    for b in raw.as_bytes() {
        let is_unreserved = b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.' | b'~');
        if is_unreserved || (*b == b'/' && !encode_slash) {
            out.push(*b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

fn parse_endpoint_host(endpoint: &str) -> Result<String, ApiError> {
    let stripped = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .ok_or_else(|| ApiError::internal("Invalid S3_ENDPOINT URL"))?;
    Ok(stripped.split('/').next().unwrap_or(stripped).to_string())
}

fn derive_signing_key(secret_key: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(
        format!("AWS4{secret_key}").as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

struct S3SignHeaderInput<'a> {
    method: &'a str,
    host: &'a str,
    canonical_uri: &'a str,
    canonical_query: &'a str,
    content_type: Option<&'a str>,
    payload: &'a [u8],
    now: DateTime<Utc>,
}

fn sign_s3_headers(state: &AppState, input: &S3SignHeaderInput<'_>) -> Vec<(String, String)> {
    let amz_date = input.now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = input.now.format("%Y%m%d").to_string();
    let payload_hash = sha256_hex(input.payload);

    let mut headers = vec![
        ("host".to_string(), input.host.to_string()),
        ("x-amz-content-sha256".to_string(), payload_hash.clone()),
        ("x-amz-date".to_string(), amz_date.clone()),
    ];
    if let Some(ct) = input.content_type {
        headers.push(("content-type".to_string(), ct.trim().to_string()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let mut canonical_headers = String::new();
    let mut signed_keys = Vec::with_capacity(headers.len());
    for (k, v) in &headers {
        canonical_headers.push_str(k);
        canonical_headers.push(':');
        canonical_headers.push_str(v);
        canonical_headers.push('\n');
        signed_keys.push(k.as_str());
    }
    let signed_headers = signed_keys.join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        input.method, input.canonical_uri, input.canonical_query
    );
    let scope = format!("{date_stamp}/{}/s3/aws4_request", state.config.s3_region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let signing_key = derive_signing_key(
        state.config.s3_secret_key.expose_secret(),
        &date_stamp,
        &state.config.s3_region,
    );
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        state.config.s3_access_key
    );

    headers.push(("authorization".to_string(), authorization));
    headers
}

fn build_presigned_put_url(
    state: &AppState,
    bucket: &str,
    object_key: &str,
    expires_in_secs: u32,
    now: DateTime<Utc>,
) -> Result<String, ApiError> {
    let host = parse_endpoint_host(&state.config.s3_endpoint)?;
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let scope = format!("{date_stamp}/{}/s3/aws4_request", state.config.s3_region);
    let credential = format!("{}/{scope}", state.config.s3_access_key);

    let canonical_uri = format!(
        "/{}/{}",
        aws_uri_encode(bucket, false),
        aws_uri_encode(object_key, false)
    );
    let canonical_query = format!(
        "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential={}&X-Amz-Date={}&X-Amz-Expires={}&X-Amz-SignedHeaders=host",
        aws_uri_encode(&credential, true),
        amz_date,
        expires_in_secs
    );
    let canonical_request =
        format!("PUT\n{canonical_uri}\n{canonical_query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = derive_signing_key(
        state.config.s3_secret_key.expose_secret(),
        &date_stamp,
        &state.config.s3_region,
    );
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    Ok(format!(
        "{}{canonical_uri}?{canonical_query}&X-Amz-Signature={signature}",
        state.config.s3_endpoint
    ))
}

async fn send_s3_signed(
    state: &AppState,
    method: reqwest::Method,
    canonical_uri: &str,
    canonical_query: &str,
    content_type: Option<&str>,
    payload: Vec<u8>,
) -> Result<(StatusCode, String), ApiError> {
    let host = parse_endpoint_host(&state.config.s3_endpoint)?;
    let now = Utc::now();
    let headers = sign_s3_headers(
        state,
        &S3SignHeaderInput {
            method: method.as_str(),
            host: &host,
            canonical_uri,
            canonical_query,
            content_type,
            payload: &payload,
            now,
        },
    );
    let url = if canonical_query.is_empty() {
        format!("{}{canonical_uri}", state.config.s3_endpoint)
    } else {
        format!(
            "{}{canonical_uri}?{canonical_query}",
            state.config.s3_endpoint
        )
    };

    let mut req = state.http_client.request(method, &url).body(payload);
    for (k, v) in headers {
        if k != "host" {
            req = req.header(k, v);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("S3 storage error: {e}")))?;
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let text = resp.text().await.unwrap_or_default();
    Ok((status, text))
}

async fn ensure_evidence_bucket(state: &AppState) -> Result<(), ApiError> {
    let bucket = &state.config.s3_bucket_evidence;
    let uri = format!("/{bucket}");
    let (head_status, _) =
        send_s3_signed(state, reqwest::Method::HEAD, &uri, "", None, Vec::new()).await?;

    if head_status == StatusCode::NOT_FOUND {
        let (create_status, create_body) =
            send_s3_signed(state, reqwest::Method::PUT, &uri, "", None, Vec::new()).await?;
        if !create_status.is_success() && create_status != StatusCode::CONFLICT {
            return Err(ApiError::internal(format!(
                "Failed creating evidence bucket '{bucket}': HTTP {create_status} {create_body}"
            )));
        }

        let policy = serde_json::json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Effect": "Allow",
                "Principal": {"AWS": ["*"]},
                "Action": ["s3:GetObject"],
                "Resource": [format!("arn:aws:s3:::{bucket}/*")]
            }]
        });
        let payload = serde_json::to_vec(&policy).map_err(ApiError::internal)?;
        let _ = send_s3_signed(
            state,
            reqwest::Method::PUT,
            &uri,
            "policy=",
            Some("application/json"),
            payload,
        )
        .await;
    }

    Ok(())
}

fn detect_image_magic(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
        Some(("image/jpeg", "jpg"))
    } else if bytes.len() >= 8 && bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        Some(("image/png", "png"))
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

fn extension_for_content_type(ct: &str) -> Option<&'static str> {
    match ct.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

pub async fn presign_upload(
    State(state): State<AppState>,
    _auth: AuthUser,
    ValidatedJson(body): ValidatedJson<PresignUploadRequest>,
) -> Result<Json<PresignUploadResponse>, ApiError> {
    let ext = extension_for_content_type(&body.content_type).ok_or_else(|| {
        ApiError::validation("content_type must be image/jpeg, image/png, or image/webp")
    })?;
    if body.size_bytes == 0 || body.size_bytes > MAX_EVIDENCE_BYTES {
        return Err(ApiError::validation(
            "size_bytes must be between 1 and 2097152 bytes",
        ));
    }

    ensure_evidence_bucket(&state).await?;

    let now = Utc::now();
    let object_key = format!("{}/{}.{}", now.format("%Y%m%d"), Uuid::new_v4(), ext);
    let expires_in_secs = 300;
    let presigned_put_url = build_presigned_put_url(
        &state,
        &state.config.s3_bucket_evidence,
        &object_key,
        expires_in_secs,
        now,
    )?;
    let photo_uri = format!("{}/{object_key}", state.config.s3_public_evidence_base);

    Ok(Json(PresignUploadResponse {
        object_key,
        presigned_put_url,
        photo_uri,
        expires_in_secs,
    }))
}

pub async fn upload_evidence(
    State(state): State<AppState>,
    _auth: AuthUser,
    bytes: Bytes,
) -> Result<(StatusCode, Json<EvidenceUploadResponse>), ApiError> {
    if bytes.is_empty() || bytes.len() > MAX_EVIDENCE_BYTES {
        return Err(ApiError::validation(
            "Image body must be between 1 and 2097152 bytes",
        ));
    }
    let (content_type, ext) = detect_image_magic(&bytes).ok_or_else(|| {
        ApiError::validation("Only JPEG, PNG, or WebP image payloads are permitted")
    })?;

    ensure_evidence_bucket(&state).await?;

    let now = Utc::now();
    let object_key = format!("{}/{}.{}", now.format("%Y%m%d"), Uuid::new_v4(), ext);
    let uri = format!("/{}/{object_key}", state.config.s3_bucket_evidence);
    let size_bytes = bytes.len();

    let (status, resp_body) = send_s3_signed(
        &state,
        reqwest::Method::PUT,
        &uri,
        "",
        Some(content_type),
        bytes.to_vec(),
    )
    .await?;

    if !status.is_success() {
        return Err(ApiError::internal(format!(
            "Failed storing evidence image: HTTP {status} {resp_body}"
        )));
    }

    state
        .metrics
        .evidence_uploads
        .fetch_add(1, Ordering::Relaxed);
    let photo_uri = format!("{}/{object_key}", state.config.s3_public_evidence_base);

    Ok((
        StatusCode::CREATED,
        Json(EvidenceUploadResponse {
            object_key,
            photo_uri,
            content_type: content_type.to_string(),
            size_bytes,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_valid_image_magic_bytes_and_rejects_scripts() {
        assert_eq!(
            detect_image_magic(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some(("image/jpeg", "jpg"))
        );
        assert_eq!(
            detect_image_magic(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            Some(("image/png", "png"))
        );
        assert_eq!(
            detect_image_magic(b"RIFF\x24\x00\x00\x00WEBPVP8 "),
            Some(("image/webp", "webp"))
        );
        assert_eq!(
            detect_image_magic(b"<svg><script>alert(1)</script></svg>"),
            None
        );
    }
}
