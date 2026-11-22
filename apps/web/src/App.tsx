import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import "maplibre-gl/dist/maplibre-gl.css";
import {
  fetchAuthMe,
  fetchBridgesOperations,
  fetchCorridorsMap,
  fetchIncidents,
  fetchNodes,
  fetchRegions,
  fetchSyncSnapshot,
  getOrCreateDeviceId,
  patchSegmentAttributes,
  postAuthLogout,
  postAuthRefresh,
  postSyncBatch,
  postVerifyCorridorAttributes,
  type AuthMeResponse,
  type AuthTokenResponse,
  type BridgesOperationsResponse,
  type CorridorSegmentFeature,
  type IncidentListItem,
  type RoadNodeItem,
  type TerritorialUnit,
} from "./api/client.ts";
import { AuthModal } from "./components/AuthModal.tsx";
import { BridgesPanel } from "./components/BridgesPanel.tsx";
import {
  CorridorMap,
  type FlyToTarget,
  type RouteOverlayData,
} from "./components/CorridorMap.tsx";
import { FieldReportModal } from "./components/FieldReportModal.tsx";
import {
  IconAlert,
  IconBridge,
  IconChevronLeft,
  IconChevronRight,
  IconCompass,
  IconHospital,
  IconLogout,
  IconMoon,
  IconPin,
  IconPlus,
  IconRefresh,
  IconRoute,
  IconSearch,
  IconSettings,
  IconSparkles,
  IconSun,
  IconUser,
  IconX,
} from "./components/Icons.tsx";
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

const SURFACE_HUMAN: Record<
  CorridorSegmentFeature["properties"]["surface_type"],
  string
> = {
  PAVED: "Asfaltado",
  GRAVEL: "Afirmado",
  DIRT_TRACK: "Trocha",
};

const PASSABILITY_HUMAN: Record<
  CorridorSegmentFeature["properties"]["passability_state"],
  string
> = {
  OPEN: "Vía libre",
  CAUTION_SUSPECTED: "Precaución",
  PARTIAL_ONE_LANE: "Paso 1 carril",
  BLOCKED: "Bloqueado",
};

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
    // Ignorar en contextos sin storage
  }
}

