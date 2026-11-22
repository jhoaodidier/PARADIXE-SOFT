import type { components } from "../api/schema.d.ts";

export type CorridorSegmentFeature = components["schemas"]["CorridorSegmentFeature"];
export type SyncChangeItem = components["schemas"]["SyncChangeItem"];
export type ProposedPassability = components["schemas"]["ProposedPassability"];

export interface SyncMetaRecord {
  key: "singleton";
  last_seq: number;
  last_synced_at: string | null;
}

export interface OutboxReportRecord {
  client_uuid: string;
  segment_id: string;
  proposed_passability: ProposedPassability;
  latitude: number;
  longitude: number;
  captured_at_client: string;
  photo_uri: string | null;
  status: "QUEUED" | "SYNCED" | "CONFLICT" | "REJECTED";
}

export interface EphemeralTelemetryRecord {
  key: "daily_ephemeral";
  ephemeral_id_date_utc: string;
  client_ephemeral_id: string;
}

const DB_NAME = "paradixe_soft_offline_v1";
const DB_VERSION = 1;

export function openOfflineDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains("sync_meta")) {
        db.createObjectStore("sync_meta", { keyPath: "key" });
      }
      if (!db.objectStoreNames.contains("corridors")) {
        db.createObjectStore("corridors", { keyPath: "properties.segment_id" });
      }
      if (!db.objectStoreNames.contains("outbox_reports")) {
        db.createObjectStore("outbox_reports", { keyPath: "client_uuid" });
      }
      if (!db.objectStoreNames.contains("telemetry_ephemeral")) {
        db.createObjectStore("telemetry_ephemeral", { keyPath: "key" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

function txDone(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error("IndexedDB transaction aborted"));
  });
}

function reqPromise<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function getSyncMeta(): Promise<SyncMetaRecord> {
  const db = await openOfflineDb();
  try {
    const tx = db.transaction("sync_meta", "readonly");
    const rec = await reqPromise<SyncMetaRecord | undefined>(
      tx.objectStore("sync_meta").get("singleton")
    );
    await txDone(tx);
    return rec ?? { key: "singleton", last_seq: 0, last_synced_at: null };
  } finally {
    db.close();
  }
}

export async function getCachedCorridors(): Promise<CorridorSegmentFeature[]> {
  const db = await openOfflineDb();
  try {
    const tx = db.transaction("corridors", "readonly");
    const items = await reqPromise<CorridorSegmentFeature[]>(
      tx.objectStore("corridors").getAll()
    );
    await txDone(tx);
    return items;
  } finally {
    db.close();
  }
}

export async function replaceCorridorCache(
  features: CorridorSegmentFeature[],
  syncedAt: string
): Promise<void> {
  const db = await openOfflineDb();
  try {
    const tx = db.transaction(["corridors", "sync_meta"], "readwrite");
    const corridorStore = tx.objectStore("corridors");
    corridorStore.clear();
    for (const feature of features) {
      corridorStore.put(feature);
    }
    const metaStore = tx.objectStore("sync_meta");
    const existing = await reqPromise<SyncMetaRecord | undefined>(metaStore.get("singleton"));
    metaStore.put({
      key: "singleton",
      last_seq: existing?.last_seq ?? 0,
      last_synced_at: syncedAt,
    } satisfies SyncMetaRecord);
    await txDone(tx);
  } finally {
    db.close();
  }
}

/**
 * Aplica cambios incrementales de /sync/snapshot y actualiza el cursor
 * last_seq dentro de UNA UNICA transaccion atomica en IndexedDB.
 */
export async function applySyncSnapshotAtomic(
  changes: SyncChangeItem[],
  nextSinceVersion: number,
  syncedAt: string
): Promise<void> {
  const db = await openOfflineDb();
  try {
    const tx = db.transaction(["corridors", "sync_meta"], "readwrite");
    const corridorStore = tx.objectStore("corridors");
    const metaStore = tx.objectStore("sync_meta");

    for (const change of changes) {
      if (change.entity_type === "ROAD_SEGMENT") {
        if (change.operation === "DELETE") {
          corridorStore.delete(change.entity_id);
        } else if (change.operation === "UPSERT" && change.payload_json) {
          const payload = change.payload_json as unknown as CorridorSegmentFeature;
          if (payload?.properties?.segment_id) {
            corridorStore.put(payload);
          }
        }
      }
    }

    const prevMeta = await reqPromise<SyncMetaRecord | undefined>(metaStore.get("singleton"));
    const updatedSeq = changes.length > 0 ? nextSinceVersion : (prevMeta?.last_seq ?? nextSinceVersion);
    metaStore.put({
      key: "singleton",
      last_seq: updatedSeq,
      last_synced_at: syncedAt,
    } satisfies SyncMetaRecord);

    await txDone(tx);
  } finally {
    db.close();
  }
}

