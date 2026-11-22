#![forbid(unsafe_code)]

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use paradixe_domain::{round_half_away_from_zero, ProposedPassability, VerificationState};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Computes the daily pseudonymous HMAC-SHA256 hash of `client_ephemeral_id`
/// using the server-side daily binary secret (never persisted alongside pings).
pub fn compute_daily_pseudonymous_hash(
    secret_key_bytes: &[u8],
    client_ephemeral_id: Uuid,
) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret_key_bytes).expect("HMAC accepts keys of any size");
    mac.update(client_ephemeral_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryPingRecord {
    pub anonymous_device_hash: String,
    pub segment_id: Uuid,
    pub speed_kmh: Decimal,
    pub recorded_at: DateTime<Utc>,
    #[serde(default)]
    pub is_urban_excluded: bool,
    #[serde(default)]
    pub is_ambiguous_parallel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryClusterResult {
    pub qualifying_devices: usize,
    pub triggered_suspected: bool,
    pub outcome_state: Option<VerificationState>,
}

/// Evaluates whether pseudonymous stationary GPS pings on `segment_id` cross the
/// threshold for creating a `SUSPECTED` incident (telemetry NEVER confirms closure).
pub fn evaluate_telemetry_cluster(
    segment_id: Uuid,
    pings: &[TelemetryPingRecord],
    evaluation_time: DateTime<Utc>,
) -> TelemetryClusterResult {
    let window_start = evaluation_time - Duration::minutes(15);
    let max_speed = Decimal::new(30, 1); // 3.0 km/h
    let mut by_device: HashMap<&str, Vec<DateTime<Utc>>> = HashMap::new();

    for p in pings {
        if p.segment_id != segment_id
            || p.is_urban_excluded
            || p.is_ambiguous_parallel
            || p.speed_kmh > max_speed
            || p.recorded_at < window_start
            || p.recorded_at > evaluation_time
        {
            continue;
        }
        by_device
            .entry(p.anonymous_device_hash.as_str())
            .or_default()
            .push(p.recorded_at);
    }

    let mut qualifying_devices = 0usize;
    for timestamps in by_device.values_mut() {
        timestamps.sort_unstable();
        timestamps.dedup();
        if timestamps.len() < 3 {
            continue;
        }
        let consecutive_ok = timestamps
            .windows(2)
            .all(|w| w[1] - w[0] <= Duration::minutes(5));
        let span_ok = timestamps[timestamps.len() - 1] - timestamps[0] >= Duration::minutes(10);
        if consecutive_ok && span_ok {
            qualifying_devices += 1;
        }
    }

    let triggered = qualifying_devices >= 10;
    TelemetryClusterResult {
        qualifying_devices,
        triggered_suspected: triggered,
        outcome_state: triggered.then_some(VerificationState::Suspected),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldReportInput {
    pub reporter_user_id: Uuid,
    pub proposed_passability: ProposedPassability,
    pub base_trust_weight: Decimal,
    pub reputation_score: Decimal,
    pub captured_at_client: DateTime<Utc>,
    pub within_500m: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsensusOutcome {
    pub unique_reporters: usize,
    pub winning_passability: Option<ProposedPassability>,
    pub winning_support: Decimal,
    pub opposing_support: Decimal,
    pub margin: Decimal,
    pub reached_community_consensus: bool,
    pub incident_state: VerificationState,
}

pub fn evaluate_field_consensus(
    reports: &[FieldReportInput],
    evaluation_time: DateTime<Utc>,
) -> ConsensusOutcome {
    let window_start = evaluation_time - Duration::minutes(30);
    let mut latest_by_user: HashMap<Uuid, &FieldReportInput> = HashMap::new();

    for r in reports {
        if !r.within_500m
            || r.captured_at_client < window_start
            || r.captured_at_client > evaluation_time
        {
            continue;
        }
        latest_by_user
            .entry(r.reporter_user_id)
            .and_modify(|existing| {
                if r.captured_at_client > existing.captured_at_client {
                    *existing = r;
                }
            })
            .or_insert(r);
    }

    let half = Decimal::new(5, 1);
    let mut scores: HashMap<ProposedPassability, (usize, Decimal)> = HashMap::new();
    let mut total_weight = Decimal::ZERO;

    for r in latest_by_user.values() {
        let w = r.base_trust_weight * (half + half * r.reputation_score);
        total_weight += w;
        let entry = scores
            .entry(r.proposed_passability)
            .or_insert((0, Decimal::ZERO));
        entry.0 += 1;
        entry.1 += w;
    }

    let min_support = Decimal::ONE;
    let min_margin = Decimal::new(5, 1);

    let candidates = [
        ProposedPassability::Blocked,
        ProposedPassability::PartialOneLane,
        ProposedPassability::Open,
    ];

    let mut best_winner: Option<(ProposedPassability, Decimal, Decimal, Decimal)> = None;
    for state in candidates {
        let (count, support) = scores.get(&state).copied().unwrap_or((0, Decimal::ZERO));
        let opposing = total_weight - support;
        let margin = support - opposing;
        if count >= 2 && support >= min_support && margin >= min_margin {
            let s_round = round_half_away_from_zero(support, 2);
            let o_round = round_half_away_from_zero(opposing, 2);
            let m_round = round_half_away_from_zero(margin, 2);
            best_winner = Some((state, s_round, o_round, m_round));
            break;
        }
    }

    if let Some((winning_passability, winning_support, opposing_support, margin)) = best_winner {
        ConsensusOutcome {
            unique_reporters: latest_by_user.len(),
            winning_passability: Some(winning_passability),
            winning_support,
            opposing_support,
            margin,
            reached_community_consensus: true,
            incident_state: VerificationState::Confirmed,
        }
    } else {
        ConsensusOutcome {
            unique_reporters: latest_by_user.len(),
            winning_passability: None,
            winning_support: round_half_away_from_zero(
                scores
                    .values()
                    .map(|(_, s)| *s)
                    .max()
                    .unwrap_or(Decimal::ZERO),
                2,
            ),
            opposing_support: Decimal::ZERO,
            margin: Decimal::ZERO,
            reached_community_consensus: false,
            incident_state: VerificationState::Suspected,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsensusCliInput {
    pub segment_id: Uuid,
    #[serde(default)]
    pub pings: Vec<TelemetryPingRecord>,
    #[serde(default)]
    pub reports: Vec<FieldReportInput>,
    pub evaluation_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusCliOutput {
    pub telemetry: TelemetryClusterResult,
    pub consensus: ConsensusOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn con_01_deduplicates_single_user_and_enforces_margin() {
        let now = DateTime::parse_from_rfc3339("2026-11-21T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();
        let u_oppose = Uuid::new_v4();

        // 1 usuario envia 5 reportes -> deduplicado a 1
        let mut reports: Vec<FieldReportInput> = (0..5)
            .map(|i| FieldReportInput {
                reporter_user_id: u1,
                proposed_passability: ProposedPassability::Blocked,
                base_trust_weight: Decimal::from_str("0.60").unwrap(),
                reputation_score: Decimal::from_str("0.50").unwrap(), // w = 0.60 * 0.75 = 0.45
                captured_at_client: now - Duration::minutes(10 - i),
                within_500m: true,
            })
            .collect();
        let out_single = evaluate_field_consensus(&reports, now);
        assert_eq!(out_single.unique_reporters, 1);
        assert!(!out_single.reached_community_consensus);
        assert_eq!(out_single.incident_state, VerificationState::Suspected);

        // 2 usuarios distintos BLOCKED (0.45 + 0.60 = 1.05) vs 1 opositor fuerte OPEN (0.90) -> margen 0.15 < 0.50
        reports.push(FieldReportInput {
            reporter_user_id: u2,
            proposed_passability: ProposedPassability::Blocked,
            base_trust_weight: Decimal::from_str("0.60").unwrap(),
            reputation_score: Decimal::ONE, // w = 0.60
            captured_at_client: now - Duration::minutes(4),
            within_500m: true,
        });
        reports.push(FieldReportInput {
            reporter_user_id: u_oppose,
            proposed_passability: ProposedPassability::Open,
            base_trust_weight: Decimal::ONE,
            reputation_score: Decimal::from_str("0.80").unwrap(), // w = 0.90
            captured_at_client: now - Duration::minutes(2),
            within_500m: true,
        });

        let out_contested = evaluate_field_consensus(&reports, now);
        assert!(!out_contested.reached_community_consensus);
        assert_eq!(out_contested.incident_state, VerificationState::Suspected);

        // Tercer informante BLOCKED rompe margen: 1.05 + 0.525 = 1.575 vs 0.90 -> margen 0.675 >= 0.50
        reports.push(FieldReportInput {
            reporter_user_id: Uuid::new_v4(),
            proposed_passability: ProposedPassability::Blocked,
            base_trust_weight: Decimal::from_str("0.70").unwrap(),
            reputation_score: Decimal::from_str("0.50").unwrap(), // w = 0.525
            captured_at_client: now - Duration::minutes(1),
            within_500m: true,
        });
        let out_confirmed = evaluate_field_consensus(&reports, now);
        assert!(out_confirmed.reached_community_consensus);
        assert_eq!(
            out_confirmed.winning_passability,
            Some(ProposedPassability::Blocked)
        );
    }

    #[test]
    fn telemetry_cluster_never_confirms_only_suspects() {
        let seg = Uuid::new_v4();
        let now = DateTime::parse_from_rfc3339("2026-11-21T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let secret = b"demo_daily_256bit_secret_key_for_test_only";

        let mut pings = Vec::new();
        for _ in 0..10 {
            let hash = compute_daily_pseudonymous_hash(secret, Uuid::new_v4());
            for m in [14, 9, 4] {
                pings.push(TelemetryPingRecord {
                    anonymous_device_hash: hash.clone(),
                    segment_id: seg,
                    speed_kmh: Decimal::from(1),
                    recorded_at: now - Duration::minutes(m),
                    is_urban_excluded: false,
                    is_ambiguous_parallel: false,
                });
            }
        }
        let res = evaluate_telemetry_cluster(seg, &pings, now);
        assert_eq!(res.qualifying_devices, 10);
        assert!(res.triggered_suspected);
        assert_eq!(res.outcome_state, Some(VerificationState::Suspected));
    }
}
