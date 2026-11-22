#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use hmac::{Hmac, Mac};
use paradixe_consensus::{
    compute_daily_pseudonymous_hash, evaluate_field_consensus, evaluate_telemetry_cluster,
    FieldReportInput, TelemetryPingRecord,
};
use paradixe_domain::{ProposedPassability, VerificationState};
use rand::RngCore;
use rust_decimal::Decimal;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use sqlx::{Postgres, Row, Transaction};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    auth::{write_audit_log, AuthUser, RoleCode},
    error::{ApiError, ValidatedJson, ValidatedPath},
    sync::append_sync_change,
    AppState, NotificationChannel, NotificationEvent,
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct DailyTelemetryKeyStore {
    dir: PathBuf,
}

impl DailyTelemetryKeyStore {
    pub fn new(dir: PathBuf) -> Result<Self, ApiError> {
        fs::create_dir_all(&dir)
            .map_err(|e| ApiError::internal(format!("failed to create telemetry key dir: {e}")))?;
        Ok(Self { dir })
    }

    /// Retrieves or provisions the server-side daily binary secret on disk (`key_YYYY-MM-DD.bin`),
    /// recording only a symbolic `key_ref` in `telemetry_daily_salts` (never raw key material).
    pub async fn get_or_create_daily_key(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        date_utc: NaiveDate,
    ) -> Result<Vec<u8>, ApiError> {
        let file_name = format!("key_{date_utc}.bin");
        let path = self.dir.join(&file_name);
        let key_ref = format!("file:{file_name}");

        let key_bytes = if path.exists() {
            fs::read(&path)
                .map_err(|e| ApiError::internal(format!("failed to read daily key: {e}")))?
        } else {
            let mut key = vec![0u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            match opts.open(&path) {
                Ok(mut f) => {
                    f.write_all(&key).map_err(|e| {
                        ApiError::internal(format!("failed to write daily key: {e}"))
                    })?;
                    key
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => fs::read(&path)
                    .map_err(|err| {
                        ApiError::internal(format!("failed to read daily key: {err}"))
                    })?,
                Err(e) => {
                    return Err(ApiError::internal(format!(
                        "failed to provision daily key: {e}"
                    )));
                }
            }
        };

        sqlx::query(
            "INSERT INTO telemetry_daily_salts (salt_date_utc, key_ref)
             VALUES ($1, $2)
             ON CONFLICT (salt_date_utc) DO NOTHING",
        )
        .bind(date_utc)
        .bind(&key_ref)
        .execute(&mut **tx)
        .await
        .map_err(ApiError::from_db)?;

        Ok(key_bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncBatchItemStatus {
    Created,
    DuplicateIgnored,
    RejectedDistance,
    StaleArchived,
    ExpiredNoDetection,
    ValidationError,
}

pub enum BatchItemOutcome {
    Created(String),
    DuplicateIgnored,
    RejectedDistance,
    StaleArchived,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryPingInput {
    pub client_ping_uuid: Uuid,
    pub latitude: f64,
    pub longitude: f64,
    pub speed_kmh: Decimal,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryIngestBody {
    pub client_ephemeral_id: Uuid,
    pub ephemeral_id_date_utc: NaiveDate,
    pub pings: Vec<TelemetryPingInput>,
}

#[derive(Debug, Serialize)]
pub struct TelemetryIngestResponse {
    pub accepted: usize,
    pub expired: usize,
    pub duplicates: usize,
    pub server_time: DateTime<Utc>,
}

pub async fn process_telemetry_batch(
    state: &AppState,
    body: &TelemetryIngestBody,
    now: DateTime<Utc>,
) -> Result<Vec<(Uuid, SyncBatchItemStatus)>, ApiError> {
    if body.pings.is_empty() {
        return Err(ApiError::validation("pings must contain at least one item"));
    }
    for ping in &body.pings {
        if !(-90.0..=90.0).contains(&ping.latitude)
            || !(-180.0..=180.0).contains(&ping.longitude)
            || ping.speed_kmh < Decimal::ZERO
        {
            return Err(ApiError::validation("Invalid ping coordinates or speed"));
        }
        if ping.recorded_at.date_naive() != body.ephemeral_id_date_utc {
            return Err(ApiError::validation(
                "ephemeral_id_date_utc must match ping capture UTC date",
            ));
        }
    }

    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;
    let daily_key = state
        .telemetry_keys
        .get_or_create_daily_key(&mut tx, body.ephemeral_id_date_utc)
        .await?;

    // Compute pseudonymous HMAC immediately; client_ephemeral_id and user_id are NEVER persisted.
    let anonymous_hash = compute_daily_pseudonymous_hash(&daily_key, body.client_ephemeral_id);

    // Purge telemetry older than 24h on synced_at.
    sqlx::query("DELETE FROM telemetry_pings WHERE synced_at < $1")
        .bind(now - Duration::hours(24))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

    let mut outcomes = Vec::with_capacity(body.pings.len());
    let mut cluster_candidates: HashMap<Uuid, (f64, f64)> = HashMap::new();

    for ping in &body.pings {
        let existing = sqlx::query(
            "SELECT anonymous_device_hash, speed_kmh, recorded_at
             FROM telemetry_pings
             WHERE client_ping_uuid = $1",
        )
        .bind(ping.client_ping_uuid)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        if let Some(row) = existing {
            let ex_hash: String = row
                .try_get("anonymous_device_hash")
                .map_err(ApiError::from_db)?;
            let ex_speed: Decimal = row.try_get("speed_kmh").map_err(ApiError::from_db)?;
            let ex_rec: DateTime<Utc> = row.try_get("recorded_at").map_err(ApiError::from_db)?;
            if ex_hash == anonymous_hash
                && ex_speed == ping.speed_kmh
                && (ex_rec - ping.recorded_at).abs() <= Duration::seconds(1)
            {
                outcomes.push((ping.client_ping_uuid, SyncBatchItemStatus::DuplicateIgnored));
                continue;
            }
            return Err(ApiError::conflict(
                "IDEMPOTENCY_CONFLICT",
                "client_ping_uuid already exists with different identity or payload",
            ));
        }

        if now - ping.recorded_at > Duration::hours(24) {
            outcomes.push((
                ping.client_ping_uuid,
                SyncBatchItemStatus::ExpiredNoDetection,
            ));
            continue;
        }

        let urban_row = sqlx::query(
            "SELECT EXISTS (
                SELECT 1
                FROM road_nodes rn
                WHERE rn.urban_radius_m > 0
                  AND ST_DWithin(
                      rn.location_geom::geography,
                      ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography,
                      rn.urban_radius_m
                  )
            ) AS is_urban",
        )
        .bind(ping.longitude)
        .bind(ping.latitude)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;
        let is_urban: bool = urban_row.try_get("is_urban").map_err(ApiError::from_db)?;

        let matched = sqlx::query(
            "SELECT segment_id,
                    ST_Distance(
                        geom::geography,
                        ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography
                    ) AS dist_m
             FROM road_segments
             WHERE is_topology_verified = TRUE
               AND ST_DWithin(
                   geom::geography,
                   ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography,
                   50
               )
             ORDER BY dist_m ASC
             LIMIT 2",
        )
        .bind(ping.longitude)
        .bind(ping.latitude)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        let is_ambiguous = if matched.len() >= 2 {
            let d0: f64 = matched[0].try_get("dist_m").map_err(ApiError::from_db)?;
            let d1: f64 = matched[1].try_get("dist_m").map_err(ApiError::from_db)?;
            (d1 - d0).abs() < 10.0
        } else {
            false
        };

        if matched.is_empty() {
            outcomes.push((ping.client_ping_uuid, SyncBatchItemStatus::ValidationError));
            continue;
        }

        let segment_id: Uuid = matched[0]
            .try_get("segment_id")
            .map_err(ApiError::from_db)?;

        if !is_urban && !is_ambiguous {
            sqlx::query(
                "INSERT INTO telemetry_pings (
                    client_ping_uuid, anonymous_device_hash, segment_id, location_geom, speed_kmh, recorded_at, synced_at
                 ) VALUES (
                    $1, $2, $3, ST_SetSRID(ST_MakePoint($4, $5), 4326), $6, $7, $8
                 )",
            )
            .bind(ping.client_ping_uuid)
            .bind(&anonymous_hash)
            .bind(segment_id)
            .bind(ping.longitude)
            .bind(ping.latitude)
            .bind(ping.speed_kmh)
            .bind(ping.recorded_at)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;

            if now - ping.recorded_at <= Duration::minutes(15) {
                cluster_candidates
                    .entry(segment_id)
                    .or_insert((ping.latitude, ping.longitude));
            }
        }

        outcomes.push((ping.client_ping_uuid, SyncBatchItemStatus::Created));
    }

    for (segment_id, (lat, lon)) in cluster_candidates {
        let open_exists = sqlx::query(
            "SELECT EXISTS (
                SELECT 1 FROM incidents
                WHERE segment_id = $1 AND verification_state IN ('SUSPECTED', 'CONFIRMED')
            ) AS open_exists",
        )
        .bind(segment_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;
        let already_open: bool = open_exists
            .try_get("open_exists")
            .map_err(ApiError::from_db)?;
        if already_open {
            continue;
        }

        let ping_rows = sqlx::query(
            "SELECT anonymous_device_hash, segment_id, speed_kmh, recorded_at
             FROM telemetry_pings
             WHERE segment_id = $1 AND recorded_at >= $2 AND recorded_at <= $3",
        )
        .bind(segment_id)
        .bind(now - Duration::minutes(15))
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        let records: Vec<TelemetryPingRecord> = ping_rows
            .into_iter()
            .map(|r| {
                Ok(TelemetryPingRecord {
                    anonymous_device_hash: r
                        .try_get("anonymous_device_hash")
                        .map_err(ApiError::from_db)?,
                    segment_id: r.try_get("segment_id").map_err(ApiError::from_db)?,
                    speed_kmh: r.try_get("speed_kmh").map_err(ApiError::from_db)?,
                    recorded_at: r.try_get("recorded_at").map_err(ApiError::from_db)?,
                    is_urban_excluded: false,
                    is_ambiguous_parallel: false,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;

        let eval = evaluate_telemetry_cluster(segment_id, &records, now);
        if eval.triggered_suspected {
            let incident_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO incidents (
                    incident_id, segment_id, trigger_source, verification_state, impact_passability,
                    hazard_location_geom, hazard_radius_m, detected_at
                 ) VALUES (
                    $1, $2, 'TELEMETRY_CLUSTER', 'SUSPECTED', 'CAUTION_SUSPECTED',
                    ST_SetSRID(ST_MakePoint($3, $4), 4326), 150, $5
                 )",
            )
            .bind(incident_id)
            .bind(segment_id)
            .bind(lon)
            .bind(lat)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;

            append_sync_change(
                &mut tx,
                "INCIDENT",
                incident_id,
                "UPSERT",
                Some(json!({
                    "incident_id": incident_id,
                    "segment_id": segment_id,
                    "verification_state": "SUSPECTED",
                    "impact_passability": "CAUTION_SUSPECTED",
                    "detected_at": now
                })),
            )
            .await?;
        }
    }

    tx.commit().await.map_err(ApiError::from_db)?;
    Ok(outcomes)
}

pub async fn ingest_telemetry_pings(
    State(state): State<AppState>,
    _auth: AuthUser,
    ValidatedJson(body): ValidatedJson<TelemetryIngestBody>,
) -> Result<(StatusCode, Json<TelemetryIngestResponse>), ApiError> {
    let now = Utc::now();
    let outcomes = process_telemetry_batch(&state, &body, now).await?;
    let accepted = outcomes
        .iter()
        .filter(|(_, s)| *s == SyncBatchItemStatus::Created)
        .count();
    let expired = outcomes
        .iter()
        .filter(|(_, s)| *s == SyncBatchItemStatus::ExpiredNoDetection)
        .count();
    let duplicates = outcomes
        .iter()
        .filter(|(_, s)| *s == SyncBatchItemStatus::DuplicateIgnored)
        .count();

    Ok((
        StatusCode::ACCEPTED,
        Json(TelemetryIngestResponse {
            accepted,
            expired,
            duplicates,
            server_time: now,
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineReportInput {
    pub client_uuid: Uuid,
    pub segment_id: Uuid,
    pub proposed_passability: ProposedPassability,
    pub latitude: f64,
    pub longitude: f64,
    pub captured_at_client: DateTime<Utc>,
    pub photo_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldReportCreateRequest {
    pub client_uuid: Uuid,
    pub segment_id: Uuid,
    pub proposed_passability: ProposedPassability,
    pub latitude: f64,
    pub longitude: f64,
    pub captured_at_client: DateTime<Utc>,
    pub photo_uri: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FieldReportResponse {
    pub report_id: Uuid,
    pub validation_status: String,
    pub clock_skew_flagged: bool,
    pub synced_at_server: DateTime<Utc>,
}

fn proposed_passability_str(p: ProposedPassability) -> &'static str {
    match p {
        ProposedPassability::Blocked => "BLOCKED",
        ProposedPassability::PartialOneLane => "PARTIAL_ONE_LANE",
        ProposedPassability::Open => "OPEN",
    }
}

fn parse_proposed_passability(raw: &str) -> Result<ProposedPassability, ApiError> {
    match raw {
        "BLOCKED" => Ok(ProposedPassability::Blocked),
        "PARTIAL_ONE_LANE" => Ok(ProposedPassability::PartialOneLane),
        "OPEN" => Ok(ProposedPassability::Open),
        _ => Err(ApiError::internal("Invalid proposed_passability in DB")),
    }
}

pub async fn process_field_report_item(
    pool: &sqlx::PgPool,
    reporter_user_id: Uuid,
    input: &OfflineReportInput,
    channel: &str,
    now: DateTime<Utc>,
) -> Result<BatchItemOutcome, ApiError> {
    if !(-90.0..=90.0).contains(&input.latitude) || !(-180.0..=180.0).contains(&input.longitude) {
        return Err(ApiError::validation("Invalid latitude or longitude"));
    }
    if !matches!(channel, "MOBILE_APP" | "WHATSAPP_BOT" | "SMS") {
        return Err(ApiError::validation("Unsupported report channel"));
    }

    let pass_str = proposed_passability_str(input.proposed_passability);
    let mut tx = pool.begin().await.map_err(ApiError::from_db)?;

    if let Some(existing) = sqlx::query(
        "SELECT reporter_user_id, segment_id, proposed_passability
         FROM field_reports
         WHERE client_uuid = $1",
    )
    .bind(input.client_uuid)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    {
        let ex_user: Uuid = existing
            .try_get("reporter_user_id")
            .map_err(ApiError::from_db)?;
        let ex_seg: Uuid = existing.try_get("segment_id").map_err(ApiError::from_db)?;
        let ex_pass: String = existing
            .try_get("proposed_passability")
            .map_err(ApiError::from_db)?;

        if ex_user == reporter_user_id && ex_seg == input.segment_id && ex_pass == pass_str {
            return Ok(BatchItemOutcome::DuplicateIgnored);
        }
        return Err(ApiError::conflict(
            "IDEMPOTENCY_CONFLICT",
            "client_uuid already registered for different payload or user",
        ));
    }

    let dist_row = sqlx::query(
        "SELECT ST_DWithin(
                    geom::geography,
                    ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography,
                    500
                ) AS within_500m
         FROM road_segments
         WHERE segment_id = $3",
    )
    .bind(input.longitude)
    .bind(input.latitude)
    .bind(input.segment_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::validation("Unknown segment_id"))?;

    let within_500m: bool = dist_row.try_get("within_500m").map_err(ApiError::from_db)?;
    let clock_skew_flagged = input.captured_at_client > now + Duration::minutes(5);
    let is_stale_capture = now - input.captured_at_client > Duration::minutes(30);

    let validation_status = if is_stale_capture {
        "STALE_ARCHIVED"
    } else if !within_500m {
        "REJECTED_DISTANCE"
    } else {
        "PENDING"
    };

    let report_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO field_reports (
            report_id, client_uuid, reporter_user_id, segment_id, proposed_passability,
            location_geom, photo_uri, channel, validation_status, captured_at_client,
            clock_skew_flagged, synced_at_server
         ) VALUES (
            $1, $2, $3, $4, $5,
            ST_SetSRID(ST_MakePoint($6, $7), 4326), $8, $9, $10, $11,
            $12, $13
         )",
    )
    .bind(report_id)
    .bind(input.client_uuid)
    .bind(reporter_user_id)
    .bind(input.segment_id)
    .bind(pass_str)
    .bind(input.longitude)
    .bind(input.latitude)
    .bind(&input.photo_uri)
    .bind(channel)
    .bind(validation_status)
    .bind(input.captured_at_client)
    .bind(clock_skew_flagged)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    if validation_status == "PENDING" {
        let window_rows = sqlx::query(
            "SELECT fr.report_id, fr.reporter_user_id, fr.proposed_passability,
                    fr.captured_at_client, r.base_trust_weight, u.reputation_score
             FROM field_reports fr
             JOIN users u ON u.user_id = fr.reporter_user_id
             JOIN roles r ON r.role_id = u.role_id
             WHERE fr.segment_id = $1
               AND fr.validation_status IN ('PENDING', 'VERIFIED')
               AND fr.captured_at_client >= $2
               AND fr.captured_at_client <= $3",
        )
        .bind(input.segment_id)
        .bind(now - Duration::minutes(30))
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        let mut consensus_inputs = Vec::with_capacity(window_rows.len());
        let mut report_refs = Vec::with_capacity(window_rows.len());
        for row in window_rows {
            let r_id: Uuid = row.try_get("report_id").map_err(ApiError::from_db)?;
            let r_pass_str: String = row
                .try_get("proposed_passability")
                .map_err(ApiError::from_db)?;
            let r_pass = parse_proposed_passability(&r_pass_str)?;
            consensus_inputs.push(FieldReportInput {
                reporter_user_id: row.try_get("reporter_user_id").map_err(ApiError::from_db)?,
                proposed_passability: r_pass,
                base_trust_weight: row
                    .try_get("base_trust_weight")
                    .map_err(ApiError::from_db)?,
                reputation_score: row.try_get("reputation_score").map_err(ApiError::from_db)?,
                captured_at_client: row
                    .try_get("captured_at_client")
                    .map_err(ApiError::from_db)?,
                within_500m: true,
            });
            report_refs.push((r_id, r_pass));
        }

        let outcome = evaluate_field_consensus(&consensus_inputs, now);
        let existing_incident = sqlx::query(
            "SELECT incident_id, verification_state, impact_passability
             FROM incidents
             WHERE segment_id = $1 AND verification_state IN ('SUSPECTED', 'CONFIRMED')
             ORDER BY detected_at DESC
             LIMIT 1",
        )
        .bind(input.segment_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::from_db)?;

        if outcome.reached_community_consensus {
            if let Some(winner) = outcome.winning_passability {
                if matches!(
                    winner,
                    ProposedPassability::Blocked | ProposedPassability::PartialOneLane
                ) {
                    let impact_pass = proposed_passability_str(winner);
                    let incident_id = if let Some(inc) = existing_incident {
                        let inc_id: Uuid = inc.try_get("incident_id").map_err(ApiError::from_db)?;
                        sqlx::query(
                            "UPDATE incidents
                             SET verification_state = 'CONFIRMED',
                                 impact_passability = $2,
                                 confirmed_at = COALESCE(confirmed_at, $3),
                                 resolved_at = NULL
                             WHERE incident_id = $1",
                        )
                        .bind(inc_id)
                        .bind(impact_pass)
                        .bind(now)
                        .execute(&mut *tx)
                        .await
                        .map_err(ApiError::from_db)?;
                        inc_id
                    } else {
                        let inc_id = Uuid::new_v4();
                        sqlx::query(
                            "INSERT INTO incidents (
                                incident_id, segment_id, trigger_source, verification_state, impact_passability,
                                hazard_location_geom, hazard_radius_m, detected_at, confirmed_at
                             ) VALUES (
                                $1, $2, 'FIELD_CONSENSUS', 'CONFIRMED', $3,
                                ST_SetSRID(ST_MakePoint($4, $5), 4326), 150, $6, $6
                             )",
                        )
                        .bind(inc_id)
                        .bind(input.segment_id)
                        .bind(impact_pass)
                        .bind(input.longitude)
                        .bind(input.latitude)
                        .bind(now)
                        .execute(&mut *tx)
                        .await
                        .map_err(ApiError::from_db)?;
                        inc_id
                    };

                    for (r_id, r_pass) in &report_refs {
                        sqlx::query(
                            "INSERT INTO incident_report_links (incident_id, report_id)
                             VALUES ($1, $2)
                             ON CONFLICT DO NOTHING",
                        )
                        .bind(incident_id)
                        .bind(r_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(ApiError::from_db)?;

                        let new_status = if *r_pass == winner {
                            "VERIFIED"
                        } else {
                            "REJECTED_CONTRADICTED"
                        };
                        sqlx::query(
                            "UPDATE field_reports SET validation_status = $2 WHERE report_id = $1",
                        )
                        .bind(r_id)
                        .bind(new_status)
                        .execute(&mut *tx)
                        .await
                        .map_err(ApiError::from_db)?;
                    }

                    append_sync_change(
                        &mut tx,
                        "INCIDENT",
                        incident_id,
                        "UPSERT",
                        Some(json!({
                            "incident_id": incident_id,
                            "segment_id": input.segment_id,
                            "verification_state": "CONFIRMED",
                            "impact_passability": impact_pass,
                            "confirmed_at": now
                        })),
                    )
                    .await?;
                }
            }
        } else if input.proposed_passability != ProposedPassability::Open {
            let incident_id = if let Some(inc) = existing_incident {
                inc.try_get("incident_id").map_err(ApiError::from_db)?
            } else {
                let inc_id = Uuid::new_v4();
                sqlx::query(
                    "INSERT INTO incidents (
                        incident_id, segment_id, trigger_source, verification_state, impact_passability,
                        hazard_location_geom, hazard_radius_m, detected_at
                     ) VALUES (
                        $1, $2, 'FIELD_CONSENSUS', 'SUSPECTED', 'CAUTION_SUSPECTED',
                        ST_SetSRID(ST_MakePoint($3, $4), 4326), 150, $5
                     )",
                )
                .bind(inc_id)
                .bind(input.segment_id)
                .bind(input.longitude)
                .bind(input.latitude)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::from_db)?;

                append_sync_change(
                    &mut tx,
                    "INCIDENT",
                    inc_id,
                    "UPSERT",
                    Some(json!({
                        "incident_id": inc_id,
                        "segment_id": input.segment_id,
                        "verification_state": "SUSPECTED",
                        "impact_passability": "CAUTION_SUSPECTED",
                        "detected_at": now
                    })),
                )
                .await?;
                inc_id
            };

            sqlx::query(
                "INSERT INTO incident_report_links (incident_id, report_id)
                 VALUES ($1, $2)
                 ON CONFLICT DO NOTHING",
            )
            .bind(incident_id)
            .bind(report_id)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from_db)?;
        }
    }

    tx.commit().await.map_err(ApiError::from_db)?;

    Ok(match validation_status {
        "STALE_ARCHIVED" => BatchItemOutcome::StaleArchived,
        "REJECTED_DISTANCE" => BatchItemOutcome::RejectedDistance,
        status => BatchItemOutcome::Created(status.to_string()),
    })
}

pub async fn create_field_report(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<FieldReportCreateRequest>,
) -> Result<(StatusCode, Json<FieldReportResponse>), ApiError> {
    let now = Utc::now();
    let channel = body.channel.as_deref().unwrap_or("MOBILE_APP");
    let offline_item = OfflineReportInput {
        client_uuid: body.client_uuid,
        segment_id: body.segment_id,
        proposed_passability: body.proposed_passability,
        latitude: body.latitude,
        longitude: body.longitude,
        captured_at_client: body.captured_at_client,
        photo_uri: body.photo_uri,
    };
    process_field_report_item(&state.pool, auth.user_id, &offline_item, channel, now).await?;

    let row = sqlx::query(
        "SELECT report_id, validation_status, clock_skew_flagged, synced_at_server
         FROM field_reports
         WHERE client_uuid = $1",
    )
    .bind(body.client_uuid)
    .fetch_one(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    Ok((
        StatusCode::CREATED,
        Json(FieldReportResponse {
            report_id: row.try_get("report_id").map_err(ApiError::from_db)?,
            validation_status: row
                .try_get("validation_status")
                .map_err(ApiError::from_db)?,
            clock_skew_flagged: row
                .try_get("clock_skew_flagged")
                .map_err(ApiError::from_db)?,
            synced_at_server: row.try_get("synced_at_server").map_err(ApiError::from_db)?,
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessagingWebhookRequest {
    pub channel: String,
    pub sender_phone_e164: String,
    pub segment_id: Uuid,
    pub proposed_passability: ProposedPassability,
    pub latitude: f64,
    pub longitude: f64,
    pub timestamp_utc: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MessagingWebhookResponse {
    pub accepted: bool,
    pub reply_preview: String,
}

pub fn verify_webhook_hmac(secret: &[u8], raw_body: &[u8], signature_header: &str) -> bool {
    let hex_part = signature_header
        .strip_prefix("sha256=")
        .unwrap_or(signature_header)
        .trim();
    let Ok(provided) = hex::decode(hex_part) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(raw_body);
    let expected = mac.finalize().into_bytes();
    if provided.len() != expected.len() {
        return false;
    }
    bool::from(provided.ct_eq(&expected[..]))
}

pub async fn receive_messaging_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<MessagingWebhookResponse>, ApiError> {
    let sig_header = headers
        .get("X-Webhook-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("Missing X-Webhook-Signature"))?;

    if !verify_webhook_hmac(
        state.config.webhook_secret.expose_secret().as_bytes(),
        &raw_body,
        sig_header,
    ) {
        return Err(ApiError::unauthorized("Invalid webhook HMAC signature"));
    }

    let body: MessagingWebhookRequest = serde_json::from_slice(&raw_body)
        .map_err(|e| ApiError::validation(format!("Invalid webhook JSON payload: {e}")))?;

    if !matches!(body.channel.as_str(), "WHATSAPP_BOT" | "SMS") {
        return Err(ApiError::validation(
            "Webhook channel must be WHATSAPP_BOT or SMS",
        ));
    }

    let now = Utc::now();
    if (now - body.timestamp_utc).abs() > Duration::minutes(5) {
        return Err(ApiError::unauthorized(
            "Webhook timestamp outside 5-minute anti-replay window",
        ));
    }

    let reporter_id: Uuid = sqlx::query(
        "SELECT u.user_id
         FROM users u
         WHERE u.phone_e164 = $1 AND u.is_active = TRUE
         LIMIT 1",
    )
    .bind(body.sender_phone_e164.trim())
    .fetch_optional(&state.pool)
    .await
    .map_err(ApiError::from_db)?
    .map(|r| r.try_get("user_id"))
    .transpose()
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::unauthorized("Unregistered messaging sender"))?;

    let offline_item = OfflineReportInput {
        client_uuid: Uuid::new_v4(),
        segment_id: body.segment_id,
        proposed_passability: body.proposed_passability,
        latitude: body.latitude,
        longitude: body.longitude,
        captured_at_client: body.timestamp_utc,
        photo_uri: None,
    };

    let outcome =
        process_field_report_item(&state.pool, reporter_id, &offline_item, &body.channel, now)
            .await?;

    let accepted = !matches!(
        outcome,
        BatchItemOutcome::RejectedDistance | BatchItemOutcome::StaleArchived
    );
    let reply_preview = state
        .gateway
        .format_and_dispatch(NotificationEvent {
            channel: if body.channel == "SMS" {
                NotificationChannel::Sms
            } else {
                NotificationChannel::WhatsappBot
            },
            message: if accepted {
                format!(
                    "PARADIXE SOFT: reporte recibido para tramo {} ({}).",
                    body.segment_id,
                    proposed_passability_str(body.proposed_passability)
                )
            } else {
                "PARADIXE SOFT: reporte fuera de radio (500 m) o vencido.".to_string()
            },
        })
        .await;

    Ok(Json(MessagingWebhookResponse {
        accepted,
        reply_preview,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentVerificationRequest {
    pub verification_state: VerificationState,
    pub impact_passability: String,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IncidentResponse {
    pub incident_id: Uuid,
    pub segment_id: Uuid,
    pub verification_state: VerificationState,
    pub impact_passability: String,
    pub detected_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}

pub async fn verify_incident(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedPath(incident_id): ValidatedPath<Uuid>,
    ValidatedJson(body): ValidatedJson<IncidentVerificationRequest>,
) -> Result<Json<IncidentResponse>, ApiError> {
    auth.require_roles(&[RoleCode::CoerAdmin, RoleCode::CivilDefense])?;

    if !matches!(
        body.impact_passability.as_str(),
        "CAUTION_SUSPECTED" | "PARTIAL_ONE_LANE" | "BLOCKED"
    ) {
        return Err(ApiError::validation("Invalid impact_passability"));
    }

    let state_str = match body.verification_state {
        VerificationState::Suspected => "SUSPECTED",
        VerificationState::Confirmed => "CONFIRMED",
        VerificationState::Dismissed => "DISMISSED",
        VerificationState::Resolved => "RESOLVED",
    };

    let now = Utc::now();
    let mut tx = state.pool.begin().await.map_err(ApiError::from_db)?;

    let before_row = sqlx::query(
        "SELECT segment_id, verification_state, impact_passability, detected_at, confirmed_at, resolved_at
         FROM incidents
         WHERE incident_id = $1
         FOR UPDATE",
    )
    .bind(incident_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::from_db)?
    .ok_or_else(|| ApiError::not_found("Incident not found"))?;

    let prev_state: String = before_row
        .try_get("verification_state")
        .map_err(ApiError::from_db)?;
    let prev_pass: String = before_row
        .try_get("impact_passability")
        .map_err(ApiError::from_db)?;
    let prev_confirmed: Option<DateTime<Utc>> = before_row
        .try_get("confirmed_at")
        .map_err(ApiError::from_db)?;

    let new_confirmed = match body.verification_state {
        VerificationState::Confirmed => Some(prev_confirmed.unwrap_or(now)),
        VerificationState::Suspected => prev_confirmed,
        VerificationState::Resolved | VerificationState::Dismissed => prev_confirmed,
    };
    let new_resolved = match body.verification_state {
        VerificationState::Resolved | VerificationState::Dismissed => Some(now),
        VerificationState::Suspected | VerificationState::Confirmed => None,
    };

    let updated = sqlx::query(
        "UPDATE incidents
         SET verification_state = $2,
             impact_passability = $3,
             confirmed_at = $4,
             resolved_at = $5
         WHERE incident_id = $1
         RETURNING incident_id, segment_id, verification_state, impact_passability, detected_at, confirmed_at, resolved_at",
    )
    .bind(incident_id)
    .bind(state_str)
    .bind(&body.impact_passability)
    .bind(new_confirmed)
    .bind(new_resolved)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::from_db)?;

    let segment_id: Uuid = updated.try_get("segment_id").map_err(ApiError::from_db)?;

    append_sync_change(
        &mut tx,
        "INCIDENT",
        incident_id,
        "UPSERT",
        Some(json!({
            "incident_id": incident_id,
            "segment_id": segment_id,
            "verification_state": state_str,
            "impact_passability": body.impact_passability,
            "confirmed_at": new_confirmed,
            "resolved_at": new_resolved
        })),
    )
    .await?;

    write_audit_log(
        &mut tx,
        Some(auth.user_id),
        "INCIDENT_VERIFICATION_UPDATED",
        "INCIDENT",
        incident_id,
        Some(json!({
            "verification_state": prev_state,
            "impact_passability": prev_pass
        })),
        json!({
            "verification_state": state_str,
            "impact_passability": body.impact_passability,
            "notes": body.notes
        }),
    )
    .await?;

    tx.commit().await.map_err(ApiError::from_db)?;

    Ok(Json(IncidentResponse {
        incident_id,
        segment_id,
        verification_state: body.verification_state,
        impact_passability: body.impact_passability,
        detected_at: updated.try_get("detected_at").map_err(ApiError::from_db)?,
        confirmed_at: new_confirmed,
        resolved_at: new_resolved,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_04_webhook_hmac_verification_is_strict() {
        let secret = b"webhook_hmac_test_secret_123456";
        let payload = br#"{"channel":"WHATSAPP_BOT","test":1}"#;
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(payload);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_webhook_hmac(secret, payload, &sig));
        assert!(!verify_webhook_hmac(secret, b"{}", &sig));
        assert!(!verify_webhook_hmac(b"wrong_secret_1234", payload, &sig));
    }
}
