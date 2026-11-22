#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

const GEOFABRIK_DIR_URL: &str = "https://download.geofabrik.de/south-america/peru/";
const GEOFABRIK_LATEST_URL: &str =
    "https://download.geofabrik.de/south-america/peru/peru-latest.osm.pbf";

/// Ensures a Peru OSM `.osm.pbf` snapshot exists at `target_path`, downloading from Geofabrik
/// if absent or `force = true`. Falls back from `peru-latest.osm.pbf` to the newest concrete
/// `peru-YYMMDD.osm.pbf` listed in Geofabrik's directory index if `latest` returns 404.
pub async fn ensure_peru_pbf(target_path: &Path, force: bool) -> Result<PathBuf, String> {
    if target_path.exists() && !force {
        let meta = std::fs::metadata(target_path).map_err(|e| e.to_string())?;
        if meta.len() > 1_000_000 {
            tracing::info!(path = %target_path.display(), bytes = meta.len(), "Using cached OSM PBF");
            return Ok(target_path.to_path_buf());
        }
    }

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let client = reqwest::Client::builder()
        .user_agent("PARADIXE-SOFT-Ingestor/0.1 (TakumiSoft)")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;

    let url = resolve_live_download_url(&client).await?;
    tracing::info!(url = %url, dest = %target_path.display(), "Downloading Geofabrik Peru PBF");

    let mut resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed downloading {url}: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Geofabrik returned HTTP {} for {url}",
            resp.status()
        ));
    }

    let tmp_path = target_path.with_extension("osm.pbf.part");
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut total_bytes: u64 = 0;
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        total_bytes += chunk.len() as u64;
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    if total_bytes < 10_000_000 {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(format!(
            "Downloaded PBF unexpectedly small ({total_bytes} bytes); aborted"
        ));
    }

    tokio::fs::rename(&tmp_path, target_path)
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!(path = %target_path.display(), bytes = total_bytes, "OSM PBF download completed");
    Ok(target_path.to_path_buf())
}

async fn resolve_live_download_url(client: &reqwest::Client) -> Result<String, String> {
    if let Ok(head) = client.head(GEOFABRIK_LATEST_URL).send().await {
        if head.status().is_success() {
            return Ok(GEOFABRIK_LATEST_URL.to_string());
        }
    }

    let html = client
        .get(GEOFABRIK_DIR_URL)
        .send()
        .await
        .map_err(|e| format!("Failed listing {GEOFABRIK_DIR_URL}: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let mut snapshots: Vec<String> = Vec::new();
    for segment in html.split("href=\"") {
        if let Some(end) = segment.find('"') {
            let candidate = &segment[..end];
            let filename = candidate.rsplit('/').next().unwrap_or(candidate);
            if filename.starts_with("peru-")
                && filename.ends_with(".osm.pbf")
                && filename != "peru-latest.osm.pbf"
                && filename[5..filename.len() - 8]
                    .bytes()
                    .all(|b| b.is_ascii_digit())
            {
                snapshots.push(filename.to_string());
            }
        }
    }
    snapshots.sort();
    snapshots.dedup();
    snapshots.reverse();

    for snap in snapshots.into_iter().take(5) {
        let url = format!("{GEOFABRIK_DIR_URL}{snap}");
        if let Ok(resp) = client.head(&url).send().await {
            if resp.status().is_success() {
                return Ok(url);
            }
        }
    }

    Err("No valid Geofabrik Peru .osm.pbf snapshot responded 200 OK".to_string())
}
