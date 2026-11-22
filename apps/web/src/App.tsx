import { useCallback, useEffect, useState } from "react";
import "maplibre-gl/dist/maplibre-gl.css";
import {
  fetchCorridorsMap,
  fetchSyncSnapshot,
  postSyncBatch,
  type CorridorSegmentFeature,
} from "./api/client.ts";
import { CorridorMap } from "./components/CorridorMap.tsx";
import { FieldReportModal } from "./components/FieldReportModal.tsx";
import { RiskBadge } from "./components/RiskBadge.tsx";
import { RoutingPanel } from "./components/RoutingPanel.tsx";
import { SettingsModal } from "./components/SettingsModal.tsx";
import { ToastRegion, type ToastMessage } from "./components/ToastRegion.tsx";
import {
  applySyncSnapshotAtomic,
  getCachedCorridors,
  getOrRotateDailyEphemeralId,
  getQueuedReports,
  getSyncMeta,
  isAssessmentStale,
  replaceCorridorCache,
  updateOutboxStatuses,
  type EphemeralTelemetryRecord,
  type OutboxReportRecord,
  type SyncMetaRecord,
} from "./offline/db.ts";
import { initFormAriaSync } from "./utils/formAriaSync.ts";
import {
  applyThemePreference,
  getSystemResolvedScheme,
  readStoredThemePreference,
  subscribeThemeChanges,
  type ThemePreference,
} from "./utils/theme.ts";
import { ensurePopoverPolyfill } from "./utils/topLayer.ts";

export interface AppProps {
  initialFeatures?: CorridorSegmentFeature[];
}

