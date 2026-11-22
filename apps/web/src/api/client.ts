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

  return (await response.json()) as T;
}

export function fetchCorridorsMap(
  token?: string | null
): Promise<CorridorMapFeatureCollection> {
  return requestJson<CorridorMapFeatureCollection>("/corridors/map", {
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
