import { describe, expect, it } from "vitest";
import type { CorridorSegmentFeature } from "../api/client.ts";
import {
  applySyncSnapshotAtomic,
  enqueueFieldReport,
  getCachedCorridors,
  getOrRotateDailyEphemeralId,
  getQueuedReports,
  getSyncMeta,
  isAssessmentStale,
} from "../offline/db.ts";

function sampleFeature(segmentId: string, name: string): CorridorSegmentFeature {
  return {
    type: "Feature",
    geometry: {
      type: "LineString",
      coordinates: [
        [-76.24, -9.93],
        [-76.09, -9.71],
      ],
    },
    properties: {
      segment_id: segmentId,
      origin_node_id: "11111111-1111-4111-8111-111111111111",
      destination_node_id: "22222222-2222-4222-8222-222222222222",
      corridor_name: name,
      direction_type: "BIDIRECTIONAL",
      length_km: 38.4,
      surface_type: "PAVED",
      passability_state: "OPEN",
      ivv_score: 34.5,
      risk_level: "GREEN",
      observed_at: "2026-11-21T20:00:00Z",
      calculated_at: "2026-11-21T20:05:00Z",
      model_version_tag: "ivv-v1.0-initial",
      is_stale: false,
      is_attributes_verified: true,
    },
  };
}

describe("Capa Offline IndexedDB PARADIXE SOFT", () => {
  it("aplica UPSERT y DELETE con cursor de sync_control en una sola transaccion", async () => {
    const segA = "aaaa1111-1111-4111-8111-111111111111";
    const segB = "bbbb2222-2222-4222-8222-222222222222";

    await applySyncSnapshotAtomic(
      [
        {
          change_seq: 1,
          entity_type: "ROAD_SEGMENT",
          entity_id: segA,
          operation: "UPSERT",
          changed_at: "2026-11-21T21:00:00Z",
          payload_json: sampleFeature(segA, "Huánuco – Ambo") as unknown as Record<string, unknown>,
        },
        {
          change_seq: 2,
          entity_type: "ROAD_SEGMENT",
          entity_id: segB,
          operation: "UPSERT",
          changed_at: "2026-11-21T21:01:00Z",
          payload_json: sampleFeature(segB, "Túnel Carpish") as unknown as Record<string, unknown>,
        },
        {
          change_seq: 3,
          entity_type: "ROAD_SEGMENT",
          entity_id: segA,
          operation: "DELETE",
          changed_at: "2026-11-21T21:02:00Z",
          payload_json: null,
        },
      ],
      3,
      "2026-11-21T21:03:00Z"
    );

    const corridors = await getCachedCorridors();
    const meta = await getSyncMeta();
    expect(corridors).toHaveLength(1);
    expect(corridors[0].properties.segment_id).toBe(segB);
    expect(meta.last_seq).toBe(3);
  });

  it("encola reportes offline con client_uuid unico en outbox_reports", async () => {
    const rec = await enqueueFieldReport({
      segment_id: "bbbb2222-2222-4222-8222-222222222222",
      proposed_passability: "BLOCKED",
      latitude: -9.712,
      longitude: -76.091,
    });
    const queued = await getQueuedReports();
    expect(queued.some((q) => q.client_uuid === rec.client_uuid)).toBe(true);
  });

  it("detecta STALE_DATA segun vigencia de 3 horas y rota ID efimero GPS por dia UTC", async () => {
    const now = new Date("2026-11-21T23:30:00Z");
    expect(isAssessmentStale("2026-11-21T21:00:00Z", 3, now)).toBe(false);
    expect(isAssessmentStale("2026-11-21T20:00:00Z", 3, now)).toBe(true);
    expect(isAssessmentStale(null, 3, now)).toBe(true);

    const day1First = await getOrRotateDailyEphemeralId(new Date("2026-11-21T10:00:00Z"));
    const day1Second = await getOrRotateDailyEphemeralId(new Date("2026-11-21T22:00:00Z"));
    expect(day1Second.client_ephemeral_id).toBe(day1First.client_ephemeral_id);

    const day2 = await getOrRotateDailyEphemeralId(new Date("2026-11-22T00:05:00Z"));
    expect(day2.ephemeral_id_date_utc).toBe("2026-11-22");
    expect(day2.client_ephemeral_id).not.toBe(day1First.client_ephemeral_id);
  });
});
