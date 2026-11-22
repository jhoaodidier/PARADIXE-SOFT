#![forbid(unsafe_code)]

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{ApiError, ValidatedJson, ValidatedQuery},
    incidents::{
        process_field_report_item, process_telemetry_batch, BatchItemOutcome, OfflineReportInput,
        SyncBatchItemStatus, TelemetryIngestBody,
    },
    AppState,
};

/// Acquires the transactional singleton lock on `sync_control` (never sequences) and
/// appends an immutable change snapshot in the caller's database transaction.
pub async fn append_sync_change(
    tx: &mut Transaction<'_, Postgres>,
    entity_type: &str,
    entity_id: Uuid,
    operation: &str,
    payload_json: Option<Value>,
) -> Result<i64, ApiError> {
    let row = sqlx::query(
        "UPDATE sync_control
         SET last_seq = last_seq + 1
         WHERE singleton_id = 1
         RETURNING last_seq",
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::from_db)?;

    let change_seq: i64 = row.try_get("last_seq").map_err(ApiError::from_db)?;

    sqlx::query(
        "INSERT INTO sync_change_log (change_seq, entity_type, entity_id, operation, payload_json)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(change_seq)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(payload_json.map(sqlx::types::Json))
    .execute(&mut **tx)
    .await
    .map_err(ApiError::from_db)?;

    Ok(change_seq)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncSnapshotQuery {
    #[serde(default)]
    pub since_version: i64,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SyncChangeItemDto {
    pub change_seq: i64,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub operation: String,
    pub changed_at: DateTime<Utc>,
    pub payload_json: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct SyncSnapshotResponse {
    pub changes: Vec<SyncChangeItemDto>,
    pub next_since_version: i64,
    pub has_more: bool,
    pub server_time: DateTime<Utc>,
}

pub async fn get_sync_snapshot(
    State(state): State<AppState>,
    _auth: AuthUser,
    ValidatedQuery(query): ValidatedQuery<SyncSnapshotQuery>,
) -> Result<Json<SyncSnapshotResponse>, ApiError> {
    if query.since_version < 0 {
        return Err(ApiError::validation("since_version must be >= 0"));
    }
    let limit = query.limit.unwrap_or(500);
    if !(1..=1000).contains(&limit) {
        return Err(ApiError::validation("limit must be between 1 and 1000"));
    }

    let rows = sqlx::query(
        "SELECT change_seq, entity_type, entity_id, operation, changed_at, payload_json
         FROM sync_change_log
         WHERE change_seq > $1
         ORDER BY change_seq ASC
         LIMIT $2",
    )
    .bind(query.since_version)
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await
    .map_err(ApiError::from_db)?;

    let has_more = rows.len() as i64 > limit;
    let take_count = if has_more { limit as usize } else { rows.len() };
    let mut changes = Vec::with_capacity(take_count);

    for row in rows.into_iter().take(take_count) {
        let payload_json: Option<sqlx::types::Json<Value>> =
            row.try_get("payload_json").map_err(ApiError::from_db)?;
        changes.push(SyncChangeItemDto {
            change_seq: row.try_get("change_seq").map_err(ApiError::from_db)?,
            entity_type: row.try_get("entity_type").map_err(ApiError::from_db)?,
            entity_id: row.try_get("entity_id").map_err(ApiError::from_db)?,
            operation: row.try_get("operation").map_err(ApiError::from_db)?,
            changed_at: row.try_get("changed_at").map_err(ApiError::from_db)?,
            payload_json: payload_json.map(|j| j.0),
        });
    }

    let next_since_version = changes
        .last()
        .map(|item| item.change_seq)
        .unwrap_or(query.since_version);

    Ok(Json(SyncSnapshotResponse {
        changes,
        next_since_version,
        has_more,
        server_time: Utc::now(),
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncBatchRequest {
    pub reports: Vec<OfflineReportInput>,
    pub telemetry_batches: Vec<TelemetryIngestBody>,
}

#[derive(Debug, Serialize)]
pub struct SyncBatchResultEntry {
    pub client_id: Uuid,
    pub status: SyncBatchItemStatus,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SyncBatchResponse {
    pub report_results: Vec<SyncBatchResultEntry>,
    pub telemetry_results: Vec<SyncBatchResultEntry>,
    pub server_time: DateTime<Utc>,
}

pub async fn post_sync_batch(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<SyncBatchRequest>,
) -> Result<Json<SyncBatchResponse>, ApiError> {
    let now = Utc::now();
    let mut report_results = Vec::with_capacity(body.reports.len());
    for report in &body.reports {
        let outcome =
            process_field_report_item(&state.pool, auth.user_id, report, "MOBILE_APP", now).await?;
        let status = match outcome {
            BatchItemOutcome::Created(_) => SyncBatchItemStatus::Created,
            BatchItemOutcome::DuplicateIgnored => SyncBatchItemStatus::DuplicateIgnored,
            BatchItemOutcome::RejectedDistance => SyncBatchItemStatus::RejectedDistance,
            BatchItemOutcome::StaleArchived => SyncBatchItemStatus::StaleArchived,
        };
        report_results.push(SyncBatchResultEntry {
            client_id: report.client_uuid,
            status,
            server_time: now,
        });
    }

    let mut telemetry_results = Vec::new();
    for batch in &body.telemetry_batches {
        let ping_outcomes = process_telemetry_batch(&state, batch, now).await?;
        for (client_ping_uuid, status) in ping_outcomes {
            telemetry_results.push(SyncBatchResultEntry {
                client_id: client_ping_uuid,
                status,
                server_time: now,
            });
        }
    }

    Ok(Json(SyncBatchResponse {
        report_results,
        telemetry_results,
        server_time: now,
    }))
}
