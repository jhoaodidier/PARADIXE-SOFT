import type { components } from "./schema.d.ts";

export type CorridorMapFeatureCollection =
  components["schemas"]["CorridorMapFeatureCollection"];
export type CorridorSegmentFeature =
  components["schemas"]["CorridorSegmentFeature"];
export type RoutingCalculateRequest =
  components["schemas"]["RoutingCalculateRequest"];
export type RoutingCalculateResponse =
  components["schemas"]["RoutingCalculateResponse"];
export type SyncSnapshotResponse =
  components["schemas"]["SyncSnapshotResponse"];
export type SyncBatchRequest = components["schemas"]["SyncBatchRequest"];
export type SyncBatchResponse = components["schemas"]["SyncBatchResponse"];
export type FieldReportCreateRequest =
  components["schemas"]["FieldReportCreateRequest"];
export type FieldReportResponse =
  components["schemas"]["FieldReportResponse"];
export type ApiError = components["schemas"]["ApiError"];
export type VehicleCategoryCode =
  components["schemas"]["VehicleCategoryCode"];
export type CargoType = components["schemas"]["CargoType"];
export type ProposedPassability = components["schemas"]["ProposedPassability"];
export type NoSafeRouteReasonCode =
  components["schemas"]["NoSafeRouteReasonCode"];

export type RoleCode =
  | "DRIVER"
  | "HEALTH_OPERATOR"
  | "CIVIL_DEFENSE"
  | "COER_ADMIN"
  | "BOT_SERVICE";

export interface TerritorialUnit {
  unit_id: string;
  ubigeo_code: string;
  iso_code: string | null;
  parent_unit_id: string | null;
  name: string;
  admin_level: number;
  center_lon: number;
  center_lat: number;
  min_lon: number;
  min_lat: number;
  max_lon: number;
  max_lat: number;
  pmtiles_uri: string | null;
  segment_count: number;
}

export interface RegionsListResponse {
  regions: TerritorialUnit[];
}

export interface OtpRequestResponse {
  challenge_id: string;
  expires_in_sec: number;
  message: string;
}

export interface AuthTokenResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in_sec: number;
}

export interface AuthMeResponse {
  user_id: string;
  phone_e164: string;
  full_name: string;
  role_code: RoleCode;
  reputation_score: number;
}

export interface IncidentListItem {
  incident_id: string;
  segment_id: string;
  corridor_name: string;
  trigger_source: string;
  verification_state: "SUSPECTED" | "CONFIRMED" | "DISMISSED" | "RESOLVED";
  impact_passability: "CAUTION_SUSPECTED" | "PARTIAL_ONE_LANE" | "BLOCKED";
  hazard_lat: number;
  hazard_lon: number;
  hazard_radius_m: number;
  detected_at: string;
  confirmed_at: string | null;
  resolved_at: string | null;
}

export interface IncidentsListResponse {
  incidents: IncidentListItem[];
}

export interface TransferCorridorSummary {
  corridor_id: string;
  side_a_node_id: string;
  side_b_node_id: string;
  length_m: number;
  max_slope_pct: number;
  transfer_mode: "PEDESTRIAN_SAFE_PATH" | "SHUTTLE_RELAY";
  max_safe_ivv: number;
  is_verified_safe: boolean;
  last_inspected_at: string;
  inspection_valid_days: number;
  requires_revalidation: boolean;
  cargo_types: CargoType[];
}

export interface LogisticBridgeSummary {
  bridge_id: string;
  incident_id: string;
  corridor_id: string;
  cargo_type: CargoType;
  status: "PROPOSED" | "ACTIVE" | "SUSPENDED_UNSAFE" | "COMPLETED";
  created_at: string;
}

export interface HumanitarianWindowSummary {
  window_id: string;
  incident_id: string;
  vehicle_category_id: string;
  vehicle_category_code: VehicleCategoryCode;
  direction: "ORIGIN_TO_DEST" | "DEST_TO_ORIGIN";
  starts_at: string;
  ends_at: string;
  status: "SCHEDULED" | "ACTIVE" | "CLOSED";
}

