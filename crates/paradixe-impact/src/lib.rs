#![forbid(unsafe_code)]

use paradixe_domain::{round_half_away_from_zero, CargoType, MitigationStatus, RoutingResolution};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImpactError {
    #[error("baseline inputs must be valid: {0}")]
    InvalidInput(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactParameters {
    pub version_tag: String,
    pub co2_idle_kg_per_hour: Decimal,
    pub co2_drive_kg_per_km: Decimal,
    pub agri_cargo_value_usd: Decimal,
    pub perishability_loss_rate_per_hour: Decimal,
    pub transfer_handling_loss_pct: Decimal,
    pub spoilage_grace_hours: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactInput {
    pub params: ImpactParameters,
    pub cargo_type: CargoType,
    pub baseline_normal_travel_min: Decimal,
    pub baseline_wait_min: Decimal,
    pub baseline_distance_km: Decimal,
    pub resolution: RoutingResolution,
    #[serde(default)]
    pub bridge_leg_total_distance_km: Option<Decimal>,
    #[serde(default)]
    pub bridge_leg_total_travel_min: Option<Decimal>,
    #[serde(default)]
    pub mitigated_idle_min: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactBaselineMetrics {
    pub travel_min: Decimal,
    pub co2_kg: Decimal,
    pub cargo_loss_usd: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactMitigatedMetrics {
    pub travel_min: Option<Decimal>,
    pub co2_kg: Option<Decimal>,
    pub cargo_loss_usd: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactDeltaMetrics {
    pub time_delta_min: Option<Decimal>,
    pub time_saved_pct: Option<Decimal>,
    pub net_co2_delta_kg: Option<Decimal>,
    pub net_agri_savings_usd: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactOutput {
    pub param_version_tag: String,
    pub mitigation_status: MitigationStatus,
    pub baseline: ImpactBaselineMetrics,
    pub mitigated: ImpactMitigatedMetrics,
    pub deltas: ImpactDeltaMetrics,
}

fn compute_agri_loss(
    params: &ImpactParameters,
    cargo_type: CargoType,
    wait_min: Decimal,
    has_bridge_transfer: bool,
) -> Decimal {
    if cargo_type != CargoType::PerishableAgri {
        return Decimal::ZERO;
    }
    let wait_hours = wait_min / Decimal::from(60);
    let billable_hours = (wait_hours - params.spoilage_grace_hours).max(Decimal::ZERO);
    let handling_loss = if has_bridge_transfer {
        params.transfer_handling_loss_pct
    } else {
        Decimal::ZERO
    };
    let loss_fraction = (billable_hours * params.perishability_loss_rate_per_hour + handling_loss)
        .min(Decimal::ONE);
    round_half_away_from_zero(loss_fraction * params.agri_cargo_value_usd, 2)
}

pub fn evaluate_impact(input: &ImpactInput) -> Result<ImpactOutput, ImpactError> {
    if input.baseline_normal_travel_min <= Decimal::ZERO
        || input.baseline_wait_min < Decimal::ZERO
        || input.baseline_distance_km < Decimal::ZERO
    {
        return Err(ImpactError::InvalidInput(
            "baseline travel must be > 0 and wait/distance >= 0",
        ));
    }

    let sixty = Decimal::from(60);
    let baseline_travel_min = round_half_away_from_zero(
        input.baseline_normal_travel_min + input.baseline_wait_min,
        2,
    );
    let baseline_co2_kg = round_half_away_from_zero(
        (input.baseline_wait_min / sixty) * input.params.co2_idle_kg_per_hour
            + input.baseline_distance_km * input.params.co2_drive_kg_per_km,
        2,
    );
    let baseline_cargo_loss_usd = compute_agri_loss(
        &input.params,
        input.cargo_type,
        input.baseline_wait_min,
        false,
    );

    let baseline = ImpactBaselineMetrics {
        travel_min: baseline_travel_min,
        co2_kg: baseline_co2_kg,
        cargo_loss_usd: baseline_cargo_loss_usd,
    };

    match &input.resolution {
        RoutingResolution::NoSafeRoute { .. } => Ok(ImpactOutput {
            param_version_tag: input.params.version_tag.clone(),
            mitigation_status: MitigationStatus::UnmitigatedNoSafeRoute,
            baseline,
            mitigated: ImpactMitigatedMetrics {
                travel_min: None,
                co2_kg: None,
                cargo_loss_usd: None,
            },
            deltas: ImpactDeltaMetrics {
                time_delta_min: None,
                time_saved_pct: None,
                net_co2_delta_kg: None,
                net_agri_savings_usd: None,
            },
        }),
        RoutingResolution::AlternateRoute {
            total_distance_km,
            estimated_time_min,
            ..
        } => {
            let idle_min = input.mitigated_idle_min.unwrap_or(Decimal::ZERO);
            let mitigated_travel_min = round_half_away_from_zero(*estimated_time_min + idle_min, 2);
            let mitigated_co2_kg = round_half_away_from_zero(
                (idle_min / sixty) * input.params.co2_idle_kg_per_hour
                    + *total_distance_km * input.params.co2_drive_kg_per_km,
                2,
            );
            let mitigated_cargo_loss_usd =
                compute_agri_loss(&input.params, input.cargo_type, idle_min, false);

            let time_delta_min =
                round_half_away_from_zero(baseline_travel_min - mitigated_travel_min, 2);
            let time_saved_pct = round_half_away_from_zero(
                (time_delta_min / baseline_travel_min) * Decimal::from(100),
                2,
            );
            let net_co2_delta_kg = round_half_away_from_zero(baseline_co2_kg - mitigated_co2_kg, 2);
            let net_agri_savings_usd =
                round_half_away_from_zero(baseline_cargo_loss_usd - mitigated_cargo_loss_usd, 2);

            Ok(ImpactOutput {
                param_version_tag: input.params.version_tag.clone(),
                mitigation_status: MitigationStatus::Mitigated,
                baseline,
                mitigated: ImpactMitigatedMetrics {
                    travel_min: Some(mitigated_travel_min),
                    co2_kg: Some(mitigated_co2_kg),
                    cargo_loss_usd: Some(mitigated_cargo_loss_usd),
                },
                deltas: ImpactDeltaMetrics {
                    time_delta_min: Some(time_delta_min),
                    time_saved_pct: Some(time_saved_pct),
                    net_co2_delta_kg: Some(net_co2_delta_kg),
                    net_agri_savings_usd: Some(net_agri_savings_usd),
                },
            })
        }
        RoutingResolution::LogisticBridgeRequired {
            bridge_proposal, ..
        } => {
            let idle_min = input.mitigated_idle_min.unwrap_or(Decimal::ZERO);
            let drive_min = input.bridge_leg_total_travel_min.unwrap_or(Decimal::ZERO);
            let drive_km = input.bridge_leg_total_distance_km.unwrap_or(Decimal::ZERO);
            let transfer_min = bridge_proposal.estimated_transfer_time_min;

            let mitigated_travel_min =
                round_half_away_from_zero(drive_min + transfer_min + idle_min, 2);
            let mitigated_co2_kg = round_half_away_from_zero(
                (idle_min / sixty) * input.params.co2_idle_kg_per_hour
                    + drive_km * input.params.co2_drive_kg_per_km,
                2,
            );
            let mitigated_cargo_loss_usd = compute_agri_loss(
                &input.params,
                input.cargo_type,
                idle_min + transfer_min,
                true,
            );

            let time_delta_min =
                round_half_away_from_zero(baseline_travel_min - mitigated_travel_min, 2);
            let time_saved_pct = round_half_away_from_zero(
                (time_delta_min / baseline_travel_min) * Decimal::from(100),
                2,
            );
            let net_co2_delta_kg = round_half_away_from_zero(baseline_co2_kg - mitigated_co2_kg, 2);
            let net_agri_savings_usd =
                round_half_away_from_zero(baseline_cargo_loss_usd - mitigated_cargo_loss_usd, 2);

            Ok(ImpactOutput {
                param_version_tag: input.params.version_tag.clone(),
                mitigation_status: MitigationStatus::Mitigated,
                baseline,
                mitigated: ImpactMitigatedMetrics {
                    travel_min: Some(mitigated_travel_min),
                    co2_kg: Some(mitigated_co2_kg),
                    cargo_loss_usd: Some(mitigated_cargo_loss_usd),
                },
                deltas: ImpactDeltaMetrics {
                    time_delta_min: Some(time_delta_min),
                    time_saved_pct: Some(time_saved_pct),
                    net_co2_delta_kg: Some(net_co2_delta_kg),
                    net_agri_savings_usd: Some(net_agri_savings_usd),
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paradixe_domain::NoSafeRouteReasonCode;
    use std::str::FromStr;

    fn trailer_params() -> ImpactParameters {
        ImpactParameters {
            version_tag: "impact-v1.0-ref".to_string(),
            co2_idle_kg_per_hour: Decimal::from_str("3.500").unwrap(),
            co2_drive_kg_per_km: Decimal::from_str("1.350").unwrap(),
            agri_cargo_value_usd: Decimal::from_str("9000.00").unwrap(),
            perishability_loss_rate_per_hour: Decimal::from_str("0.0300").unwrap(),
            transfer_handling_loss_pct: Decimal::from_str("0.015").unwrap(),
            spoilage_grace_hours: Decimal::from_str("2.00").unwrap(),
        }
    }

    #[test]
    fn sim_01_imp_01_no_safe_route_yields_null_mitigated_and_deltas() {
        let out = evaluate_impact(&ImpactInput {
            params: trailer_params(),
            cargo_type: CargoType::PerishableAgri,
            baseline_normal_travel_min: Decimal::from(120),
            baseline_wait_min: Decimal::from(360),
            baseline_distance_km: Decimal::from(60),
            resolution: RoutingResolution::NoSafeRoute {
                blocking_incident_id: None,
                reason_code: NoSafeRouteReasonCode::StaleIvv,
                warnings: vec![],
            },
            bridge_leg_total_distance_km: None,
            bridge_leg_total_travel_min: None,
            mitigated_idle_min: None,
        })
        .unwrap();

        assert_eq!(
            out.mitigation_status,
            MitigationStatus::UnmitigatedNoSafeRoute
        );
        assert!(out.mitigated.travel_min.is_none());
        assert!(out.mitigated.co2_kg.is_none());
        assert!(out.mitigated.cargo_loss_usd.is_none());
        assert!(out.deltas.time_delta_min.is_none());
        assert!(out.deltas.net_co2_delta_kg.is_none());
    }

    #[test]
    fn sim_02_long_detour_preserves_negative_co2_delta() {
        let out = evaluate_impact(&ImpactInput {
            params: trailer_params(),
            cargo_type: CargoType::PerishableAgri,
            baseline_normal_travel_min: Decimal::from(60),
            baseline_wait_min: Decimal::from(60), // idle CO2 = 3.5 kg + drive 30*1.35 = 44.0 kg
            baseline_distance_km: Decimal::from(30),
            resolution: RoutingResolution::AlternateRoute {
                routing_config_tag: "routing-v1.0".to_string(),
                total_distance_km: Decimal::from(140), // drive CO2 = 189.0 kg > 44.0 kg
                estimated_time_min: Decimal::from(180),
                max_ivv_on_route: Decimal::from(25),
                hops: vec![],
                eta_quality: "NOMINAL_ESTIMATE".to_string(),
                operational_authorization: "REQUIRED".to_string(),
                warnings: vec![],
            },
            bridge_leg_total_distance_km: None,
            bridge_leg_total_travel_min: None,
            mitigated_idle_min: None,
        })
        .unwrap();

        assert_eq!(out.mitigation_status, MitigationStatus::Mitigated);
        assert!(out.deltas.net_co2_delta_kg.unwrap() < Decimal::ZERO);
    }
}
