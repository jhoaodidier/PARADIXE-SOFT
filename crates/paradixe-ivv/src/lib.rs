#![forbid(unsafe_code)]

use chrono::{DateTime, Duration, Utc};
use paradixe_domain::{round_half_away_from_zero, RiskLevel};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IvvError {
    #[error("IVV model weights must sum to 1.000 (got {0})")]
    InvalidWeightsSum(Decimal),
    #[error("IVV model caps and thresholds are invalid: {0}")]
    InvalidModelConfig(&'static str),
    #[error("weather observation is invalid: {0}")]
    InvalidObservation(&'static str),
    #[error("segment geotechnical attributes are invalid: {0}")]
    InvalidSegment(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IvvModelParams {
    pub version_tag: String,
    pub w_precip: Decimal,
    pub w_slope_ravine: Decimal,
    pub w_soil: Decimal,
    pub w_hotspot: Decimal,
    pub precip_24h_cap_mm: Decimal,
    pub precip_72h_cap_mm: Decimal,
    pub slope_cap_pct: Decimal,
    pub ravine_max_dist_m: Decimal,
    pub amber_threshold: Decimal,
    pub red_threshold: Decimal,
    pub validity_hours: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentGeoInput {
    pub max_slope_forward_pct: Decimal,
    #[serde(default)]
    pub max_slope_reverse_pct: Option<Decimal>,
    pub soil_susceptibility_idx: Decimal,
    pub distance_to_ravine_m: Decimal,
    pub is_historical_hotspot: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherInput {
    pub precip_24h_mm: Decimal,
    pub precip_72h_mm: Decimal,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IvvComponents {
    #[serde(rename = "P")]
    pub p: Decimal,
    #[serde(rename = "S")]
    pub s: Decimal,
    #[serde(rename = "G")]
    pub g: Decimal,
    #[serde(rename = "H")]
    pub h: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IvvOutput {
    pub model_version_tag: String,
    pub ivv_score: Decimal,
    pub risk_level: RiskLevel,
    pub is_stale: bool,
    pub components: IvvComponents,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IvvCliInput {
    pub model: IvvModelParams,
    pub segment: SegmentGeoInput,
    pub observation: WeatherInput,
    #[serde(default)]
    pub calculated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub evaluation_time: Option<DateTime<Utc>>,
}

fn clamp01(value: Decimal) -> Decimal {
    value.max(Decimal::ZERO).min(Decimal::ONE)
}

pub fn evaluate_ivv_freshness(
    calculated_at: DateTime<Utc>,
    validity_hours: i64,
    evaluation_time: DateTime<Utc>,
) -> bool {
    evaluation_time > calculated_at + Duration::hours(validity_hours)
}

pub fn calculate_ivv(
    model: &IvvModelParams,
    segment: &SegmentGeoInput,
    observation: &WeatherInput,
    calculated_at: Option<DateTime<Utc>>,
    evaluation_time: Option<DateTime<Utc>>,
) -> Result<IvvOutput, IvvError> {
    if model.w_precip < Decimal::ZERO
        || model.w_slope_ravine < Decimal::ZERO
        || model.w_soil < Decimal::ZERO
        || model.w_hotspot < Decimal::ZERO
    {
        return Err(IvvError::InvalidModelConfig("weights must be non-negative"));
    }

    let weights_sum = model.w_precip + model.w_slope_ravine + model.w_soil + model.w_hotspot;
    if weights_sum != Decimal::ONE {
        return Err(IvvError::InvalidWeightsSum(weights_sum));
    }

    if model.precip_24h_cap_mm <= Decimal::ZERO
        || model.precip_72h_cap_mm <= Decimal::ZERO
        || model.slope_cap_pct <= Decimal::ZERO
        || model.ravine_max_dist_m <= Decimal::ZERO
        || model.validity_hours <= 0
        || model.red_threshold <= model.amber_threshold
    {
        return Err(IvvError::InvalidModelConfig("invalid cap or threshold"));
    }

    if observation.precip_24h_mm < Decimal::ZERO
        || observation.precip_72h_mm < observation.precip_24h_mm
    {
        return Err(IvvError::InvalidObservation(
            "precip_72h_mm must be >= precip_24h_mm and non-negative",
        ));
    }

    if segment.max_slope_forward_pct < Decimal::ZERO
        || segment
            .max_slope_reverse_pct
            .is_some_and(|v| v < Decimal::ZERO)
        || segment.distance_to_ravine_m < Decimal::ZERO
        || segment.soil_susceptibility_idx < Decimal::ZERO
        || segment.soil_susceptibility_idx > Decimal::ONE
    {
        return Err(IvvError::InvalidSegment("out-of-range geotechnical input"));
    }

    if let Some(eval_time) = evaluation_time {
        if observation.observed_at > eval_time + Duration::minutes(5) {
            return Err(IvvError::InvalidObservation(
                "observed_at exceeds evaluation_time tolerance (+5 min)",
            ));
        }
    }

    let p24 = clamp01(observation.precip_24h_mm / model.precip_24h_cap_mm);
    let p72 = clamp01(observation.precip_72h_mm / model.precip_72h_cap_mm);
    let p = Decimal::new(6, 1) * p24 + Decimal::new(4, 1) * p72;

    let slope_max = segment
        .max_slope_forward_pct
        .max(segment.max_slope_reverse_pct.unwrap_or(Decimal::ZERO));
    let s_slope = clamp01(slope_max / model.slope_cap_pct);
    let s_ravine = Decimal::ONE - clamp01(segment.distance_to_ravine_m / model.ravine_max_dist_m);
    let s = Decimal::new(5, 1) * s_slope + Decimal::new(5, 1) * s_ravine;

    let g = segment.soil_susceptibility_idx;
    let h = if segment.is_historical_hotspot {
        Decimal::ONE
    } else {
        Decimal::new(2, 1)
    };

    let weighted =
        model.w_precip * p + model.w_slope_ravine * s + model.w_soil * g + model.w_hotspot * h;
    let ivv_score = round_half_away_from_zero(Decimal::from(100) * weighted, 2);

    let is_stale = match (calculated_at, evaluation_time) {
        (Some(calc), Some(eval)) => evaluate_ivv_freshness(calc, model.validity_hours, eval),
        _ => false,
    };

    let risk_level = if is_stale {
        RiskLevel::Unknown
    } else if ivv_score < model.amber_threshold {
        RiskLevel::Green
    } else if ivv_score < model.red_threshold {
        RiskLevel::Amber
    } else {
        RiskLevel::Red
    };

    Ok(IvvOutput {
        model_version_tag: model.version_tag.clone(),
        ivv_score,
        risk_level,
        is_stale,
        components: IvvComponents { p, s, g, h },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn initial_model() -> IvvModelParams {
        IvvModelParams {
            version_tag: "ivv-v1.0-initial".to_string(),
            w_precip: Decimal::from_str("0.350").unwrap(),
            w_slope_ravine: Decimal::from_str("0.300").unwrap(),
            w_soil: Decimal::from_str("0.200").unwrap(),
            w_hotspot: Decimal::from_str("0.150").unwrap(),
            precip_24h_cap_mm: Decimal::from(80),
            precip_72h_cap_mm: Decimal::from(180),
            slope_cap_pct: Decimal::from(45),
            ravine_max_dist_m: Decimal::from(500),
            amber_threshold: Decimal::from(40),
            red_threshold: Decimal::from(70),
            validity_hours: 3,
        }
    }

    #[test]
    fn ivv_01_deterministic_bounds_and_carpish_red() {
        let model = initial_model();
        let obs_at = DateTime::parse_from_rfc3339("2026-11-21T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // Maximos = 100.00
        let max_out = calculate_ivv(
            &model,
            &SegmentGeoInput {
                max_slope_forward_pct: Decimal::from(45),
                max_slope_reverse_pct: None,
                soil_susceptibility_idx: Decimal::ONE,
                distance_to_ravine_m: Decimal::ZERO,
                is_historical_hotspot: true,
            },
            &WeatherInput {
                precip_24h_mm: Decimal::from(80),
                precip_72h_mm: Decimal::from(180),
                observed_at: obs_at,
            },
            None,
            None,
        )
        .unwrap();
        assert_eq!(max_out.ivv_score, Decimal::from_str("100.00").unwrap());
        assert_eq!(max_out.risk_level, RiskLevel::Red);

        // Minimos (H=0.2) = 3.00
        let min_out = calculate_ivv(
            &model,
            &SegmentGeoInput {
                max_slope_forward_pct: Decimal::ZERO,
                max_slope_reverse_pct: None,
                soil_susceptibility_idx: Decimal::ZERO,
                distance_to_ravine_m: Decimal::from(500),
                is_historical_hotspot: false,
            },
            &WeatherInput {
                precip_24h_mm: Decimal::ZERO,
                precip_72h_mm: Decimal::ZERO,
                observed_at: obs_at,
            },
            None,
            None,
        )
        .unwrap();
        assert_eq!(min_out.ivv_score, Decimal::from_str("3.00").unwrap());
        assert_eq!(min_out.risk_level, RiskLevel::Green);

        // Tramo Carpish con lluvia 85/190 mm -> 84.00 RED
        let carpish_out = calculate_ivv(
            &model,
            &SegmentGeoInput {
                max_slope_forward_pct: Decimal::from_str("27.0").unwrap(),
                max_slope_reverse_pct: None,
                soil_susceptibility_idx: Decimal::from_str("0.80").unwrap(),
                distance_to_ravine_m: Decimal::from(200),
                is_historical_hotspot: true,
            },
            &WeatherInput {
                precip_24h_mm: Decimal::from(85),
                precip_72h_mm: Decimal::from(190),
                observed_at: obs_at,
            },
            None,
            None,
        )
        .unwrap();
        assert_eq!(carpish_out.ivv_score, Decimal::from_str("84.00").unwrap());
        assert_eq!(carpish_out.risk_level, RiskLevel::Red);
    }

    #[test]
    fn ivv_02_stale_assessment_becomes_unknown() {
        let model = initial_model();
        let obs_at = DateTime::parse_from_rfc3339("2026-11-21T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let calc_at = obs_at;
        let eval_expired = calc_at + Duration::hours(3) + Duration::seconds(1);

        let out = calculate_ivv(
            &model,
            &SegmentGeoInput {
                max_slope_forward_pct: Decimal::from(10),
                max_slope_reverse_pct: None,
                soil_susceptibility_idx: Decimal::from_str("0.20").unwrap(),
                distance_to_ravine_m: Decimal::from(400),
                is_historical_hotspot: false,
            },
            &WeatherInput {
                precip_24h_mm: Decimal::from(10),
                precip_72h_mm: Decimal::from(20),
                observed_at: obs_at,
            },
            Some(calc_at),
            Some(eval_expired),
        )
        .unwrap();
        assert!(out.is_stale);
        assert_eq!(out.risk_level, RiskLevel::Unknown);
    }
}
