#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use chrono::{DateTime, Utc};
use paradixe_domain::RiskLevel;
use paradixe_ivv::{calculate_ivv, IvvModelParams, SegmentGeoInput, WeatherInput};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::regions::RegionSpec;

const WEATHER_CELL_DEG: f64 = 0.20;
const OPEN_METEO_CELLS_PER_BATCH: usize = 40;
const DB_CHUNK_SIZE: usize = 500;

type GridCellCoord = ((i32, i32), (f64, f64));

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            Self::One(item) => vec![item],
            Self::Many(items) => items,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenMeteoHourly {
    time: Vec<String>,
    precipitation: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoForecastItem {
    hourly: OpenMeteoHourly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrecipTotals {
    pub precip_24h_mm: Decimal,
    pub precip_72h_mm: Decimal,
}

fn dec2_precip(mm: f64) -> Decimal {
    let scaled = (mm.clamp(0.0, 9999.99) * 100.0).round() as i64;
    Decimal::new(scaled, 2)
}

/// Sums trailing 24h and 72h hourly precipitation ending at `window_end`
/// (ensuring `precip_72h_mm >= precip_24h_mm`). Works for both current observations and +24/+48/+72h forecasts.
pub fn accumulate_trailing_precipitation(
    times: &[String],
    precip_mm: &[Option<f64>],
    window_end: DateTime<Utc>,
) -> PrecipTotals {
    let mut sum_24 = 0.0f64;
    let mut sum_72 = 0.0f64;
    let end_naive = window_end.naive_utc();
    let cutoff_24 = end_naive - chrono::Duration::hours(24);
    let cutoff_72 = end_naive - chrono::Duration::hours(72);

    for (t_str, val_opt) in times.iter().zip(precip_mm.iter()) {
        let Some(mm) = val_opt.filter(|v| v.is_finite() && *v >= 0.0) else {
            continue;
        };
        let Ok(dt) = chrono::NaiveDateTime::parse_from_str(t_str, "%Y-%m-%dT%H:%M") else {
            continue;
        };
        if dt > end_naive || dt < cutoff_72 {
            continue;
        }
        sum_72 += mm;
        if dt >= cutoff_24 {
            sum_24 += mm;
        }
    }

    let p24 = dec2_precip(sum_24);
    let p72 = dec2_precip(sum_72).max(p24);
    PrecipTotals {
        precip_24h_mm: p24,
        precip_72h_mm: p72,
    }
}

struct SegRowMeta {
    segment_id: Uuid,
    origin_node_id: Uuid,
    destination_node_id: Uuid,
    cell: (i32, i32),
    geo: SegmentGeoInput,
}

async fn load_segments_and_grid_cells(
    pool: &PgPool,
    region: Option<&RegionSpec>,
) -> Result<(Vec<SegRowMeta>, Vec<GridCellCoord>), String> {
    let seg_rows = if let Some(reg) = region {
        sqlx::query(
            "SELECT rs.segment_id, rs.origin_node_id, rs.destination_node_id,
                    rs.max_slope_forward_pct, rs.max_slope_reverse_pct,
                    rs.soil_susceptibility_idx, rs.distance_to_ravine_m, rs.is_historical_hotspot,
                    ST_X(ST_Centroid(rs.geom)) AS cen_lon,
                    ST_Y(ST_Centroid(rs.geom)) AS cen_lat
             FROM road_segments rs
             WHERE rs.is_topology_verified = TRUE
               AND ST_Intersects(rs.geom, ST_GeomFromText($1, 4326))",
        )
        .bind(reg.bbox.padded(0.05).polygon_wkt())
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query(
            "SELECT rs.segment_id, rs.origin_node_id, rs.destination_node_id,
                    rs.max_slope_forward_pct, rs.max_slope_reverse_pct,
                    rs.soil_susceptibility_idx, rs.distance_to_ravine_m, rs.is_historical_hotspot,
                    ST_X(ST_Centroid(rs.geom)) AS cen_lon,
                    ST_Y(ST_Centroid(rs.geom)) AS cen_lat
             FROM road_segments rs
             WHERE rs.is_topology_verified = TRUE",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    };

    let mut cell_keys: BTreeMap<(i32, i32), (f64, f64)> = BTreeMap::new();
    let mut segments = Vec::with_capacity(seg_rows.len());
    for r in seg_rows {
        let cen_lon: f64 = r.try_get("cen_lon").map_err(|e| e.to_string())?;
        let cen_lat: f64 = r.try_get("cen_lat").map_err(|e| e.to_string())?;
        let cell = (
            (cen_lon / WEATHER_CELL_DEG).round() as i32,
            (cen_lat / WEATHER_CELL_DEG).round() as i32,
        );
        cell_keys.entry(cell).or_insert_with(|| {
            (
                f64::from(cell.0) * WEATHER_CELL_DEG,
                f64::from(cell.1) * WEATHER_CELL_DEG,
            )
        });

        let dist_rav: i32 = r
            .try_get("distance_to_ravine_m")
            .map_err(|e| e.to_string())?;
        segments.push(SegRowMeta {
            segment_id: r.try_get("segment_id").map_err(|e| e.to_string())?,
            origin_node_id: r.try_get("origin_node_id").map_err(|e| e.to_string())?,
            destination_node_id: r
                .try_get("destination_node_id")
                .map_err(|e| e.to_string())?,
            cell,
            geo: SegmentGeoInput {
                max_slope_forward_pct: r
                    .try_get("max_slope_forward_pct")
                    .map_err(|e| e.to_string())?,
                max_slope_reverse_pct: r
                    .try_get("max_slope_reverse_pct")
                    .map_err(|e| e.to_string())?,
                soil_susceptibility_idx: r
                    .try_get("soil_susceptibility_idx")
                    .map_err(|e| e.to_string())?,
                distance_to_ravine_m: Decimal::from(dist_rav),
                is_historical_hotspot: r
                    .try_get("is_historical_hotspot")
                    .map_err(|e| e.to_string())?,
            },
        });
    }

    Ok((segments, cell_keys.into_iter().collect()))
}

async fn append_sync_change(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: Uuid,
    operation: &str,
    payload_json: serde_json::Value,
) -> Result<(), String> {
    let row = sqlx::query(
        "UPDATE sync_control SET last_seq = last_seq + 1 WHERE singleton_id = 1 RETURNING last_seq",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    let change_seq: i64 = row.try_get("last_seq").map_err(|e| e.to_string())?;

    sqlx::query(
        "INSERT INTO sync_change_log (change_seq, entity_type, entity_id, operation, payload_json)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(change_seq)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(sqlx::types::Json(payload_json))
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn load_active_ivv_model(pool: &PgPool) -> Result<(Uuid, IvvModelParams), String> {
    let row = sqlx::query(
        "SELECT model_version_id, version_tag, w_precip, w_slope_ravine, w_soil, w_hotspot,
                precip_24h_cap_mm, precip_72h_cap_mm, slope_cap_pct, ravine_max_dist_m,
                amber_threshold, red_threshold, validity_hours
         FROM ivv_model_versions
         WHERE is_active = TRUE
         LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    let id: Uuid = row.try_get("model_version_id").map_err(|e| e.to_string())?;
    let dist: i32 = row
        .try_get("ravine_max_dist_m")
        .map_err(|e| e.to_string())?;
    let vh: i16 = row.try_get("validity_hours").map_err(|e| e.to_string())?;

    let params = IvvModelParams {
        version_tag: row.try_get("version_tag").map_err(|e| e.to_string())?,
        w_precip: row.try_get("w_precip").map_err(|e| e.to_string())?,
        w_slope_ravine: row.try_get("w_slope_ravine").map_err(|e| e.to_string())?,
        w_soil: row.try_get("w_soil").map_err(|e| e.to_string())?,
        w_hotspot: row.try_get("w_hotspot").map_err(|e| e.to_string())?,
        precip_24h_cap_mm: row
            .try_get("precip_24h_cap_mm")
            .map_err(|e| e.to_string())?,
        precip_72h_cap_mm: row
            .try_get("precip_72h_cap_mm")
            .map_err(|e| e.to_string())?,
        slope_cap_pct: row.try_get("slope_cap_pct").map_err(|e| e.to_string())?,
        ravine_max_dist_m: Decimal::from(dist),
        amber_threshold: row.try_get("amber_threshold").map_err(|e| e.to_string())?,
        red_threshold: row.try_get("red_threshold").map_err(|e| e.to_string())?,
        validity_hours: i64::from(vh),
    };
    Ok((id, params))
}

async fn fetch_open_meteo_batch_with_retry(
    client: &reqwest::Client,
    url: &str,
    lats: &str,
    lons: &str,
    forecast_days: &str,
) -> Result<Vec<OpenMeteoForecastItem>, String> {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let resp = client
            .get(url)
            .query(&[
                ("latitude", lats),
                ("longitude", lons),
                ("hourly", "precipitation"),
                ("past_days", "3"),
                ("forecast_days", forecast_days),
                ("timezone", "UTC"),
            ])
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let parsed: OneOrMany<OpenMeteoForecastItem> =
                    r.json().await.map_err(|e| e.to_string())?;
                return Ok(parsed.into_vec());
            }
            Ok(r) if attempt < 4 && r.status().as_u16() == 429 => {
                let retry_after_secs = r
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(|s| s.clamp(5, 180))
                    .unwrap_or(65);
                tracing::warn!(
                    attempt,
                    wait_secs = retry_after_secs,
                    "Open-Meteo forecast rate limited (429); backing off"
                );
                tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
            }
            Ok(r) if attempt < 4 && r.status().is_server_error() => {
                let wait_secs = 2u64 * u64::from(attempt);
                tracing::warn!(
                    status = %r.status(),
                    attempt,
                    wait_secs,
                    "Open-Meteo forecast 5xx; retrying"
                );
                tokio::time::sleep(Duration::from_secs(wait_secs)).await;
            }
            Ok(r) => {
                return Err(format!("Open-Meteo forecast HTTP {}", r.status()));
            }
            Err(e) if attempt < 4 => {
                let wait_secs = 2u64 * u64::from(attempt);
                tracing::warn!(error = %e, attempt, wait_secs, "Open-Meteo forecast error; retrying");
                tokio::time::sleep(Duration::from_secs(wait_secs)).await;
            }
            Err(e) => {
                return Err(format!("Open-Meteo forecast request failed: {e}"));
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WeatherSyncStats {
    pub grid_cells_queried: usize,
    pub segments_updated: usize,
    pub green_count: usize,
    pub amber_count: usize,
    pub red_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ForecastSyncStats {
    pub grid_cells_queried: usize,
    pub forecast_rows_upserted: usize,
}

async fn query_grid_cells_weather(
    cells_vec: &[GridCellCoord],
    forecast_days: &str,
) -> Result<HashMap<(i32, i32), OpenMeteoForecastItem>, String> {
    let base_url = std::env::var("OPEN_METEO_BASE_URL")
        .unwrap_or_else(|_| "https://api.open-meteo.com/v1".to_string())
        .trim_end_matches('/')
        .to_string();
    let client = reqwest::Client::builder()
        .user_agent("PARADIXE-SOFT-Ingestor/0.1 (TakumiSoft)")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{base_url}/forecast");
    let mut by_cell = HashMap::with_capacity(cells_vec.len());

    for chunk in cells_vec.chunks(OPEN_METEO_CELLS_PER_BATCH) {
        let lats = chunk
            .iter()
            .map(|(_, (_, lat))| format!("{lat:.4}"))
            .collect::<Vec<_>>()
            .join(",");
        let lons = chunk
            .iter()
            .map(|(_, (lon, _))| format!("{lon:.4}"))
            .collect::<Vec<_>>()
            .join(",");

        let items =
            fetch_open_meteo_batch_with_retry(&client, &url, &lats, &lons, forecast_days).await?;
        if items.len() != chunk.len() {
            return Err(format!(
                "Open-Meteo returned {} items for {} cells",
                items.len(),
                chunk.len()
            ));
        }
        for ((cell, _), item) in chunk.iter().zip(items) {
            by_cell.insert(*cell, item);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Ok(by_cell)
}

/// Fetches real 72h Open-Meteo precipitation across 0.20-degree spatial grid cells covering
/// verified road segments (optionally restricted to `region`) and recalculates IVV scores.
pub async fn sync_weather_and_ivv(
    pool: &PgPool,
    region: Option<&RegionSpec>,
) -> Result<WeatherSyncStats, String> {
    let (model_id, model_params) = load_active_ivv_model(pool).await?;
    let (segments, cells_vec) = load_segments_and_grid_cells(pool, region).await?;
    if segments.is_empty() {
        return Ok(WeatherSyncStats::default());
    }

    let now = Utc::now();
    let raw_by_cell = query_grid_cells_weather(&cells_vec, "1").await?;
    let precip_by_cell: HashMap<(i32, i32), PrecipTotals> = raw_by_cell
        .into_iter()
        .map(|(cell, item)| {
            (
                cell,
                accumulate_trailing_precipitation(
                    &item.hourly.time,
                    &item.hourly.precipitation,
                    now,
                ),
            )
        })
        .collect();

    let mut stats = WeatherSyncStats {
        grid_cells_queried: cells_vec.len(),
        ..Default::default()
    };

    for seg_chunk in segments.chunks(DB_CHUNK_SIZE) {
        let mut obs_ids = Vec::with_capacity(seg_chunk.len());
        let mut obs_seg_ids = Vec::with_capacity(seg_chunk.len());
        let mut p24_vals = Vec::with_capacity(seg_chunk.len());
        let mut p72_vals = Vec::with_capacity(seg_chunk.len());
        let mut assess_ids = Vec::with_capacity(seg_chunk.len());
        let mut ivv_scores = Vec::with_capacity(seg_chunk.len());
        let mut red_seg_ids = Vec::new();
        let mut red_node_ids = Vec::new();

        for seg in seg_chunk {
            let totals = precip_by_cell
                .get(&seg.cell)
                .copied()
                .unwrap_or(PrecipTotals {
                    precip_24h_mm: Decimal::ZERO,
                    precip_72h_mm: Decimal::ZERO,
                });
            let ivv_out = calculate_ivv(
                &model_params,
                &seg.geo,
                &WeatherInput {
                    precip_24h_mm: totals.precip_24h_mm,
                    precip_72h_mm: totals.precip_72h_mm,
                    observed_at: now,
                },
                Some(now),
                Some(now),
            )
            .map_err(|e| e.to_string())?;

            let obs_id = Uuid::new_v4();
            let assessment_id = Uuid::new_v4();

            obs_ids.push(obs_id);
            obs_seg_ids.push(seg.segment_id);
            p24_vals.push(totals.precip_24h_mm);
            p72_vals.push(totals.precip_72h_mm);
            assess_ids.push(assessment_id);
            ivv_scores.push(ivv_out.ivv_score);

            match ivv_out.risk_level {
                RiskLevel::Green => stats.green_count += 1,
                RiskLevel::Amber => stats.amber_count += 1,
                RiskLevel::Red => {
                    stats.red_count += 1;
                    red_seg_ids.push(seg.segment_id);
                    red_node_ids.push(seg.origin_node_id);
                    red_node_ids.push(seg.destination_node_id);
                }
                RiskLevel::Unknown => {}
            }
            stats.segments_updated += 1;
        }

        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT INTO weather_observations (
                observation_id, segment_id, source_name, precip_24h_mm, precip_72h_mm, observed_at
             )
             SELECT v.id, v.seg_id, 'OPEN_METEO', v.p24, v.p72, $5
             FROM UNNEST($1::uuid[], $2::uuid[], $3::numeric[], $4::numeric[]) AS v(id, seg_id, p24, p72)",
        )
        .bind(&obs_ids)
        .bind(&obs_seg_ids)
        .bind(&p24_vals)
        .bind(&p72_vals)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        sqlx::query(
            "INSERT INTO ivv_assessments (assessment_id, observation_id, model_version_id, ivv_score, calculated_at)
             SELECT v.a_id, v.o_id, $3, v.score, $4
             FROM UNNEST($1::uuid[], $2::uuid[], $5::numeric[]) AS v(a_id, o_id, score)",
        )
        .bind(&assess_ids)
        .bind(&obs_ids)
        .bind(model_id)
        .bind(now)
        .bind(&ivv_scores)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        if !red_seg_ids.is_empty() {
            sqlx::query(
                "UPDATE transfer_corridors tc
                 SET requires_revalidation = TRUE
                 FROM road_segments rs
                 WHERE tc.requires_revalidation = FALSE
                   AND rs.segment_id = ANY($1::uuid[])
                   AND (
                       tc.side_a_node_id = ANY($2::uuid[])
                       OR tc.side_b_node_id = ANY($2::uuid[])
                       OR ST_DWithin(tc.path_geom::geography, rs.geom::geography, 500)
                   )",
            )
            .bind(&red_seg_ids)
            .bind(&red_node_ids)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }

        let summary_id = Uuid::new_v4();
        append_sync_change(
            &mut tx,
            "IVV_ASSESSMENT",
            summary_id,
            "UPSERT",
            json!({
                "batch_source": "OPEN_METEO",
                "model_version_tag": model_params.version_tag,
                "updated_segments": seg_chunk.len(),
                "observed_at": now
            }),
        )
        .await?;
        tx.commit().await.map_err(|e| e.to_string())?;
    }

    Ok(stats)
}

/// Computes deterministic +24h/+48h/+72h predicted IVV risk levels from Open-Meteo precipitation forecasts.
pub async fn sync_weather_forecasts(
    pool: &PgPool,
    region: Option<&RegionSpec>,
) -> Result<ForecastSyncStats, String> {
    let (_model_id, model_params) = load_active_ivv_model(pool).await?;
    let (segments, cells_vec) = load_segments_and_grid_cells(pool, region).await?;
    if segments.is_empty() {
        return Ok(ForecastSyncStats::default());
    }

    let now = Utc::now();
    let raw_by_cell = query_grid_cells_weather(&cells_vec, "4").await?;
    let horizons: [i16; 3] = [24, 48, 72];
    let mut precip_by_cell_horizon: HashMap<((i32, i32), i16), PrecipTotals> =
        HashMap::with_capacity(raw_by_cell.len() * 3);

    for (cell, item) in raw_by_cell {
        for horizon in horizons {
            let target_end = now + chrono::Duration::hours(i64::from(horizon));
            let totals = accumulate_trailing_precipitation(
                &item.hourly.time,
                &item.hourly.precipitation,
                target_end,
            );
            precip_by_cell_horizon.insert((cell, horizon), totals);
        }
    }

    let mut stats = ForecastSyncStats {
        grid_cells_queried: cells_vec.len(),
        forecast_rows_upserted: 0,
    };

    for seg_chunk in segments.chunks(250) {
        let mut seg_ids = Vec::with_capacity(seg_chunk.len() * 3);
        let mut horizon_vals = Vec::with_capacity(seg_chunk.len() * 3);
        let mut p24_vals = Vec::with_capacity(seg_chunk.len() * 3);
        let mut p72_vals = Vec::with_capacity(seg_chunk.len() * 3);
        let mut scores = Vec::with_capacity(seg_chunk.len() * 3);
        let mut risks = Vec::with_capacity(seg_chunk.len() * 3);

        for seg in seg_chunk {
            for horizon in horizons {
                let totals = precip_by_cell_horizon
                    .get(&(seg.cell, horizon))
                    .copied()
                    .unwrap_or(PrecipTotals {
                        precip_24h_mm: Decimal::ZERO,
                        precip_72h_mm: Decimal::ZERO,
                    });
                let ivv_out = calculate_ivv(
                    &model_params,
                    &seg.geo,
                    &WeatherInput {
                        precip_24h_mm: totals.precip_24h_mm,
                        precip_72h_mm: totals.precip_72h_mm,
                        observed_at: now,
                    },
                    Some(now),
                    Some(now),
                )
                .map_err(|e| e.to_string())?;

                let risk_str = match ivv_out.risk_level {
                    RiskLevel::Green | RiskLevel::Unknown => "GREEN",
                    RiskLevel::Amber => "AMBER",
                    RiskLevel::Red => "RED",
                };

                seg_ids.push(seg.segment_id);
                horizon_vals.push(horizon);
                p24_vals.push(totals.precip_24h_mm);
                p72_vals.push(totals.precip_72h_mm);
                scores.push(ivv_out.ivv_score);
                risks.push(risk_str);
                stats.forecast_rows_upserted += 1;
            }
        }

        sqlx::query(
            "INSERT INTO weather_forecasts (
                segment_id, horizon_hours, precip_24h_mm, precip_72h_mm,
                predicted_ivv_score, predicted_risk_level, source_name, generated_at
             )
             SELECT v.seg_id, v.horizon, v.p24, v.p72, v.score, v.risk, 'OPEN_METEO_FORECAST', $7
             FROM UNNEST($1::uuid[], $2::int2[], $3::numeric[], $4::numeric[], $5::numeric[], $6::text[])
                AS v(seg_id, horizon, p24, p72, score, risk)
             ON CONFLICT (segment_id, horizon_hours) DO UPDATE SET
                precip_24h_mm = EXCLUDED.precip_24h_mm,
                precip_72h_mm = EXCLUDED.precip_72h_mm,
                predicted_ivv_score = EXCLUDED.predicted_ivv_score,
                predicted_risk_level = EXCLUDED.predicted_risk_level,
                generated_at = EXCLUDED.generated_at",
        )
        .bind(&seg_ids)
        .bind(&horizon_vals)
        .bind(&p24_vals)
        .bind(&p72_vals)
        .bind(&scores)
        .bind(&risks)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_open_meteo_single_and_batch_and_sums_windows() {
        let single_json = r#"{
            "hourly": {
                "time": ["2026-11-20T00:00", "2026-11-21T04:00", "2026-11-22T04:00", "2026-11-22T12:00"],
                "precipitation": [10.0, 15.5, 5.2, 99.0]
            }
        }"#;
        let parsed: OneOrMany<OpenMeteoForecastItem> = serde_json::from_str(single_json).unwrap();
        let items = parsed.into_vec();
        assert_eq!(items.len(), 1);

        let now = Utc.with_ymd_and_hms(2026, 11, 22, 6, 0, 0).unwrap();
        let totals = accumulate_trailing_precipitation(
            &items[0].hourly.time,
            &items[0].hourly.precipitation,
            now,
        );
        assert_eq!(totals.precip_24h_mm, Decimal::new(520, 2)); // 5.20 mm
        assert_eq!(totals.precip_72h_mm, Decimal::new(3070, 2)); // 30.70 mm (excludes future 99.0)
        assert!(totals.precip_72h_mm >= totals.precip_24h_mm);

        let forecast_24h = accumulate_trailing_precipitation(
            &items[0].hourly.time,
            &items[0].hourly.precipitation,
            now + chrono::Duration::hours(24),
        );
        assert_eq!(forecast_24h.precip_24h_mm, Decimal::new(9900, 2)); // 2026-11-22T12:00 only
        assert_eq!(forecast_24h.precip_72h_mm, Decimal::new(11970, 2)); // includes 99.0 + 5.2 + 15.5
    }
}
