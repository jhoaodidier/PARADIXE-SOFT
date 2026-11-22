import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "maplibre-gl/dist/maplibre-gl.css";
import {
  fetchAuthMe,
  fetchBridgesOperations,
  fetchCorridorsMap,
  fetchIncidents,
  fetchRegions,
  fetchSyncSnapshot,
  getOrCreateDeviceId,
  postAuthLogout,
  postAuthRefresh,
  postSyncBatch,
  type AuthMeResponse,
  type AuthTokenResponse,
  type BridgesOperationsResponse,
  type CorridorSegmentFeature,
  type IncidentListItem,
  type TerritorialUnit,
} from "./api/client.ts";
import { AuthModal } from "./components/AuthModal.tsx";
import { BridgesPanel } from "./components/BridgesPanel.tsx";
import { CorridorMap } from "./components/CorridorMap.tsx";
import { FieldReportModal } from "./components/FieldReportModal.tsx";
import { IncidentsPanel } from "./components/IncidentsPanel.tsx";
import { OperationalTabs, type TabDefinition } from "./components/OperationalTabs.tsx";
import { RegionSelectorBar } from "./components/RegionSelectorBar.tsx";
import { RiskBadge } from "./components/RiskBadge.tsx";
import { RoutingPanel } from "./components/RoutingPanel.tsx";
import { SettingsModal } from "./components/SettingsModal.tsx";
import { SimulatorPanel } from "./components/SimulatorPanel.tsx";
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
import { ensureFocusgroupPolyfill } from "./utils/focusgroup.ts";
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

