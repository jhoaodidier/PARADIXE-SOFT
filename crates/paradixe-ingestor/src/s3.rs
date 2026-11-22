#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket_tiles: String,
    pub bucket_media: String,
}

impl S3Config {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            endpoint: std::env::var("S3_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string())
                .trim_end_matches('/')
                .to_string(),
            region: std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            access_key: std::env::var("S3_ACCESS_KEY")
                .unwrap_or_else(|_| "rustfsadmin".to_string()),
            secret_key: std::env::var("S3_SECRET_KEY")
                .unwrap_or_else(|_| "rustfsadmin_dev_only".to_string()),
            bucket_tiles: std::env::var("S3_BUCKET_TILES")
                .unwrap_or_else(|_| "paradixe-tiles".to_string()),
            bucket_media: std::env::var("S3_BUCKET_MEDIA")
                .unwrap_or_else(|_| "paradixe-media".to_string()),
        })
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub struct S3SignRequest<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub canonical_uri: &'a str,
    pub canonical_query: &'a str,
    pub extra_headers: &'a [(&'a str, &'a str)],
    pub payload: &'a [u8],
    pub now: DateTime<Utc>,
}

pub fn sign_aws_v4(cfg: &S3Config, req: &S3SignRequest<'_>) -> Vec<(String, String)> {
    let amz_date = req.now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = req.now.format("%Y%m%d").to_string();
    let payload_hash = sha256_hex(req.payload);

    let mut hdrs: BTreeMap<String, String> = BTreeMap::new();
    hdrs.insert("host".to_string(), req.host.to_string());
    hdrs.insert("x-amz-content-sha256".to_string(), payload_hash.clone());
    hdrs.insert("x-amz-date".to_string(), amz_date.clone());
    for (k, v) in req.extra_headers {
        hdrs.insert(k.to_ascii_lowercase(), v.trim().to_string());
    }

    let mut canonical_headers = String::new();
    let mut signed_keys = Vec::with_capacity(hdrs.len());
    for (k, v) in &hdrs {
        canonical_headers.push_str(k);
        canonical_headers.push(':');
        canonical_headers.push_str(v);
        canonical_headers.push('\n');
        signed_keys.push(k.as_str());
    }
    let signed_headers = signed_keys.join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        req.method, req.canonical_uri, req.canonical_query
    );
    let scope = format!("{date_stamp}/{}/s3/aws4_request", cfg.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let k_date = hmac_sha256(
        format!("AWS4{}", cfg.secret_key).as_bytes(),
        date_stamp.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, cfg.region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        cfg.access_key
    );

    let mut out: Vec<(String, String)> = hdrs.into_iter().collect();
    out.push(("authorization".to_string(), authorization));
    out
}

fn parse_host(endpoint: &str) -> Result<String, String> {
    let without_scheme = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .ok_or_else(|| format!("Invalid S3 endpoint URL: {endpoint}"))?;
    Ok(without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string())
}

pub struct S3Client {
    cfg: S3Config,
    http: reqwest::Client,
    host: String,
}

impl S3Client {
    pub fn new(cfg: S3Config) -> Result<Self, String> {
        let host = parse_host(&cfg.endpoint)?;
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { cfg, http, host })
    }

    async fn send_signed(
        &self,
        method: reqwest::Method,
        canonical_uri: &str,
        canonical_query: &str,
        extra_headers: &[(&str, &str)],
        payload: Vec<u8>,
    ) -> Result<(reqwest::StatusCode, String), String> {
        let now = Utc::now();
        let headers = sign_aws_v4(
            &self.cfg,
            &S3SignRequest {
                method: method.as_str(),
                host: &self.host,
                canonical_uri,
                canonical_query,
                extra_headers,
                payload: &payload,
                now,
            },
        );
        let url = if canonical_query.is_empty() {
            format!("{}{}", self.cfg.endpoint, canonical_uri)
        } else {
            format!("{}{}?{}", self.cfg.endpoint, canonical_uri, canonical_query)
        };

        let mut req = self.http.request(method, &url).body(payload);
        for (k, v) in headers {
            if k != "host" {
                req = req.header(k, v);
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("S3 request failed for {url}: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        Ok((status, text))
    }

    pub async fn ensure_bucket(&self, bucket: &str, public_read: bool) -> Result<(), String> {
        let uri = format!("/{bucket}");
        let (head_status, _) = self
            .send_signed(reqwest::Method::HEAD, &uri, "", &[], Vec::new())
            .await?;

        if head_status.as_u16() == 404 {
            let (create_status, create_body) = self
                .send_signed(reqwest::Method::PUT, &uri, "", &[], Vec::new())
                .await?;
            if !create_status.is_success() && create_status.as_u16() != 409 {
                return Err(format!(
                    "Failed creating bucket '{bucket}': HTTP {create_status} {create_body}"
                ));
            }
        }

        if public_read {
            let policy = serde_json::json!({
                "Version": "2012-10-17",
                "Statement": [{
                    "Effect": "Allow",
                    "Principal": {"AWS": ["*"]},
                    "Action": ["s3:GetObject"],
                    "Resource": [format!("arn:aws:s3:::{bucket}/*")]
                }]
            });
            let payload = serde_json::to_vec(&policy).map_err(|e| e.to_string())?;
            let (pol_status, pol_body) = self
                .send_signed(
                    reqwest::Method::PUT,
                    &uri,
                    "policy=",
                    &[("content-type", "application/json")],
                    payload,
                )
                .await?;
            if !pol_status.is_success() {
                tracing::warn!(
                    bucket = %bucket,
                    status = %pol_status,
                    body = %pol_body,
                    "Could not set public-read policy; objects may require presigned URLs"
                );
            }
        }
        Ok(())
    }

    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<String, String> {
        let clean_key = key.trim_start_matches('/');
        let uri = format!("/{bucket}/{clean_key}");
        let (status, body) = self
            .send_signed(
                reqwest::Method::PUT,
                &uri,
                "",
                &[("content-type", content_type)],
                bytes,
            )
            .await?;
        if !status.is_success() {
            return Err(format!(
                "Failed uploading s3://{bucket}/{clean_key}: HTTP {status} {body}"
            ));
        }
        Ok(format!("{}{uri}", self.cfg.endpoint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn sigv4_produces_deterministic_authorization_header() {
        let cfg = S3Config {
            endpoint: "http://127.0.0.1:9000".to_string(),
            region: "us-east-1".to_string(),
            access_key: "rustfsadmin".to_string(),
            secret_key: "rustfsadmin_dev_only".to_string(),
            bucket_tiles: "paradixe-tiles".to_string(),
            bucket_media: "paradixe-media".to_string(),
        };
        let ts = Utc.with_ymd_and_hms(2026, 11, 22, 0, 0, 0).unwrap();
        let req = S3SignRequest {
            method: "PUT",
            host: "127.0.0.1:9000",
            canonical_uri: "/paradixe-tiles/pe-huc.pmtiles",
            canonical_query: "",
            extra_headers: &[("content-type", "application/vnd.pmtiles")],
            payload: b"PMTiles\x03",
            now: ts,
        };
        let h1 = sign_aws_v4(&cfg, &req);
        let h2 = sign_aws_v4(&cfg, &req);
        assert_eq!(h1, h2);
        let auth = h1.iter().find(|(k, _)| k == "authorization").unwrap();
        assert!(auth.1.starts_with(
            "AWS4-HMAC-SHA256 Credential=rustfsadmin/20261122/us-east-1/s3/aws4_request"
        ));
    }
}