export interface VehicleCategoryItem {
  category_id: string;
  code: VehicleCategoryCode;
  max_slope_pct: number;
  min_road_width_m: number;
  max_weight_tons: number;
  allows_dirt_track: boolean;
}

export interface BridgesOperationsResponse {
  transfer_corridors: TransferCorridorSummary[];
  bridges: LogisticBridgeSummary[];
  humanitarian_windows: HumanitarianWindowSummary[];
  vehicle_categories: VehicleCategoryItem[];
}

export interface SimScenarioResponse {
  scenario_id: string;
  name: string;
  fixture_tag: string;
  revision: number;
  status: string;
  created_at: string;
}

export type SimEventRequest =
  | {
      event_type: "WEATHER_OVERRIDE";
      segment_id: string;
      precip_24h_mm: number;
      precip_72h_mm: number;
    }
  | {
      event_type: "BLOCKAGE_OVERRIDE";
      segment_id: string;
      simulated_passability: "BLOCKED" | "PARTIAL_ONE_LANE";
      hazard_lat: number;
      hazard_lon: number;
      hazard_radius_m: number;
    };

export interface SimEventResponse {
  sim_event_id: string;
  scenario_id: string;
  execution_context: "SIMULATION";
  simulated_notifications: Array<{ channel: string; preview: string }>;
}

export type SimRoutingResponse = RoutingCalculateResponse & {
  execution_context: "SIMULATION";
  scenario_id: string;
};

export interface SimEvaluateResponse {
  sim_log_id: string;
  resolution_type: string;
  mitigation_status: "MITIGATED" | "UNMITIGATED_NO_SAFE_ROUTE";
  scenario_revision: number;
  evaluation_time: string;
  param_version_tag: string;
  model_version_tag: string;
  routing_config_tag: string;
}

export interface SimImpactItem {
  sim_log_id: string;
  resolution_type: string;
  mitigation_status: "MITIGATED" | "UNMITIGATED_NO_SAFE_ROUTE";
  baseline: {
    travel_min: number;
    co2_kg: number;
    cargo_loss_usd: number;
  };
  mitigated: {
    travel_min: number | null;
    co2_kg: number | null;
    cargo_loss_usd: number | null;
  };
  deltas: {
    time_delta_min: number | null;
    time_saved_pct: number | null;
    net_co2_delta_kg: number | null;
    net_agri_savings_usd: number | null;
  };
}

export interface SimImpactResponse {
  scenario_id: string;
  evaluations: SimImpactItem[];
}

export class ApiClientError extends Error {
  readonly status: number;
  readonly errorCode: string;
  readonly details?: Record<string, unknown>;

  constructor(
    status: number,
    errorCode: string,
    message: string,
    details?: Record<string, unknown>
  ) {
    super(message);
    this.status = status;
    this.errorCode = errorCode;
    this.details = details;
  }
}

const DEFAULT_BASE_URL = "/api/v1";
let fallbackDeviceId: string | null = null;

export function getOrCreateDeviceId(): string {
  try {
    const existing = localStorage.getItem("paradixe_device_id");
    if (existing && existing.length > 0) return existing;
    const generated = `web-${crypto.randomUUID()}`;
    localStorage.setItem("paradixe_device_id", generated);
    return generated;
  } catch {
    if (!fallbackDeviceId) {
      fallbackDeviceId =
        typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
          ? `web-${crypto.randomUUID()}`
          : "web-session-device";
    }
    return fallbackDeviceId;
  }
}

function getBaseUrl(): string {
  const configured = import.meta.env?.VITE_API_BASE_URL;
  return typeof configured === "string" && configured.length > 0
    ? configured.replace(/\/+$/, "")
    : DEFAULT_BASE_URL;
}