function readStoredToken(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStoredToken(key: string, value: string | null): void {
  try {
    if (value === null) {
      localStorage.removeItem(key);
    } else {
      localStorage.setItem(key, value);
    }
  } catch {
    // Ignore storage errors inside restricted contexts
  }
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
  const [authModalOpen, setAuthModalOpen] = useState(false);
  const [toasts, setToasts] = useState<ToastMessage[]>([]);

  const [themePreference, setThemePreference] = useState<ThemePreference>(() =>
    readStoredThemePreference()
  );
  const [systemScheme, setSystemScheme] = useState<"light" | "dark">(() =>
    getSystemResolvedScheme()
  );

  const [authToken, setAuthToken] = useState<string | null>(() =>
    readStoredToken("paradixe_access_token")
  );
  const [refreshToken, setRefreshToken] = useState<string | null>(() =>
    readStoredToken("paradixe_refresh_token")
  );
  const [currentUser, setCurrentUser] = useState<AuthMeResponse | null>(null);

  const [territorialUnits, setTerritorialUnits] = useState<TerritorialUnit[]>([]);
  const [selectedRegionCode, setSelectedRegionCode] = useState<string>("PE-HUC");
  const [selectedProvinceUnitId, setSelectedProvinceUnitId] = useState<string>("");
  const [selectedDistrictUnitId, setSelectedDistrictUnitId] = useState<string>("");
  const [loadingMap, setLoadingMap] = useState(false);

  const [incidents, setIncidents] = useState<IncidentListItem[]>([]);
  const [bridgesOperations, setBridgesOperations] = useState<BridgesOperationsResponse | null>(
    null
  );
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const pushToast = useCallback((text: string) => {
    setToasts((prev) => [...prev.slice(-2), { id: crypto.randomUUID(), text }]);
  }, []);

  const dismissToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const clearSession = useCallback(() => {
    writeStoredToken("paradixe_access_token", null);
    writeStoredToken("paradixe_refresh_token", null);
    setAuthToken(null);
    setRefreshToken(null);
    setCurrentUser(null);
  }, []);

  const refreshLocalState = useCallback(async () => {
    const [cached, meta, queued, ephemeral] = await Promise.all([
      getCachedCorridors(),
      getSyncMeta(),
      getQueuedReports(),
      getOrRotateDailyEphemeralId(),
    ]);
    if (!mountedRef.current) return;
    if (cached.length > 0) {
      setFeatures(cached);
      setSelectedSegmentId((prev) => prev ?? cached[0]?.properties.segment_id ?? null);
    }
    setSyncMeta(meta);
    setQueuedReports(queued);
    setEphemeralRecord(ephemeral);
  }, []);

  const refreshOperations = useCallback(async () => {
    if (typeof navigator !== "undefined" && !navigator.onLine) return;
    try {
      const [incResp, bridgeResp] = await Promise.all([
        fetchIncidents({ limit: 100 }),
        fetchBridgesOperations(),
      ]);
      if (!mountedRef.current) return;
      if (incResp?.incidents) {
        setIncidents(incResp.incidents);
      }
      if (bridgeResp) {
        setBridgesOperations(bridgeResp);
      }
    } catch {
      // Silencio seguro en modo offline o respuestas simuladas
    }
  }, []);

  const selectedUnitForBbox = useMemo(() => {
    if (selectedDistrictUnitId) {
      return territorialUnits.find((u) => u.unit_id === selectedDistrictUnitId) ?? null;
    }
    if (selectedProvinceUnitId) {
      return territorialUnits.find((u) => u.unit_id === selectedProvinceUnitId) ?? null;
    }
    return null;
  }, [territorialUnits, selectedProvinceUnitId, selectedDistrictUnitId]);

  const activeRegionUnit = useMemo(() => {
    return (
      territorialUnits.find(
        (u) => u.admin_level === 4 && (u.iso_code ?? u.ubigeo_code) === selectedRegionCode
      ) ?? null
    );
  }, [territorialUnits, selectedRegionCode]);

  const syncWithServer = useCallback(async () => {
    if (typeof navigator !== "undefined" && !navigator.onLine) return;
    setLoadingMap(true);
    try {
      const regResp = await fetchRegions().catch(() => null);
      if (!mountedRef.current) return;
      if (regResp?.regions && regResp.regions.length > 0) {
        setTerritorialUnits(regResp.regions);
      }

      const bboxParam = selectedUnitForBbox
        ? `${selectedUnitForBbox.min_lon},${selectedUnitForBbox.min_lat},${selectedUnitForBbox.max_lon},${selectedUnitForBbox.max_lat}`
        : undefined;

      const mapData = await fetchCorridorsMap(authToken, {
        region: selectedRegionCode,
        bbox: bboxParam,
        limit: 2500,
      });

      const nowIso = new Date().toISOString();
      await replaceCorridorCache(mapData.features, nowIso);
      if (!mountedRef.current) return;
      setFeatures(mapData.features);
      if (mapData.features.length > 0) {
        setSelectedSegmentId((prev) =>
          prev && mapData.features.some((f) => f.properties.segment_id === prev)
            ? prev
            : mapData.features[0].properties.segment_id
        );
      }

      await refreshOperations();
      if (!mountedRef.current) return;

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
          if (mountedRef.current) {
            pushToast("Cola offline sincronizada correctamente con el servidor.");
          }
        }
      }
      await refreshLocalState();
    } catch {
      // Mantiene lectura de cache local si el backend no esta accesible
    } finally {
      if (mountedRef.current) {
        setLoadingMap(false);
      }
    }
  }, [
    authToken,
    pushToast,
    refreshLocalState,
    refreshOperations,
    selectedRegionCode,
    selectedUnitForBbox,
  ]);

  useEffect(() => {
    ensurePopoverPolyfill();
    ensureFocusgroupPolyfill();
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
    let cancelled = false;
    async function restoreSession() {
      if (!authToken) {
        setCurrentUser(null);
        return;
      }
      try {
        const me = await fetchAuthMe(authToken);
        if (!cancelled) setCurrentUser(me);
      } catch {
        if (refreshToken) {
          try {
            const refreshed = await postAuthRefresh(refreshToken, getOrCreateDeviceId());
            if (cancelled) return;
            writeStoredToken("paradixe_access_token", refreshed.access_token);
            writeStoredToken("paradixe_refresh_token", refreshed.refresh_token);
            setAuthToken(refreshed.access_token);
            setRefreshToken(refreshed.refresh_token);
            const me = await fetchAuthMe(refreshed.access_token);
            if (!cancelled) setCurrentUser(me);
            return;
          } catch {
            // Token invalido o revocado
          }
        }
        if (!cancelled) clearSession();
      }
    }
    void restoreSession();
    return () => {
      cancelled = true;
    };
  }, [authToken, refreshToken, clearSession]);

  useEffect(() => {
    void refreshLocalState().then(() => {
      if (mountedRef.current) {
        void syncWithServer();
      }
    });

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

  const handleAuthenticated = (tokens: AuthTokenResponse, user: AuthMeResponse) => {
    writeStoredToken("paradixe_access_token", tokens.access_token);
    writeStoredToken("paradixe_refresh_token", tokens.refresh_token);
    setAuthToken(tokens.access_token);
    setRefreshToken(tokens.refresh_token);
    setCurrentUser(user);
    pushToast(`Sesión iniciada: ${user.full_name} (${user.role_code}).`);
  };

  const handleLogout = async () => {
    if (authToken && refreshToken) {
      await postAuthLogout(refreshToken, getOrCreateDeviceId(), authToken).catch(() => {});
    }
    clearSession();
    pushToast("Sesión institucional cerrada.");
  };

  const handleSelectRegion = (code: string) => {
    setSelectedRegionCode(code);
    setSelectedProvinceUnitId("");
    setSelectedDistrictUnitId("");
  };

  const handleSelectProvince = (unitId: string) => {
    setSelectedProvinceUnitId(unitId);
    setSelectedDistrictUnitId("");
  };

  const hasStaleData = features.some(
    (f) => f.properties.is_stale || isAssessmentStale(f.properties.calculated_at)
  );
  const selectedFeature =
    features.find((f) => f.properties.segment_id === selectedSegmentId) ?? features[0] ?? null;

  const focusBounds = useMemo<[[number, number], [number, number]] | null>(() => {
    const unit = selectedUnitForBbox ?? activeRegionUnit;
    if (!unit) return null;
    if (unit.min_lon >= unit.max_lon || unit.min_lat >= unit.max_lat) return null;
    return [
      [unit.min_lon, unit.min_lat],
      [unit.max_lon, unit.max_lat],
    ];
  }, [selectedUnitForBbox, activeRegionUnit]);

  const resolvedScheme = themePreference === "system" ? systemScheme : themePreference;
  const activeRegionLabel =
    selectedUnitForBbox?.name ?? activeRegionUnit?.name ?? `Región ${selectedRegionCode}`;

  const operationalTabs: TabDefinition[] = [
    {
      id: "routing",
      label: "Enrutamiento",
      content: <RoutingPanel features={features} isOnline={isOnline} authToken={authToken} />,
    },
    {
      id: "incidents",
      label: "Incidentes",
      badge: incidents.filter(
        (i) => i.verification_state === "SUSPECTED" || i.verification_state === "CONFIRMED"
      ).length,
      content: (
        <IncidentsPanel
          incidents={incidents}
          isOnline={isOnline}
          authToken={authToken}
          userRole={currentUser?.role_code ?? null}
          onOpenReportModal={() => setReportModalOpen(true)}
          onOpenAuthModal={() => setAuthModalOpen(true)}
          onRefreshOperations={refreshOperations}
          onToast={pushToast}
        />
      ),
    },
    {
      id: "bridges",
      label: "Puentes y Ventanas",
      badge: bridgesOperations?.bridges.length ?? 0,
      content: (
        <BridgesPanel
          operations={bridgesOperations}
          incidents={incidents}
          isOnline={isOnline}
          authToken={authToken}
          userRole={currentUser?.role_code ?? null}
          onOpenAuthModal={() => setAuthModalOpen(true)}
          onRefreshOperations={refreshOperations}
          onToast={pushToast}
        />
      ),
    },
    {
      id: "simulator",
      label: "Simulador",
      content: (
        <SimulatorPanel
          isOnline={isOnline}
          authToken={authToken}
          userRole={currentUser?.role_code ?? null}
          onOpenAuthModal={() => setAuthModalOpen(true)}
          onToast={pushToast}
        />
      ),
    },
  ];

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
          {currentUser ? (
            <div className="auth-chip">
              <span>
                {currentUser.full_name} · <strong>{currentUser.role_code}</strong>
              </span>
              <button type="button" onClick={() => void handleLogout()}>
                Salir
              </button>
            </div>
          ) : (
            <button type="button" onClick={() => setAuthModalOpen(true)}>
              Iniciar sesión
            </button>
          )}
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
        <main id="main-content" tabIndex={-1} className="main-column">
          <RegionSelectorBar
            units={territorialUnits}
            selectedRegionCode={selectedRegionCode}
            selectedProvinceUnitId={selectedProvinceUnitId}
            selectedDistrictUnitId={selectedDistrictUnitId}
            onSelectRegion={handleSelectRegion}
            onSelectProvince={handleSelectProvince}
            onSelectDistrict={setSelectedDistrictUnitId}
            features={features}
            loading={loadingMap}
            pmtilesActiveUrl={activeRegionUnit?.pmtiles_uri ?? "/tiles/pe-huc.pmtiles"}
          />

          <CorridorMap
            features={features}
            selectedSegmentId={selectedSegmentId}
            onSelectSegment={setSelectedSegmentId}
            pmtilesUrl={activeRegionUnit?.pmtiles_uri}
            focusBounds={focusBounds}
            resolvedScheme={resolvedScheme}
            regionLabel={activeRegionLabel}
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

          <OperationalTabs tabs={operationalTabs} defaultTabId="routing" />
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
          void refreshOperations();
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

      <AuthModal
        open={authModalOpen}
        onClose={() => setAuthModalOpen(false)}
        onAuthenticated={handleAuthenticated}
      />

      <ToastRegion
        toasts={toasts}
        modalOpen={reportModalOpen || settingsModalOpen || authModalOpen}
        onDismiss={dismissToast}
      />
    </div>
  );
}

export default App;