export function App({ initialFeatures }: AppProps) {
  const [features, setFeatures] = useState<CorridorSegmentFeature[]>(initialFeatures ?? []);
  const [selectedSegmentId, setSelectedSegmentId] = useState<string | null>(
    initialFeatures?.[0]?.properties.segment_id ?? null
  );
  const [isOnline, setIsOnline] = useState<boolean>(
    typeof navigator !== "undefined" ? navigator.onLine : true
  );
  const [syncMeta, setSyncMeta] = useState<SyncMetaRecord>({
    key: "singleton",
    last_seq: 0,
    last_synced_at: null,
  });
  const [queuedReports, setQueuedReports] = useState<OutboxReportRecord[]>([]);
  const [ephemeralRecord, setEphemeralRecord] = useState<EphemeralTelemetryRecord | null>(null);
  const [reportModalOpen, setReportModalOpen] = useState(false);
  const [settingsModalOpen, setSettingsModalOpen] = useState(false);
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const [themePreference, setThemePreference] = useState<ThemePreference>(() =>
    readStoredThemePreference()
  );
  const [systemScheme, setSystemScheme] = useState<"light" | "dark">(() =>
    getSystemResolvedScheme()
  );
  const [authToken] = useState<string | null>(() => {
    try {
      return localStorage.getItem("paradixe_access_token");
    } catch {
      return null;
    }
  });

  const pushToast = useCallback((text: string) => {
    setToasts((prev) => [...prev.slice(-2), { id: crypto.randomUUID(), text }]);
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const refreshLocalState = useCallback(async () => {
    const [cached, meta, queued, ephemeral] = await Promise.all([
      getCachedCorridors(),
      getSyncMeta(),
      getQueuedReports(),
      getOrRotateDailyEphemeralId(),
    ]);
    if (cached.length > 0) {
      setFeatures(cached);
      setSelectedSegmentId((prev) => prev ?? cached[0]?.properties.segment_id ?? null);
    }
    setSyncMeta(meta);
    setQueuedReports(queued);
    setEphemeralRecord(ephemeral);
  }, []);

  const syncWithServer = useCallback(async () => {
    if (typeof navigator !== "undefined" && !navigator.onLine) return;
    try {
      const mapData = await fetchCorridorsMap(authToken);
      const nowIso = new Date().toISOString();
      await replaceCorridorCache(mapData.features, nowIso);
      setFeatures(mapData.features);

      if (authToken) {
        const meta = await getSyncMeta();
        const snap = await fetchSyncSnapshot(meta.last_seq, 500, authToken);
        await applySyncSnapshotAtomic(snap.changes, snap.next_since_version, snap.server_time);

        const pending = await getQueuedReports();
        if (pending.length > 0) {
          const batchResp = await postSyncBatch(
            {
              reports: pending.map((r) => ({
                client_uuid: r.client_uuid,
                segment_id: r.segment_id,
                proposed_passability: r.proposed_passability,
                latitude: r.latitude,
                longitude: r.longitude,
                captured_at_client: r.captured_at_client,
                photo_uri: r.photo_uri,
              })),
              telemetry_batches: [],
            },
            authToken
          );
          await updateOutboxStatuses(
            batchResp.report_results.map((res) => ({
              client_uuid: res.client_id,
              status:
                res.status === "CREATED" || res.status === "DUPLICATE_IGNORED"
                  ? "SYNCED"
                  : "REJECTED",
            }))
          );
          pushToast("Cola offline sincronizada correctamente con el servidor.");
        }
      }
      await refreshLocalState();
    } catch {
      // Mantiene lectura de cache local si el backend no esta accesible
    }
  }, [authToken, pushToast, refreshLocalState]);

  useEffect(() => {
    ensurePopoverPolyfill();
    const cleanupAria = initFormAriaSync(document);
    const unsubscribeTheme = subscribeThemeChanges((pref, sys) => {
      setThemePreference(pref);
      setSystemScheme(sys);
    });
    return () => {
      cleanupAria();
      unsubscribeTheme();
    };
  }, []);

  useEffect(() => {
    void refreshLocalState().then(() => syncWithServer());

    const onOnline = () => {
      setIsOnline(true);
      void syncWithServer();
    };
    const onOffline = () => {
      setIsOnline(false);
    };

    window.addEventListener("online", onOnline);
    window.addEventListener("offline", onOffline);
    return () => {
      window.removeEventListener("online", onOnline);
      window.removeEventListener("offline", onOffline);
    };
  }, [refreshLocalState, syncWithServer]);

  const handleThemeChange = (pref: ThemePreference) => {
    applyThemePreference(pref);
    setThemePreference(pref);
  };

  const hasStaleData = features.some(
    (f) => f.properties.is_stale || isAssessmentStale(f.properties.calculated_at)
  );
  const selectedFeature =
    features.find((f) => f.properties.segment_id === selectedSegmentId) ?? features[0] ?? null;

  return (
    <div className="app-shell">
      <a href="#main-content" className="skip-link visually-hidden">
        Ir al contenido principal
      </a>

      <header className="app-header">
        <div className="brand-group">
          <h1 className="brand-title">PARADIXE SOFT</h1>
          <p className="brand-subtitle">
            Red Territorial de Resiliencia Vial y Logística Crítica — TakumiSoft
          </p>
        </div>

        <nav className="primary-nav" aria-label="Acciones operativas principales">
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => setReportModalOpen(true)}
          >
            Reportar vía
          </button>
          <button type="button" onClick={() => void syncWithServer()} disabled={!isOnline}>
            Sincronizar ({syncMeta.last_seq})
          </button>
          <button type="button" onClick={() => setSettingsModalOpen(true)}>
            Configuración
          </button>
        </nav>
      </header>

      <div className="status-banners">
        {!isOnline && (
          <div className="banner banner--warning" role="alert">
            <span>
              <strong>Modo Sin Conexión (Offline-First):</strong> visualizando red en caché
              IndexedDB. Los reportes se encolan localmente; el cálculo de nuevas rutas requiere
              conexión.
            </span>
          </div>
        )}

        {hasStaleData && (
          <div className="banner banner--danger" role="alert">
            <span>
              <strong>STALE_DATA:</strong> Existen tramos con índice IVV vencido (&gt; 3 h) o sin
              observación vigente. Ningún dato vencido autoriza paso operativo.
            </span>
          </div>
        )}

        {queuedReports.length > 0 && (
          <div className="banner banner--info" role="status">
            <span>
              <strong>Cola outbox_queue:</strong> {queuedReports.length} reporte(s) pendiente(s) de
              envío al servidor.
            </span>
            <button type="button" onClick={() => void syncWithServer()} disabled={!isOnline}>
              Sincronizar ahora
            </button>
          </div>
        )}
      </div>

      <div className="workspace">
        <main id="main-content" tabIndex={-1}>
          <CorridorMap
            features={features}
            selectedSegmentId={selectedSegmentId}
            onSelectSegment={setSelectedSegmentId}
          />
        </main>

        <aside className="side-panel" aria-label="Panel operativo de tramo y enrutamiento">
          {selectedFeature && (
            <section className="card" aria-labelledby="selected-segment-title">
              <h2 id="selected-segment-title">Detalle del Tramo Seleccionado</h2>
              <p>
                <strong>{selectedFeature.properties.corridor_name}</strong>
              </p>
              <RiskBadge
                riskLevel={selectedFeature.properties.risk_level}
                ivvScore={selectedFeature.properties.ivv_score}
                isStale={selectedFeature.properties.is_stale}
              />
              <div className="metrics-grid" style={{ marginBlockStart: "0.75rem" }}>
                <div className="metric-box">
                  <span className="metric-label">Observación climática</span>
                  <span className="metric-value">
                    {selectedFeature.properties.observed_at
                      ? new Date(selectedFeature.properties.observed_at).toLocaleString("es-PE")
                      : "Sin dato"}
                  </span>
                </div>
                <div className="metric-box">
                  <span className="metric-label">Modelo IVV</span>
                  <span className="metric-value">
                    {selectedFeature.properties.model_version_tag ?? "No calculado"}
                  </span>
                </div>
              </div>
              <p className="hint" style={{ marginBlockStart: "0.75rem" }}>
                {selectedFeature.properties.is_attributes_verified
                  ? "Atributos físicos validados en campo."
                  : "Atributos físicos no verificados en campo (no autorizan despacho)."}
              </p>
            </section>
          )}

          <RoutingPanel features={features} isOnline={isOnline} authToken={authToken} />
        </aside>
      </div>

      <FieldReportModal
        open={reportModalOpen}
        onClose={() => setReportModalOpen(false)}
        features={features}
        defaultSegmentId={selectedSegmentId}
        isOnline={isOnline}
        authToken={authToken}
        onQueuedOrSent={(msg) => {
          pushToast(msg);
          void refreshLocalState();
        }}
      />

      <SettingsModal
        open={settingsModalOpen}
        onClose={() => setSettingsModalOpen(false)}
        themePreference={themePreference}
        systemScheme={systemScheme}
        onChangeTheme={handleThemeChange}
        syncMeta={syncMeta}
        queuedCount={queuedReports.length}
        ephemeralRecord={ephemeralRecord}
      />

      <ToastRegion
        toasts={toasts}
        modalOpen={reportModalOpen || settingsModalOpen}
        onDismiss={dismissToast}
      />
    </div>
  );
}

export default App;