export async function enqueueFieldReport(input: {
  segment_id: string;
  proposed_passability: ProposedPassability;
  latitude: number;
  longitude: number;
  photo_uri?: string | null;
}): Promise<OutboxReportRecord> {
  const record: OutboxReportRecord = {
    client_uuid: crypto.randomUUID(),
    segment_id: input.segment_id,
    proposed_passability: input.proposed_passability,
    latitude: input.latitude,
    longitude: input.longitude,
    captured_at_client: new Date().toISOString(),
    photo_uri: input.photo_uri ?? null,
    status: "QUEUED",
  };
  const db = await openOfflineDb();
  try {
    const tx = db.transaction("outbox_reports", "readwrite");
    tx.objectStore("outbox_reports").put(record);
    await txDone(tx);
    return record;
  } finally {
    db.close();
  }
}

export async function getQueuedReports(): Promise<OutboxReportRecord[]> {
  const db = await openOfflineDb();
  try {
    const tx = db.transaction("outbox_reports", "readonly");
    const all = await reqPromise<OutboxReportRecord[]>(tx.objectStore("outbox_reports").getAll());
    await txDone(tx);
    return all.filter((r) => r.status === "QUEUED");
  } finally {
    db.close();
  }
}

export async function updateOutboxStatuses(
  updates: Array<{ client_uuid: string; status: OutboxReportRecord["status"] }>
): Promise<void> {
  const db = await openOfflineDb();
  try {
    const tx = db.transaction("outbox_reports", "readwrite");
    const store = tx.objectStore("outbox_reports");
    for (const u of updates) {
      if (u.status === "SYNCED") {
        store.delete(u.client_uuid);
      } else {
        const existing = await reqPromise<OutboxReportRecord | undefined>(store.get(u.client_uuid));
        if (existing) {
          store.put({ ...existing, status: u.status });
        }
      }
    }
    await txDone(tx);
  } finally {
    db.close();
  }
}

/**
 * Rota client_ephemeral_id diariamente segun fecha UTC sin almacenar user_id.
 */
export async function getOrRotateDailyEphemeralId(
  now: Date = new Date()
): Promise<EphemeralTelemetryRecord> {
  const todayUtc = now.toISOString().slice(0, 10);
  const db = await openOfflineDb();
  try {
    const tx = db.transaction("telemetry_ephemeral", "readwrite");
    const store = tx.objectStore("telemetry_ephemeral");
    const existing = await reqPromise<EphemeralTelemetryRecord | undefined>(
      store.get("daily_ephemeral")
    );
    if (existing && existing.ephemeral_id_date_utc === todayUtc) {
      await txDone(tx);
      return existing;
    }
    const fresh: EphemeralTelemetryRecord = {
      key: "daily_ephemeral",
      ephemeral_id_date_utc: todayUtc,
      client_ephemeral_id: crypto.randomUUID(),
    };
    store.put(fresh);
    await txDone(tx);
    return fresh;
  } finally {
    db.close();
  }
}

/**
 * Determina STALE_DATA a partir de la vigencia real del calculo climatico (calculated_at + validityHours),
 * nunca a partir de last_synced_at.
 */
export function isAssessmentStale(
  calculatedAt: string | null | undefined,
  validityHours = 3,
  now: Date = new Date()
): boolean {
  if (!calculatedAt) return true;
  const calcMs = Date.parse(calculatedAt);
  if (Number.isNaN(calcMs)) return true;
  const expiresAtMs = calcMs + validityHours * 60 * 60 * 1000;
  return now.getTime() > expiresAtMs;
}
