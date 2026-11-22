import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

interface OpenApiDoc {
  openapi: string;
  paths: Record<string, Record<string, unknown>>;
  components: {
    schemas: Record<string, Record<string, unknown>>;
  };
}

describe("Contrato OpenAPI 3.1 PARADIXE SOFT", () => {
  const specPath = resolve(process.cwd(), "../../openapi/openapi-v1.json");
  const spec = JSON.parse(readFileSync(specPath, "utf-8")) as OpenApiDoc;

  it("declara version OpenAPI 3.1.0, las 22 rutas canonicas y las extensiones operativas de /api/v1", () => {
    expect(spec.openapi).toBe("3.1.0");
    const corePaths = [
      "/auth/otp/request",
      "/auth/login",
      "/auth/refresh",
      "/auth/logout",
      "/auth/revoke-all",
      "/sync/snapshot",
      "/sync/batch",
      "/corridors/map",
      "/ivv/recalculate",
      "/telemetry/pings",
      "/reports",
      "/webhooks/messaging",
      "/incidents/{incident_id}/verification",
      "/routing/calculate",
      "/bridges",
      "/bridges/{bridge_id}/status",
      "/incidents/{incident_id}/humanitarian-windows",
      "/simulator/scenarios",
      "/simulator/scenarios/{scenario_id}/events",
      "/simulator/scenarios/{scenario_id}/routing",
      "/simulator/scenarios/{scenario_id}/evaluate",
      "/simulator/scenarios/{scenario_id}/impact",
    ];
    const extendedPaths = [
      "/auth/register",
      "/auth/me",
      "/admin/users",
      "/regions",
      "/nodes",
      "/segments/{segment_id}/attributes",
      "/corridors/verify-attributes",
      "/corridors/forecast",
      "/analytics/summary",
      "/events/stream",
      "/uploads/presign",
      "/uploads/evidence",
      "/subscriptions",
      "/subscriptions/{subscription_id}",
      "/incidents",
      "/operations/bridges",
      "/transfer-corridors",
      "/transfer-corridors/{corridor_id}/inspect",
      "/routing/multi-stop",
      "/b2b/api-keys",
      "/b2b/api-keys/{api_key_id}",
      "/b2b/webhooks",
      "/b2b/webhooks/{webhook_id}",
      "/b2b/corridors/status",
    ];
    for (const path of [...corePaths, ...extendedPaths]) {
      expect(spec.paths[path]).toBeDefined();
    }
    expect(Object.keys(spec.paths).length).toBeGreaterThanOrEqual(46);
  });

  it("incluye los 11 reason_code de NO_SAFE_ROUTE y esquemas discriminados estrictos", () => {
    const reasonEnum = spec.components.schemas.NoSafeRouteReasonCode.enum as string[];
    expect(reasonEnum).toEqual([
      "UNVERIFIED_PHYSICAL_ATTRIBUTES",
      "STALE_IVV",
      "VEHICLE_RESTRICTION",
      "TRANSFER_REVALIDATION_REQUIRED",
      "TRANSFER_INSPECTION_EXPIRED",
      "HAZARD_INTERSECTS_TRANSFER_PATH",
      "TRANSFER_IVV_EXCEEDED",
      "TRANSFER_CARGO_CAPACITY_EXCEEDED",
      "TRANSFER_CARGO_NOT_ALLOWED",
      "NO_VERIFIED_TRANSFER_CORRIDOR",
      "NETWORK_DISCONNECTED",
    ]);

    const routingResp = spec.components.schemas.RoutingCalculateResponse as {
      oneOf: Array<{ $ref: string }>;
      discriminator: { propertyName: string; mapping: Record<string, string> };
    };
    expect(routingResp.discriminator.propertyName).toBe("resolution_type");
    expect(Object.keys(routingResp.discriminator.mapping)).toEqual([
      "ALTERNATE_ROUTE",
      "LOGISTIC_BRIDGE_REQUIRED",
      "NO_SAFE_ROUTE",
    ]);

    const simEventReq = spec.components.schemas.SimEventRequest as {
      discriminator: { propertyName: string; mapping: Record<string, string> };
    };
    expect(simEventReq.discriminator.propertyName).toBe("event_type");
    expect(Object.keys(simEventReq.discriminator.mapping)).toEqual([
      "WEATHER_OVERRIDE",
      "BLOCKAGE_OVERRIDE",
    ]);
    expect(spec.components.schemas.WeatherOverrideEvent.additionalProperties).toBe(false);
    expect(spec.components.schemas.BlockageOverrideEvent.additionalProperties).toBe(false);
  });
});