export function App({ initialFeatures }: AppProps) {
  const searchInputId = useId();

  const [features, setFeatures] = useState<CorridorSegmentFeature[]>(initialFeatures ?? []);
  const [selectedSegmentId, setSelectedSegmentId] = useState<string | null>(
    initialFeatures?.[0]?.properties.segment_id ?? null
  );
  const [inspectorOpen, setInspectorOpen] = useState<boolean>(
    Boolean(initialFeatures && initialFeatures.length > 0)
  );
  const [drawerOpen, setDrawerOpen] = useState<boolean>(true);
  const [activeTabId, setActiveTabId] = useState<string>("routing");

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
  const [roadNodes, setRoadNodes] = useState<RoadNodeItem[]>([]);
  const [selectedRegionCode, setSelectedRegionCode] = useState<string>("PE-HUC");
  const [selectedProvinceUnitId, setSelectedProvinceUnitId] = useState<string>("");
  const [selectedDistrictUnitId, setSelectedDistrictUnitId] = useState<string>("");
  const [loadingMap, setLoadingMap] = useState(false);

  const [incidents, setIncidents] = useState<IncidentListItem[]>([]);
  const [bridgesOperations, setBridgesOperations] = useState<BridgesOperationsResponse | null>(
    null
  );

  const [globalQuery, setGlobalQuery] = useState("");
  const [searchFocused, setSearchFocused] = useState(false);
  const [flyToTarget, setFlyToTarget] = useState<FlyToTarget | null>(null);
  const [routeOverlay, setRouteOverlay] = useState<RouteOverlayData | null>(null);
  const [quickOriginNodeId, setQuickOriginNodeId] = useState<string | null>(null);
  const [quickDestNodeId, setQuickDestNodeId] = useState<string | null>(null);
  const [certifyingSegment, setCertifyingSegment] = useState(false);

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
      // Silencio seguro
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
      const [regResp, nodesResp] = await Promise.all([
        fetchRegions().catch(() => null),
        fetchNodes({ region: selectedRegionCode, limit: 80 }).catch(() => null),
      ]);
      if (!mountedRef.current) return;
      if (regResp?.regions && regResp.regions.length > 0) {
        setTerritorialUnits(regResp.regions);
      }
      if (nodesResp?.nodes) {
        setRoadNodes(nodesResp.nodes);
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
            pushToast("Cola offline sincronizada.");
          }
        }
      }
      await refreshLocalState();
    } catch {
      // Mantiene cache local si el servidor no responde
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
    pushToast(`Hola, ${user.full_name}.`);
  };

  const handleLogout = async () => {
    if (authToken && refreshToken) {
      await postAuthLogout(refreshToken, getOrCreateDeviceId(), authToken).catch(() => {});
    }
    clearSession();
    pushToast("Sesión cerrada.");
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

  const handleSelectSegmentFromMap = useCallback((segmentId: string) => {
    setSelectedSegmentId(segmentId);
    setInspectorOpen(true);
  }, []);

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

  const resolvedScheme: "light" | "dark" =
    themePreference === "system" ? systemScheme : themePreference;

  const activeRegionLabel =
    selectedUnitForBbox?.name ?? activeRegionUnit?.name ?? `Región ${selectedRegionCode}`;

  // Unified search results combining territorial units, hospitals/towns, and named corridors
  const searchResults = useMemo(() => {
    const q = globalQuery.trim().toLowerCase();
    if (q.length < 2) return [];
    const results: Array<
      | { kind: "node"; id: string; title: string; subtitle: string; node: RoadNodeItem }
      | { kind: "unit"; id: string; title: string; subtitle: string; unit: TerritorialUnit }
      | { kind: "corridor"; id: string; title: string; subtitle: string; feature: CorridorSegmentFeature }
    > = [];

    for (const n of roadNodes) {
      if (n.name.toLowerCase().includes(q)) {
        results.push({
          kind: "node",
          id: `node-${n.node_id}`,
          title: n.name,
          subtitle: n.node_type === "HOSPITAL" ? "Hospital / Salud" : "Centro poblado",
          node: n,
        });
      }
      if (results.length >= 5) break;
    }

    for (const u of territorialUnits) {
      if (u.admin_level >= 6 && u.name.toLowerCase().includes(q)) {
        results.push({
          kind: "unit",
          id: `unit-${u.unit_id}`,
          title: u.name,
          subtitle: u.admin_level === 6 ? "Provincia" : "Distrito",
          unit: u,
        });
      }
      if (results.length >= 8) break;
    }

    const seenNames = new Set<string>();
    for (const f of features) {
      const nameLower = f.properties.corridor_name.toLowerCase();
      if (nameLower.includes(q) && !seenNames.has(nameLower)) {
        seenNames.add(nameLower);
        results.push({
          kind: "corridor",
          id: `seg-${f.properties.segment_id}`,
          title: f.properties.corridor_name,
          subtitle: `${f.properties.length_km.toFixed(1)} km · ${SURFACE_HUMAN[f.properties.surface_type]}`,
          feature: f,
        });
      }
      if (results.length >= 10) break;
    }

    return results;
  }, [globalQuery, roadNodes, territorialUnits, features]);

  const activeIncidentCount = incidents.filter(
    (i) => i.verification_state === "SUSPECTED" || i.verification_state === "CONFIRMED"
  ).length;

  const operationalTabs: TabDefinition[] = [
    {
      id: "routing",
      label: "Rutas",
      icon: <IconRoute size={14} />,
      content: (
        <RoutingPanel
          features={features}
          nodes={roadNodes}
          isOnline={isOnline}
          authToken={authToken}
          externalOriginNodeId={quickOriginNodeId}
          externalDestinationNodeId={quickDestNodeId}
          onRouteComputed={setRouteOverlay}
        />
      ),
    },
    {
      id: "explore",
      label: "Explorar",
      icon: <IconCompass size={14} />,
      content: (
        <RegionSelectorBar
          units={territorialUnits}
          selectedRegionCode={selectedRegionCode}
          selectedProvinceUnitId={selectedProvinceUnitId}
          selectedDistrictUnitId={selectedDistrictUnitId}
          onSelectRegion={handleSelectRegion}
          onSelectProvince={handleSelectProvince}
          onSelectDistrict={setSelectedDistrictUnitId}
          features={features}
          selectedSegmentId={selectedSegmentId}
          onSelectFeature={(f) => {
            setSelectedSegmentId(f.properties.segment_id);
            setInspectorOpen(true);
            const first = f.geometry.coordinates[0];
            if (first && first.length >= 2) {
              setFlyToTarget({ lon: first[0], lat: first[1], zoom: 12.2, nonce: Date.now() });
            }
          }}
          loading={loadingMap}
        />
      ),
    },
    {
      id: "incidents",
      label: "Alertas",
      icon: <IconAlert size={14} />,
      badge: activeIncidentCount,
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
      label: "Puentes",
      icon: <IconBridge size={14} />,
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
      icon: <IconSparkles size={14} />,
      content: (
        <SimulatorPanel
          isOnline={isOnline}
          authToken={authToken}
          userRole={currentUser?.role_code ?? null}
          onOpenAuthModal={() => setAuthModalOpen(true)}
          onToast={pushToast}
          onVisualizeSimulation={setRouteOverlay}
        />
      ),
    },
  ];

  const selectedEffectiveRisk =
    !selectedFeature || selectedFeature.properties.is_stale
      ? "UNKNOWN"
      : selectedFeature.properties.risk_level;
  const selectedIvvValue =
    !selectedFeature ||
    selectedFeature.properties.is_stale ||
    selectedFeature.properties.ivv_score === null
      ? 0
      : Math.min(100, Math.max(0, selectedFeature.properties.ivv_score));

  return (
    <div className="app-viewport">
      <a href="#main-content" className="skip-link visually-hidden">
        Ir al mapa principal
      </a>

      {/* Full-viewport interactive map */}
      <main id="main-content" tabIndex={-1} className="map-main">
        <CorridorMap
          features={features}
          selectedSegmentId={selectedSegmentId}
          onSelectSegment={handleSelectSegmentFromMap}
          pmtilesUrl={activeRegionUnit?.pmtiles_uri}
          focusBounds={focusBounds}
          flyToTarget={flyToTarget}
          resolvedScheme={resolvedScheme}
          regionLabel={activeRegionLabel}
          routeOverlay={routeOverlay}
        />
      </main>

      {/* Floating minimalist top navigation & unified search bar */}
      <header className="floating-topbar">
        <div className="brand-lockup">
          <img src="/icon.svg" alt="" width={34} height={34} className="brand-logo" />
          <div className="brand-title-wrap">
            <h1 className="brand-title">PARADIXE</h1>
            <span
              className={`conn-pill ${isOnline ? "conn-pill--online" : "conn-pill--offline"}`}
            >
              {isOnline ? "En vivo" : "Offline"}
            </span>
          </div>
        </div>

        <div className="topbar-search-wrap">
          <label htmlFor={searchInputId} className="visually-hidden">
            Buscar distrito, hospital o vía
          </label>
          <div className="topbar-search-box">
            <IconSearch size={16} className="search-icon" />
            <input
              id={searchInputId}
              className="search-input"
              type="search"
              placeholder="Buscar vía, distrito u hospital..."
              value={globalQuery}
              onFocus={() => setSearchFocused(true)}
              onBlur={() => setTimeout(() => setSearchFocused(false), 180)}
              onChange={(e) => setGlobalQuery(e.target.value)}
            />
          </div>

          {searchFocused && searchResults.length > 0 && (
            <ul className="search-results-dropdown" role="listbox" aria-label="Resultados rápidos">
              {searchResults.map((item) => (
                <li key={item.id}>
                  <button
                    type="button"
                    className="search-hit-btn"
                    onMouseDown={() => {
                      if (item.kind === "node") {
                        setFlyToTarget({
                          lon: item.node.lon,
                          lat: item.node.lat,
                          zoom: 13,
                          nonce: Date.now(),
                        });
                      } else if (item.kind === "unit") {
                        if (item.unit.admin_level === 6) {
                          handleSelectProvince(item.unit.unit_id);
                        } else {
                          setSelectedDistrictUnitId(item.unit.unit_id);
                        }
                      } else {
                        setSelectedSegmentId(item.feature.properties.segment_id);
                        setInspectorOpen(true);
                        const pt = item.feature.geometry.coordinates[0];
                        if (pt && pt.length >= 2) {
                          setFlyToTarget({
                            lon: pt[0],
                            lat: pt[1],
                            zoom: 12.2,
                            nonce: Date.now(),
                          });
                        }
                      }
                      setGlobalQuery("");
                      setSearchFocused(false);
                    }}
                  >
                    {item.kind === "node" && item.node.node_type === "HOSPITAL" ? (
                      <IconHospital size={16} />
                    ) : (
                      <IconPin size={16} />
                    )}
                    <span>
                      <strong>{item.title}</strong>
                      <small>{item.subtitle}</small>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <nav className="primary-nav" aria-label="Acciones principales">
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => setReportModalOpen(true)}
          >
            <IconPlus size={15} />
            <span>Reportar vía</span>
          </button>

          {currentUser ? (
            <div className="auth-chip">
              <span>{currentUser.full_name}</span>
              <button
                type="button"
                className="icon-btn icon-btn--sm"
                aria-label="Cerrar sesión"
                onClick={() => void handleLogout()}
              >
                <IconLogout size={15} />
              </button>
            </div>
          ) : (
            <button
              type="button"
              className="btn btn--sm"
              onClick={() => setAuthModalOpen(true)}
            >
              <IconUser size={15} />
              <span>Iniciar sesión</span>
            </button>
          )}

          <button
            type="button"
            className="icon-btn"
            aria-label={
              resolvedScheme === "dark" ? "Cambiar a modo claro" : "Cambiar a modo oscuro"
            }
            onClick={() => handleThemeChange(resolvedScheme === "dark" ? "light" : "dark")}
          >
            {resolvedScheme === "dark" ? <IconSun size={17} /> : <IconMoon size={17} />}
          </button>

          <button
            type="button"
            className="icon-btn"
            aria-label="Actualizar red vial"
            disabled={!isOnline}
            onClick={() => void syncWithServer()}
          >
            <IconRefresh size={17} />
          </button>

          <button
            type="button"
            className="icon-btn"
            aria-label="Configuración"
            onClick={() => setSettingsModalOpen(true)}
          >
            <IconSettings size={17} />
          </button>
        </nav>
      </header>

      {/* Non-blocking compact status pills */}
      <div className="floating-status-strip">
        {!isOnline && (
          <div className="status-pill status-pill--amber" role="alert">
            <span>Modo Sin Conexión (Offline-First)</span>
          </div>
        )}
        {hasStaleData && (
          <div className="status-pill status-pill--red" role="alert">
            <span>STALE_DATA: Clima vencido (&gt; 3 h)</span>
          </div>
        )}
        {queuedReports.length > 0 && (
          <div className="status-pill status-pill--blue" role="status">
            <span>Cola outbox_queue: {queuedReports.length} pendiente(s)</span>
            <button
              type="button"
              className="chip-action-link"
              disabled={!isOnline}
              onClick={() => void syncWithServer()}
            >
              Enviar
            </button>
          </div>
        )}
      </div>

      {/* Collapsible Floating Left Spatial Drawer */}
      <aside
        className={`floating-drawer${drawerOpen ? "" : " is-collapsed"}`}
        aria-label="Panel de navegación y rutas"
      >
        <div className="drawer-surface">
          <OperationalTabs
            tabs={operationalTabs}
            activeTabId={activeTabId}
            onChangeTab={setActiveTabId}
          />
        </div>

        <button
          type="button"
          className="drawer-collapse-handle"
          aria-expanded={drawerOpen}
          aria-label={drawerOpen ? "Contraer panel lateral" : "Abrir panel lateral"}
          onClick={() => setDrawerOpen((v) => !v)}
        >
          {drawerOpen ? <IconChevronLeft size={16} /> : <IconChevronRight size={16} />}
        </button>
      </aside>

      {/* Floating Contextual Inspector on right when segment is active */}
      {selectedFeature && inspectorOpen && (
        <article
          className="floating-inspector"
          aria-labelledby="selected-segment-title"
        >
          <div className="inspector-head">
            <div>
              <span className="section-eyebrow">Tramo seleccionado</span>
              <h2 id="selected-segment-title" className="inspector-title">
                {selectedFeature.properties.corridor_name}
              </h2>
            </div>
            <button
              type="button"
              className="icon-btn icon-btn--sm"
              aria-label="Ocultar detalle de tramo"
              onClick={() => setInspectorOpen(false)}
            >
              <IconX size={16} />
            </button>
          </div>

          <div className="inspector-meta-row">
            <RiskBadge
              riskLevel={selectedFeature.properties.risk_level}
              ivvScore={selectedFeature.properties.ivv_score}
              isStale={selectedFeature.properties.is_stale}
            />
            <span className="inspector-specs">
              {selectedFeature.properties.length_km.toFixed(1)} km ·{" "}
              {SURFACE_HUMAN[selectedFeature.properties.surface_type]} ·{" "}
              {PASSABILITY_HUMAN[selectedFeature.properties.passability_state]}
            </span>
          </div>

          <div className="inspector-meter-wrap">
            <div className="inspector-meter-top">
              <span>Riesgo climático (IVV)</span>
              <strong className="ivv-score-readout">
                {selectedFeature.properties.ivv_score !== null &&
                !selectedFeature.properties.is_stale
                  ? `${selectedFeature.properties.ivv_score.toFixed(1)} / 100`
                  : "Sin dato vigente"}
              </strong>
            </div>
            <progress
              className={`ivv-meter ivv-meter--${selectedEffectiveRisk}`}
              max={100}
              value={selectedIvvValue}
              aria-label="Índice de Vulnerabilidad Vial"
            />
          </div>

          <div className="inspector-actions">
            <button
              type="button"
              className="btn btn--sm"
              onClick={() => {
                setQuickOriginNodeId(selectedFeature.properties.origin_node_id);
                setActiveTabId("routing");
                setDrawerOpen(true);
              }}
            >
              Usar origen
            </button>
            <button
              type="button"
              className="btn btn--sm"
              onClick={() => {
                setQuickDestNodeId(selectedFeature.properties.destination_node_id);
                setActiveTabId("routing");
                setDrawerOpen(true);
              }}
            >
              Usar destino
            </button>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              onClick={() => setReportModalOpen(true)}
            >
              Reportar
            </button>
          </div>

          {authToken &&
            (currentUser?.role_code === "COER_ADMIN" ||
              currentUser?.role_code === "CIVIL_DEFENSE") &&
            !selectedFeature.properties.is_attributes_verified && (
              <div className="ops-actions-row" style={{ marginBlockStart: "0.5rem" }}>
                <button
                  type="button"
                  className="btn btn--sm"
                  disabled={!isOnline || certifyingSegment}
                  onClick={() => {
                    setCertifyingSegment(true);
                    void patchSegmentAttributes(
                      selectedFeature.properties.segment_id,
                      { geometry_source: "FIELD_SURVEY" },
                      authToken
                    )
                      .then(() => {
                        pushToast(
                          `Tramo ${selectedFeature.properties.corridor_name} certificado en campo.`
                        );
                        return syncWithServer();
                      })
                      .catch(() => {
                        pushToast("No fue posible certificar el tramo.");
                      })
                      .finally(() => {
                        if (mountedRef.current) setCertifyingSegment(false);
                      });
                  }}
                >
                  Certificar tramo
                </button>
                {currentUser?.role_code === "COER_ADMIN" &&
                  !selectedFeature.properties.corridor_name.startsWith("Acceso ") && (
                    <button
                      type="button"
                      className="btn btn--sm"
                      disabled={!isOnline || certifyingSegment}
                      onClick={() => {
                        setCertifyingSegment(true);
                        void postVerifyCorridorAttributes(
                          {
                            corridor_name: selectedFeature.properties.corridor_name,
                            region: selectedRegionCode,
                            geometry_source: "OFFICIAL",
                          },
                          authToken
                        )
                          .then((res) => {
                            pushToast(
                              `Eje certificado: ${res.segments_verified} tramos habilitados.`
                            );
                            return syncWithServer();
                          })
                          .catch(() => {
                            pushToast("No fue posible certificar el eje.");
                          })
                          .finally(() => {
                            if (mountedRef.current) setCertifyingSegment(false);
                          });
                      }}
                    >
                      Certificar eje
                    </button>
                  )}
              </div>
            )}

          <details className="disclosure-inline" style={{ marginBlockStart: "0.55rem" }}>
            <summary>Ficha técnica</summary>
            <p className="hint" style={{ marginBlock: "0.35rem 0" }}>
              Clima:{" "}
              {selectedFeature.properties.observed_at
                ? new Date(selectedFeature.properties.observed_at).toLocaleString("es-PE")
                : "Sin observación"}
              {" · "}
              {selectedFeature.properties.is_attributes_verified
                ? "Verificado en campo"
                : "Sin verificación física"}
            </p>
          </details>
        </article>
      )}

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
