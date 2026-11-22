#![forbid(unsafe_code)]

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use paradixe_domain::{
    round_half_away_from_zero, BridgeProposal, CargoType, Direction, DirectionType,
    NoSafeRouteReasonCode, PassabilityState, RouteHop, RoutingResolution, SurfaceType,
    TransferMode, VehicleCategoryCode,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleCategorySpec {
    pub code: VehicleCategoryCode,
    pub max_slope_pct: Decimal,
    pub min_road_width_m: Decimal,
    pub max_weight_tons: Decimal,
    pub allows_dirt_track: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeSpec {
    pub node_id: Uuid,
    pub is_geotechnically_stable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentSpec {
    pub segment_id: Uuid,
    pub origin_node_id: Uuid,
    pub destination_node_id: Uuid,
    pub direction_type: DirectionType,
    pub length_km: Decimal,
    pub max_slope_forward_pct: Decimal,
    #[serde(default)]
    pub max_slope_reverse_pct: Option<Decimal>,
    pub road_width_m: Decimal,
    pub max_supported_tons: Decimal,
    pub surface_type: SurfaceType,
    pub base_travel_time_min: Decimal,
    pub is_topology_verified: bool,
    #[serde(default)]
    pub attributes_verified_at: Option<DateTime<Utc>>,
    pub passability_state: PassabilityState,
    #[serde(default)]
    pub active_confirmed_incident_id: Option<Uuid>,
    #[serde(default)]
    pub ivv_score: Option<Decimal>,
    #[serde(default)]
    pub ivv_is_stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferCapabilitySpec {
    pub cargo_type: CargoType,
    pub max_payload_kg: Decimal,
    pub requires_medical_supervision: bool,
    pub estimated_transfer_time_min: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferCorridorSpec {
    pub corridor_id: Uuid,
    pub side_a_node_id: Uuid,
    pub side_b_node_id: Uuid,
    pub length_m: u32,
    pub transfer_mode: TransferMode,
    pub max_safe_ivv: Decimal,
    pub is_verified_safe: bool,
    pub last_inspected_at: DateTime<Utc>,
    pub inspection_valid_days: i64,
    pub requires_revalidation: bool,
    pub hazard_intersects_path: bool,
    pub sector_ivv: Option<Decimal>,
    #[serde(default)]
    pub sector_ivv_is_stale: bool,
    pub capabilities: Vec<TransferCapabilitySpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanitarianWindowSpec {
    pub segment_id: Uuid,
    pub vehicle_category_code: VehicleCategoryCode,
    pub direction: Direction,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingConfig {
    pub version_tag: String,
    pub alpha_ivv_weight: Decimal,
    pub beta_suspected_penalty: Decimal,
    #[serde(default)]
    pub allow_synthetic_attributes: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingRequest {
    pub origin_node_id: Uuid,
    pub destination_node_id: Uuid,
    pub vehicle_category_code: VehicleCategoryCode,
    pub cargo_type: CargoType,
    #[serde(default)]
    pub cargo_payload_kg: Option<Decimal>,
    #[serde(default)]
    pub patient_count: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingInput {
    pub vehicle: VehicleCategorySpec,
    pub nodes: Vec<NodeSpec>,
    pub segments: Vec<SegmentSpec>,
    #[serde(default)]
    pub transfer_corridors: Vec<TransferCorridorSpec>,
    #[serde(default)]
    pub humanitarian_windows: Vec<HumanitarianWindowSpec>,
    pub config: RoutingConfig,
    pub request: RoutingRequest,
    pub evaluation_time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct DirectedEdge<'a> {
    to: Uuid,
    direction: Direction,
    segment: &'a SegmentSpec,
    cost: Decimal,
    nominal_time_min: Decimal,
    used_synthetic_attrs: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct HeapItem {
    cost: Decimal,
    node: Uuid,
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| self.node.cmp(&other.node))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
struct PathFound {
    hops: Vec<RouteHop>,
    total_distance_km: Decimal,
    estimated_time_min: Decimal,
    max_ivv_on_route: Decimal,
    used_synthetic_attrs: bool,
}

fn decimal_minutes_to_duration(minutes: Decimal) -> Duration {
    let millis = (minutes * Decimal::from(60_000))
        .round()
        .to_string()
        .parse::<i64>()
        .unwrap_or(0);
    Duration::milliseconds(millis)
}

fn find_shortest_path(
    origin: Uuid,
    destination: Uuid,
    adjacency: &HashMap<Uuid, Vec<DirectedEdge<'_>>>,
    windows: &[HumanitarianWindowSpec],
    vehicle_code: VehicleCategoryCode,
    start_time: DateTime<Utc>,
) -> Option<PathFound> {
    if origin == destination {
        return Some(PathFound {
            hops: Vec::new(),
            total_distance_km: Decimal::ZERO,
            estimated_time_min: Decimal::ZERO,
            max_ivv_on_route: Decimal::ZERO,
            used_synthetic_attrs: false,
        });
    }

    let mut dist: HashMap<Uuid, Decimal> = HashMap::new();
    let mut arrival_times: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
    let mut prev: HashMap<Uuid, (Uuid, usize)> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(origin, Decimal::ZERO);
    arrival_times.insert(origin, start_time);
    heap.push(HeapItem {
        cost: Decimal::ZERO,
        node: origin,
    });

    while let Some(HeapItem { cost, node }) = heap.pop() {
        if node == destination {
            break;
        }
        if cost > *dist.get(&node).unwrap_or(&Decimal::MAX) {
            continue;
        }
        let current_time = arrival_times[&node];

        let Some(edges) = adjacency.get(&node) else {
            continue;
        };
        for (idx, edge) in edges.iter().enumerate() {
            if edge.segment.passability_state == PassabilityState::PartialOneLane {
                let entry_at = current_time;
                let exit_at = entry_at + decimal_minutes_to_duration(edge.nominal_time_min);
                let has_window = windows.iter().any(|w| {
                    w.segment_id == edge.segment.segment_id
                        && w.direction == edge.direction
                        && w.vehicle_category_code == vehicle_code
                        && w.starts_at <= entry_at
                        && exit_at <= w.ends_at
                });
                if !has_window {
                    continue;
                }
            }

            let next_cost = cost + edge.cost;
            if next_cost < *dist.get(&edge.to).unwrap_or(&Decimal::MAX) {
                dist.insert(edge.to, next_cost);
                arrival_times.insert(
                    edge.to,
                    current_time + decimal_minutes_to_duration(edge.nominal_time_min),
                );
                prev.insert(edge.to, (node, idx));
                heap.push(HeapItem {
                    cost: next_cost,
                    node: edge.to,
                });
            }
        }
    }

    if !prev.contains_key(&destination) {
        return None;
    }

    let mut hops_rev = Vec::new();
    let mut total_distance_km = Decimal::ZERO;
    let mut estimated_time_min = Decimal::ZERO;
    let mut max_ivv_on_route = Decimal::ZERO;
    let mut used_synthetic_attrs = false;

    let mut cur = destination;
    while let Some(&(parent, edge_idx)) = prev.get(&cur) {
        let edge = &adjacency[&parent][edge_idx];
        hops_rev.push(RouteHop {
            segment_id: edge.segment.segment_id,
            direction: edge.direction,
        });
        total_distance_km += edge.segment.length_km;
        estimated_time_min += edge.nominal_time_min;
        if let Some(ivv) = edge.segment.ivv_score {
            if ivv > max_ivv_on_route {
                max_ivv_on_route = ivv;
            }
        }
        used_synthetic_attrs |= edge.used_synthetic_attrs;
        cur = parent;
    }

    hops_rev.reverse();
    Some(PathFound {
        hops: hops_rev,
        total_distance_km: round_half_away_from_zero(total_distance_km, 2),
        estimated_time_min: round_half_away_from_zero(estimated_time_min, 2),
        max_ivv_on_route: round_half_away_from_zero(max_ivv_on_route, 2),
        used_synthetic_attrs,
    })
}

pub fn calculate_route(input: &RoutingInput) -> RoutingResolution {
    let mut reasons: HashSet<NoSafeRouteReasonCode> = HashSet::new();
    let mut blocking_incidents: Vec<Uuid> = Vec::new();
    let node_map: HashMap<Uuid, &NodeSpec> = input.nodes.iter().map(|n| (n.node_id, n)).collect();

    if !node_map.contains_key(&input.request.origin_node_id)
        || !node_map.contains_key(&input.request.destination_node_id)
    {
        return RoutingResolution::NoSafeRoute {
            blocking_incident_id: None,
            reason_code: NoSafeRouteReasonCode::NetworkDisconnected,
            warnings: vec![],
        };
    }

    let mut adjacency: HashMap<Uuid, Vec<DirectedEdge<'_>>> = HashMap::new();

    for seg in &input.segments {
        if !seg.is_topology_verified {
            continue;
        }
        if seg.passability_state == PassabilityState::Blocked {
            if let Some(inc_id) = seg.active_confirmed_incident_id {
                blocking_incidents.push(inc_id);
            }
            continue;
        }

        let candidates: Vec<(Uuid, Uuid, Direction, Decimal)> = match seg.direction_type {
            DirectionType::OneWay => vec![(
                seg.origin_node_id,
                seg.destination_node_id,
                Direction::OriginToDest,
                seg.max_slope_forward_pct,
            )],
            DirectionType::Bidirectional => vec![
                (
                    seg.origin_node_id,
                    seg.destination_node_id,
                    Direction::OriginToDest,
                    seg.max_slope_forward_pct,
                ),
                (
                    seg.destination_node_id,
                    seg.origin_node_id,
                    Direction::DestToOrigin,
                    seg.max_slope_reverse_pct
                        .unwrap_or(seg.max_slope_forward_pct),
                ),
            ],
        };

        for (from, to, direction, slope) in candidates {
            let mut used_synthetic_attrs = false;
            if seg.attributes_verified_at.is_none() {
                if input.config.allow_synthetic_attributes {
                    used_synthetic_attrs = true;
                } else {
                    reasons.insert(NoSafeRouteReasonCode::UnverifiedPhysicalAttributes);
                    continue;
                }
            }

            if seg.ivv_score.is_none() || seg.ivv_is_stale {
                reasons.insert(NoSafeRouteReasonCode::StaleIvv);
                continue;
            }
            let ivv = seg.ivv_score.unwrap_or(Decimal::ZERO);

            let surface_ok =
                seg.surface_type != SurfaceType::DirtTrack || input.vehicle.allows_dirt_track;
            if !surface_ok
                || slope > input.vehicle.max_slope_pct
                || seg.road_width_m < input.vehicle.min_road_width_m
                || input.vehicle.max_weight_tons > seg.max_supported_tons
            {
                reasons.insert(NoSafeRouteReasonCode::VehicleRestriction);
                continue;
            }

            let suspected_flag = if seg.passability_state == PassabilityState::CautionSuspected {
                Decimal::ONE
            } else {
                Decimal::ZERO
            };
            let ivv_factor = ivv / Decimal::from(100);
            let penalty = Decimal::ONE
                + input.config.alpha_ivv_weight * ivv_factor
                + input.config.beta_suspected_penalty * suspected_flag;
            let cost = seg.base_travel_time_min * penalty;

            adjacency.entry(from).or_default().push(DirectedEdge {
                to,
                direction,
                segment: seg,
                cost,
                nominal_time_min: seg.base_travel_time_min,
                used_synthetic_attrs,
            });
        }
    }

    if let Some(found) = find_shortest_path(
        input.request.origin_node_id,
        input.request.destination_node_id,
        &adjacency,
        &input.humanitarian_windows,
        input.vehicle.code,
        input.evaluation_time,
    ) {
        let mut warnings = Vec::new();
        if found.used_synthetic_attrs {
            warnings.push("SYNTHETIC_ATTRIBUTES".to_string());
        }
        return RoutingResolution::AlternateRoute {
            routing_config_tag: input.config.version_tag.clone(),
            total_distance_km: found.total_distance_km,
            estimated_time_min: found.estimated_time_min,
            max_ivv_on_route: found.max_ivv_on_route,
            hops: found.hops,
            eta_quality: "NOMINAL_ESTIMATE".to_string(),
            operational_authorization: "REQUIRED".to_string(),
            warnings,
        };
    }

    // Evaluar Puentes Logisticos si no hay ruta vehicular completa
    let mut bridge_found: Option<(Uuid, BridgeProposal)> = None;
    for corridor in &input.transfer_corridors {
        let side_a_stable = node_map
            .get(&corridor.side_a_node_id)
            .is_some_and(|n| n.is_geotechnically_stable);
        let side_b_stable = node_map
            .get(&corridor.side_b_node_id)
            .is_some_and(|n| n.is_geotechnically_stable);
        if !side_a_stable || !side_b_stable || !corridor.is_verified_safe {
            reasons.insert(NoSafeRouteReasonCode::NoVerifiedTransferCorridor);
            continue;
        }

        let leg1_ab = find_shortest_path(
            input.request.origin_node_id,
            corridor.side_a_node_id,
            &adjacency,
            &input.humanitarian_windows,
            input.vehicle.code,
            input.evaluation_time,
        );
        let leg2_ab = leg1_ab.as_ref().and_then(|l1| {
            find_shortest_path(
                corridor.side_b_node_id,
                input.request.destination_node_id,
                &adjacency,
                &input.humanitarian_windows,
                input.vehicle.code,
                input.evaluation_time + decimal_minutes_to_duration(l1.estimated_time_min),
            )
        });

        let connected = leg1_ab.is_some() && leg2_ab.is_some();
        if !connected {
            continue;
        }

        if corridor.requires_revalidation {
            reasons.insert(NoSafeRouteReasonCode::TransferRevalidationRequired);
            continue;
        }
        let expires_at =
            corridor.last_inspected_at + Duration::days(corridor.inspection_valid_days);
        if input.evaluation_time > expires_at {
            reasons.insert(NoSafeRouteReasonCode::TransferInspectionExpired);
            continue;
        }
        if corridor.hazard_intersects_path {
            reasons.insert(NoSafeRouteReasonCode::HazardIntersectsTransferPath);
            continue;
        }
        if corridor.sector_ivv_is_stale || corridor.sector_ivv.is_none() {
            reasons.insert(NoSafeRouteReasonCode::StaleIvv);
            continue;
        }
        if corridor.sector_ivv.unwrap_or(Decimal::MAX) > corridor.max_safe_ivv {
            reasons.insert(NoSafeRouteReasonCode::TransferIvvExceeded);
            continue;
        }

        let Some(cap) = corridor
            .capabilities
            .iter()
            .find(|c| c.cargo_type == input.request.cargo_type)
        else {
            reasons.insert(NoSafeRouteReasonCode::TransferCargoNotAllowed);
            continue;
        };

        if matches!(
            input.request.cargo_type,
            CargoType::PerishableAgri | CargoType::MedicalSupplies
        ) {
            let Some(payload) = input.request.cargo_payload_kg else {
                reasons.insert(NoSafeRouteReasonCode::TransferCargoNotAllowed);
                continue;
            };
            if payload <= Decimal::ZERO || payload > cap.max_payload_kg {
                reasons.insert(NoSafeRouteReasonCode::TransferCargoCapacityExceeded);
                continue;
            }
        }

        if let Some(incident_id) = blocking_incidents.first().copied() {
            bridge_found = Some((
                incident_id,
                BridgeProposal {
                    corridor_id: corridor.corridor_id,
                    side_a_node_id: corridor.side_a_node_id,
                    side_b_node_id: corridor.side_b_node_id,
                    transfer_mode: corridor.transfer_mode,
                    length_m: corridor.length_m,
                    last_inspected_at: corridor.last_inspected_at,
                    inspection_expires_at: expires_at,
                    operational_authorization: "REQUIRED".to_string(),
                    estimated_transfer_time_min: cap.estimated_transfer_time_min,
                },
            ));
            break;
        } else {
            reasons.insert(NoSafeRouteReasonCode::NoVerifiedTransferCorridor);
        }
    }

    if let Some((blocking_incident_id, bridge_proposal)) = bridge_found {
        return RoutingResolution::LogisticBridgeRequired {
            blocking_incident_id,
            bridge_proposal,
            warnings: vec![],
        };
    }

    if reasons.is_empty() {
        reasons.insert(NoSafeRouteReasonCode::NetworkDisconnected);
    }

    let mut ordered: Vec<NoSafeRouteReasonCode> = reasons.into_iter().collect();
    ordered.sort_by_key(NoSafeRouteReasonCode::precedence_rank);
    let primary_reason = ordered[0];
    let secondary_warnings: Vec<String> = ordered[1..]
        .iter()
        .filter_map(|r| serde_json::to_value(r).ok())
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();

    RoutingResolution::NoSafeRoute {
        blocking_incident_id: blocking_incidents.first().copied(),
        reason_code: primary_reason,
        warnings: secondary_warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn now_utc() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-21T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn heavy_trailer() -> VehicleCategorySpec {
        VehicleCategorySpec {
            code: VehicleCategoryCode::HeavyTrailer,
            max_slope_pct: Decimal::from(10),
            min_road_width_m: Decimal::from(4),
            max_weight_tons: Decimal::from(36),
            allows_dirt_track: false,
        }
    }

    fn base_segment(
        id: Uuid,
        from: Uuid,
        to: Uuid,
        time_min: i64,
        ivv: Option<&str>,
    ) -> SegmentSpec {
        SegmentSpec {
            segment_id: id,
            origin_node_id: from,
            destination_node_id: to,
            direction_type: DirectionType::Bidirectional,
            length_km: Decimal::from(20),
            max_slope_forward_pct: Decimal::from(6),
            max_slope_reverse_pct: Some(Decimal::from(6)),
            road_width_m: Decimal::from(6),
            max_supported_tons: Decimal::from(40),
            surface_type: SurfaceType::Paved,
            base_travel_time_min: Decimal::from(time_min),
            is_topology_verified: true,
            attributes_verified_at: Some(now_utc()),
            passability_state: PassabilityState::Open,
            active_confirmed_incident_id: None,
            ivv_score: ivv.map(|v| Decimal::from_str(v).unwrap()),
            ivv_is_stale: false,
        }
    }

    #[test]
    fn rte_01_alpha_weight_change_reroutes_amber_corridor() {
        let n1 = Uuid::new_v4();
        let n2 = Uuid::new_v4();
        let seg_amber = Uuid::new_v4();
        let seg_alt = Uuid::new_v4();

        let nodes = vec![
            NodeSpec {
                node_id: n1,
                is_geotechnically_stable: true,
            },
            NodeSpec {
                node_id: n2,
                is_geotechnically_stable: true,
            },
        ];
        // seg_amber: base 20 min, IVV 65 -> con alpha=0.75 costo=29.75; con alpha=2.00 costo=46.00
        // seg_alt: base 32 min, IVV 10 -> con alpha=0.75 costo=34.40; con alpha=2.00 costo=38.40
        let segments = vec![
            base_segment(seg_amber, n1, n2, 20, Some("65.00")),
            base_segment(seg_alt, n1, n2, 32, Some("10.00")),
        ];

        let req = RoutingRequest {
            origin_node_id: n1,
            destination_node_id: n2,
            vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
            cargo_type: CargoType::PerishableAgri,
            cargo_payload_kg: Some(Decimal::from(1200)),
            patient_count: None,
        };

        let res_low_alpha = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes: nodes.clone(),
            segments: segments.clone(),
            transfer_corridors: vec![],
            humanitarian_windows: vec![],
            config: RoutingConfig {
                version_tag: "routing-v1.0".to_string(),
                alpha_ivv_weight: Decimal::from_str("0.75").unwrap(),
                beta_suspected_penalty: Decimal::from_str("0.50").unwrap(),
                allow_synthetic_attributes: false,
            },
            request: req.clone(),
            evaluation_time: now_utc(),
        });
        match res_low_alpha {
            RoutingResolution::AlternateRoute { hops, .. } => {
                assert_eq!(hops[0].segment_id, seg_amber)
            }
            other => panic!("expected AlternateRoute, got {other:?}"),
        }

        let res_high_alpha = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes,
            segments,
            transfer_corridors: vec![],
            humanitarian_windows: vec![],
            config: RoutingConfig {
                version_tag: "routing-v2.0".to_string(),
                alpha_ivv_weight: Decimal::from_str("2.00").unwrap(),
                beta_suspected_penalty: Decimal::from_str("0.50").unwrap(),
                allow_synthetic_attributes: false,
            },
            request: req,
            evaluation_time: now_utc(),
        });
        match res_high_alpha {
            RoutingResolution::AlternateRoute { hops, .. } => {
                assert_eq!(hops[0].segment_id, seg_alt)
            }
            other => panic!("expected AlternateRoute, got {other:?}"),
        }
    }

    #[test]
    fn rte_02_and_rte_03_bidirectional_slope_and_partial_window_coverage() {
        let n1 = Uuid::new_v4();
        let n2 = Uuid::new_v4();
        let seg_id = Uuid::new_v4();
        let mut seg = base_segment(seg_id, n1, n2, 30, Some("20.00"));
        seg.max_slope_forward_pct = Decimal::from(6);
        seg.max_slope_reverse_pct = Some(Decimal::from(14));
        seg.passability_state = PassabilityState::PartialOneLane;

        let nodes = vec![
            NodeSpec {
                node_id: n1,
                is_geotechnically_stable: true,
            },
            NodeSpec {
                node_id: n2,
                is_geotechnically_stable: true,
            },
        ];
        let cfg = RoutingConfig {
            version_tag: "routing-v1.0".to_string(),
            alpha_ivv_weight: Decimal::from_str("0.75").unwrap(),
            beta_suspected_penalty: Decimal::from_str("0.50").unwrap(),
            allow_synthetic_attributes: false,
        };

        // Ventana de 35 min cubre viaje de 30 min en ORIGIN_TO_DEST
        let full_window = HumanitarianWindowSpec {
            segment_id: seg_id,
            vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
            direction: Direction::OriginToDest,
            starts_at: now_utc(),
            ends_at: now_utc() + Duration::minutes(35),
        };
        let ok = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes: nodes.clone(),
            segments: vec![seg.clone()],
            transfer_corridors: vec![],
            humanitarian_windows: vec![full_window],
            config: cfg.clone(),
            request: RoutingRequest {
                origin_node_id: n1,
                destination_node_id: n2,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::MedicalSupplies,
                cargo_payload_kg: Some(Decimal::from(100)),
                patient_count: None,
            },
            evaluation_time: now_utc(),
        });
        assert!(matches!(ok, RoutingResolution::AlternateRoute { .. }));

        // Ventana de solo 20 min termina a mitad de tramo (30 min) -> NO_SAFE_ROUTE
        let short_window = HumanitarianWindowSpec {
            segment_id: seg_id,
            vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
            direction: Direction::OriginToDest,
            starts_at: now_utc(),
            ends_at: now_utc() + Duration::minutes(20),
        };
        let blocked_short = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes: nodes.clone(),
            segments: vec![seg.clone()],
            transfer_corridors: vec![],
            humanitarian_windows: vec![short_window],
            config: cfg.clone(),
            request: RoutingRequest {
                origin_node_id: n1,
                destination_node_id: n2,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::MedicalSupplies,
                cargo_payload_kg: Some(Decimal::from(100)),
                patient_count: None,
            },
            evaluation_time: now_utc(),
        });
        assert!(matches!(
            blocked_short,
            RoutingResolution::NoSafeRoute { .. }
        ));

        // Sentido inverso tiene pendiente 14% > 10% -> VEHICLE_RESTRICTION
        let rev_blocked = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes,
            segments: vec![seg],
            transfer_corridors: vec![],
            humanitarian_windows: vec![],
            config: cfg,
            request: RoutingRequest {
                origin_node_id: n2,
                destination_node_id: n1,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::MedicalSupplies,
                cargo_payload_kg: Some(Decimal::from(100)),
                patient_count: None,
            },
            evaluation_time: now_utc(),
        });
        match rev_blocked {
            RoutingResolution::NoSafeRoute { reason_code, .. } => {
                assert_eq!(reason_code, NoSafeRouteReasonCode::VehicleRestriction)
            }
            other => panic!("expected VehicleRestriction, got {other:?}"),
        }
    }

    #[test]
    fn rte_04_and_bridge_checks() {
        let origin = Uuid::new_v4();
        let side_a = Uuid::new_v4();
        let side_b = Uuid::new_v4();
        let dest = Uuid::new_v4();
        let inc_id = Uuid::new_v4();

        let nodes = vec![
            NodeSpec {
                node_id: origin,
                is_geotechnically_stable: true,
            },
            NodeSpec {
                node_id: side_a,
                is_geotechnically_stable: true,
            },
            NodeSpec {
                node_id: side_b,
                is_geotechnically_stable: true,
            },
            NodeSpec {
                node_id: dest,
                is_geotechnically_stable: true,
            },
        ];
        let mut blocked_mid = base_segment(Uuid::new_v4(), side_a, side_b, 10, Some("84.00"));
        blocked_mid.passability_state = PassabilityState::Blocked;
        blocked_mid.active_confirmed_incident_id = Some(inc_id);

        let segments = vec![
            base_segment(Uuid::new_v4(), origin, side_a, 20, Some("20.00")),
            blocked_mid,
            base_segment(Uuid::new_v4(), side_b, dest, 25, Some("20.00")),
        ];

        let cfg = RoutingConfig {
            version_tag: "routing-v1.0".to_string(),
            alpha_ivv_weight: Decimal::from_str("0.75").unwrap(),
            beta_suspected_penalty: Decimal::from_str("0.50").unwrap(),
            allow_synthetic_attributes: false,
        };

        // BRG-01: inspeccion vencida hace 31 dias -> TRANSFER_INSPECTION_EXPIRED
        let expired_corridor = TransferCorridorSpec {
            corridor_id: Uuid::new_v4(),
            side_a_node_id: side_a,
            side_b_node_id: side_b,
            length_m: 450,
            transfer_mode: TransferMode::PedestrianSafePath,
            max_safe_ivv: Decimal::from(60),
            is_verified_safe: true,
            last_inspected_at: now_utc() - Duration::days(31),
            inspection_valid_days: 30,
            requires_revalidation: false,
            hazard_intersects_path: false,
            sector_ivv: Some(Decimal::from(35)),
            sector_ivv_is_stale: false,
            capabilities: vec![TransferCapabilitySpec {
                cargo_type: CargoType::PerishableAgri,
                max_payload_kg: Decimal::from(1500),
                requires_medical_supervision: false,
                estimated_transfer_time_min: Decimal::from(25),
            }],
        };

        let res_expired = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes: nodes.clone(),
            segments: segments.clone(),
            transfer_corridors: vec![expired_corridor.clone()],
            humanitarian_windows: vec![],
            config: cfg.clone(),
            request: RoutingRequest {
                origin_node_id: origin,
                destination_node_id: dest,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::PerishableAgri,
                cargo_payload_kg: Some(Decimal::from(1200)),
                patient_count: None,
            },
            evaluation_time: now_utc(),
        });
        assert!(matches!(
            res_expired,
            RoutingResolution::NoSafeRoute {
                reason_code: NoSafeRouteReasonCode::TransferInspectionExpired,
                ..
            }
        ));

        // BRG-02: peligro intersecta camino -> HAZARD_INTERSECTS_TRANSFER_PATH
        let mut hazard_corridor = expired_corridor.clone();
        hazard_corridor.last_inspected_at = now_utc() - Duration::days(5);
        hazard_corridor.hazard_intersects_path = true;
        let res_hazard = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes: nodes.clone(),
            segments: segments.clone(),
            transfer_corridors: vec![hazard_corridor.clone()],
            humanitarian_windows: vec![],
            config: cfg.clone(),
            request: RoutingRequest {
                origin_node_id: origin,
                destination_node_id: dest,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::PerishableAgri,
                cargo_payload_kg: Some(Decimal::from(1200)),
                patient_count: None,
            },
            evaluation_time: now_utc(),
        });
        assert!(matches!(
            res_hazard,
            RoutingResolution::NoSafeRoute {
                reason_code: NoSafeRouteReasonCode::HazardIntersectsTransferPath,
                ..
            }
        ));

        // BRG-03: paciente en sendero no habilitado para pacientes -> TRANSFER_CARGO_NOT_ALLOWED
        let mut valid_corridor = hazard_corridor;
        valid_corridor.hazard_intersects_path = false;
        let res_cargo = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes: nodes.clone(),
            segments: segments.clone(),
            transfer_corridors: vec![valid_corridor.clone()],
            humanitarian_windows: vec![],
            config: cfg.clone(),
            request: RoutingRequest {
                origin_node_id: origin,
                destination_node_id: dest,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::MedicalPatient,
                cargo_payload_kg: None,
                patient_count: Some(1),
            },
            evaluation_time: now_utc(),
        });
        assert!(matches!(
            res_cargo,
            RoutingResolution::NoSafeRoute {
                reason_code: NoSafeRouteReasonCode::TransferCargoNotAllowed,
                ..
            }
        ));

        // Corredor valido para PerishableAgri 1200 kg -> LOGISTIC_BRIDGE_REQUIRED
        let res_bridge = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes: nodes.clone(),
            segments,
            transfer_corridors: vec![valid_corridor],
            humanitarian_windows: vec![],
            config: cfg.clone(),
            request: RoutingRequest {
                origin_node_id: origin,
                destination_node_id: dest,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::PerishableAgri,
                cargo_payload_kg: Some(Decimal::from(1200)),
                patient_count: None,
            },
            evaluation_time: now_utc(),
        });
        assert!(matches!(
            res_bridge,
            RoutingResolution::LogisticBridgeRequired { .. }
        ));

        // RTE-04: grafo desconectado sin incidente
        let res_disc = calculate_route(&RoutingInput {
            vehicle: heavy_trailer(),
            nodes,
            segments: vec![],
            transfer_corridors: vec![],
            humanitarian_windows: vec![],
            config: cfg,
            request: RoutingRequest {
                origin_node_id: origin,
                destination_node_id: dest,
                vehicle_category_code: VehicleCategoryCode::HeavyTrailer,
                cargo_type: CargoType::MedicalSupplies,
                cargo_payload_kg: Some(Decimal::from(100)),
                patient_count: None,
            },
            evaluation_time: now_utc(),
        });
        assert!(matches!(
            res_disc,
            RoutingResolution::NoSafeRoute {
                blocking_incident_id: None,
                reason_code: NoSafeRouteReasonCode::NetworkDisconnected,
                ..
            }
        ));
    }
}