async function requestJson<T>(
  path: string,
  options: RequestInit & { token?: string | null } = {}
): Promise<T> {
  const { token, headers, ...rest } = options;
  const finalHeaders = new Headers(headers);
  if (!finalHeaders.has("Content-Type") && rest.body) {
    finalHeaders.set("Content-Type", "application/json");
  }
  if (token) {
    finalHeaders.set("Authorization", `Bearer ${token}`);
  }

  const response = await fetch(`${getBaseUrl()}${path}`, {
    ...rest,
    headers: finalHeaders,
  });

  if (!response.ok) {
    let payload: Partial<ApiError> | undefined;
    try {
      payload = (await response.json()) as Partial<ApiError>;
    } catch {
      payload = undefined;
    }
    throw new ApiClientError(
      response.status,
      payload?.error_code ?? `HTTP_${response.status}`,
      payload?.message ?? `Error del servidor (${response.status})`,
      payload?.details
    );
  }

  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

export function fetchRegions(): Promise<RegionsListResponse> {
  return requestJson<RegionsListResponse>("/regions", {
    method: "GET",
  });
}

export function fetchCorridorsMap(
  token?: string | null,
  options?: { region?: string; bbox?: string; limit?: number }
): Promise<CorridorMapFeatureCollection> {
  const params = new URLSearchParams();
  if (options?.region) params.set("region", options.region);
  if (options?.bbox) params.set("bbox", options.bbox);
  if (typeof options?.limit === "number") params.set("limit", String(options.limit));
  const qs = params.toString();
  return requestJson<CorridorMapFeatureCollection>(`/corridors/map${qs ? `?${qs}` : ""}`, {
    method: "GET",
    token,
  });
}

export function fetchSyncSnapshot(
  sinceVersion: number,
  limit = 500,
  token?: string | null
): Promise<SyncSnapshotResponse> {
  const params = new URLSearchParams({
    since_version: String(sinceVersion),
    limit: String(limit),
  });
  return requestJson<SyncSnapshotResponse>(`/sync/snapshot?${params.toString()}`, {
    method: "GET",
    token,
  });
}

export function postSyncBatch(
  body: SyncBatchRequest,
  token?: string | null
): Promise<SyncBatchResponse> {
  return requestJson<SyncBatchResponse>("/sync/batch", {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function postRoutingCalculate(
  body: RoutingCalculateRequest,
  token?: string | null
): Promise<RoutingCalculateResponse> {
  return requestJson<RoutingCalculateResponse>("/routing/calculate", {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function postFieldReport(
  body: FieldReportCreateRequest,
  token?: string | null
): Promise<FieldReportResponse> {
  return requestJson<FieldReportResponse>("/reports", {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function postAuthOtpRequest(phoneE164: string): Promise<OtpRequestResponse> {
  return requestJson<OtpRequestResponse>("/auth/otp/request", {
    method: "POST",
    body: JSON.stringify({ phone_e164: phoneE164 }),
  });
}

export function postAuthLogin(body: {
  phone_e164: string;
  device_id: string;
  password?: string;
  challenge_id?: string;
  otp_code?: string;
}): Promise<AuthTokenResponse> {
  return requestJson<AuthTokenResponse>("/auth/login", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export function postAuthRefresh(
  refreshToken: string,
  deviceId: string
): Promise<AuthTokenResponse> {
  return requestJson<AuthTokenResponse>("/auth/refresh", {
    method: "POST",
    body: JSON.stringify({ refresh_token: refreshToken, device_id: deviceId }),
  });
}

export function postAuthLogout(
  refreshToken: string,
  deviceId: string,
  token: string
): Promise<void> {
  return requestJson<void>("/auth/logout", {
    method: "POST",
    body: JSON.stringify({ refresh_token: refreshToken, device_id: deviceId }),
    token,
  });
}

export function fetchAuthMe(token: string): Promise<AuthMeResponse> {
  return requestJson<AuthMeResponse>("/auth/me", {
    method: "GET",
    token,
  });
}

export function fetchIncidents(options?: {
  state?: string;
  limit?: number;
}): Promise<IncidentsListResponse> {
  const params = new URLSearchParams();
  if (options?.state) params.set("state", options.state);
  if (typeof options?.limit === "number") params.set("limit", String(options.limit));
  const qs = params.toString();
  return requestJson<IncidentsListResponse>(`/incidents${qs ? `?${qs}` : ""}`, {
    method: "GET",
  });
}

export function patchIncidentVerification(
  incidentId: string,
  body: {
    verification_state: "SUSPECTED" | "CONFIRMED" | "DISMISSED" | "RESOLVED";
    impact_passability: "CAUTION_SUSPECTED" | "PARTIAL_ONE_LANE" | "BLOCKED";
    notes?: string;
  },
  token: string
): Promise<{ incident_id: string; verification_state: string; impact_passability: string }> {
  return requestJson(`/incidents/${incidentId}/verification`, {
    method: "PATCH",
    body: JSON.stringify(body),
    token,
  });
}

export function fetchBridgesOperations(): Promise<BridgesOperationsResponse> {
  return requestJson<BridgesOperationsResponse>("/operations/bridges", {
    method: "GET",
  });
}

export function postProposeBridge(
  body: { incident_id: string; corridor_id: string; cargo_type: CargoType },
  token: string
): Promise<LogisticBridgeSummary> {
  return requestJson<LogisticBridgeSummary>("/bridges", {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function patchBridgeStatus(
  bridgeId: string,
  status: "ACTIVE" | "SUSPENDED_UNSAFE" | "COMPLETED",
  token: string
): Promise<LogisticBridgeSummary> {
  return requestJson<LogisticBridgeSummary>(`/bridges/${bridgeId}/status`, {
    method: "PATCH",
    body: JSON.stringify({ status }),
    token,
  });
}

export function postHumanitarianWindow(
  incidentId: string,
  body: {
    vehicle_category_id: string;
    direction: "ORIGIN_TO_DEST" | "DEST_TO_ORIGIN";
    starts_at: string;
    ends_at: string;
  },
  token: string
): Promise<HumanitarianWindowSummary> {
  return requestJson<HumanitarianWindowSummary>(`/incidents/${incidentId}/humanitarian-windows`, {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function postSimCreateScenario(
  body: { name: string; fixture_tag: string },
  token: string
): Promise<SimScenarioResponse> {
  return requestJson<SimScenarioResponse>("/simulator/scenarios", {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function postSimInjectEvent(
  scenarioId: string,
  body: SimEventRequest,
  token: string
): Promise<SimEventResponse> {
  return requestJson<SimEventResponse>(`/simulator/scenarios/${scenarioId}/events`, {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function postSimRouting(
  scenarioId: string,
  body: {
    origin_node_id: string;
    destination_node_id: string;
    vehicle_category_code: VehicleCategoryCode;
    cargo_type: CargoType;
    cargo_payload_kg?: number;
    patient_count?: number;
    scenario_revision: number;
    evaluation_time: string;
  },
  token: string
): Promise<SimRoutingResponse> {
  return requestJson<SimRoutingResponse>(`/simulator/scenarios/${scenarioId}/routing`, {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function postSimEvaluate(
  scenarioId: string,
  body: {
    origin_node_id: string;
    destination_node_id: string;
    vehicle_category_code: VehicleCategoryCode;
    cargo_type: CargoType;
    cargo_payload_kg?: number;
    patient_count?: number;
    baseline_wait_min: number;
    scenario_revision: number;
    evaluation_time: string;
  },
  token: string
): Promise<SimEvaluateResponse> {
  return requestJson<SimEvaluateResponse>(`/simulator/scenarios/${scenarioId}/evaluate`, {
    method: "POST",
    body: JSON.stringify(body),
    token,
  });
}

export function fetchSimImpact(scenarioId: string, token: string): Promise<SimImpactResponse> {
  return requestJson<SimImpactResponse>(`/simulator/scenarios/${scenarioId}/impact`, {
    method: "GET",
    token,
  });
}
