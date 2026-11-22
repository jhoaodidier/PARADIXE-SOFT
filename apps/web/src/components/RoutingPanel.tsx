import { useEffect, useId, useMemo, useState, type FormEvent, type ReactNode } from "react";
import {
  ApiClientError,
  postMultiStopRouting,
  postRoutingCalculate,
  type CargoType,
  type CorridorSegmentFeature,
  type MultiStopRoutingResponse,
  type NoSafeRouteReasonCode,
  type RoadNodeItem,
  type RoutingCalculateResponse,
  type VehicleCategoryCode,
} from "../api/client.ts";
import type { RouteOverlayData } from "./CorridorMap.tsx";
import {
  IconAmbulance,
  IconBike,
  IconCar,
  IconClock,
  IconCopy,
  IconPlus,
  IconRoute,
  IconShare,
  IconSwap,
  IconTrash,
  IconTruck,
} from "./Icons.tsx";

export interface RoutingPanelProps {
  features: CorridorSegmentFeature[];
  nodes?: RoadNodeItem[];
  isOnline: boolean;
  authToken: string | null;
  externalOriginNodeId?: string | null;
  externalDestinationNodeId?: string | null;
  initialVehicle?: VehicleCategoryCode;
  initialCargo?: CargoType;
  onRouteComputed?: (overlay: RouteOverlayData | null) => void;
  onBuildShareUrl?: (params: {
    origin: string;
    dest: string;
    vehicle: VehicleCategoryCode;
    cargo: CargoType;
  }) => string;
  onToast?: (message: string) => void;
}

const VEHICLE_OPTIONS: Array<{
  code: VehicleCategoryCode;
  label: string;
  icon: ReactNode;
}> = [
  { code: "AMBULANCE_4X4", label: "Ambulancia", icon: <IconAmbulance size={16} /> },
  { code: "PICKUP_4X4", label: "Camioneta 4x4", icon: <IconCar size={16} /> },
  { code: "LIGHT_TRUCK", label: "Camión", icon: <IconTruck size={16} /> },
  { code: "HEAVY_TRAILER", label: "Tráiler", icon: <IconTruck size={16} /> },
  { code: "MOTORCYCLE", label: "Moto", icon: <IconBike size={16} /> },
];

const CARGO_OPTIONS: Array<{ code: CargoType; label: string }> = [
  { code: "MEDICAL_PATIENT", label: "Paciente" },
  { code: "MEDICAL_SUPPLIES", label: "Insumos" },
  { code: "PERISHABLE_AGRI", label: "Agrícola" },
];

const REASON_DESCRIPTIONS: Record<NoSafeRouteReasonCode, string> = {
  UNVERIFIED_PHYSICAL_ATTRIBUTES:
    "Tramos de la red sin verificación física de ancho o pendiente.",
  STALE_IVV:
    "El pronóstico climático caducó y no autoriza cálculo seguro.",
  VEHICLE_RESTRICTION:
    "El vehículo seleccionado excede la pendiente, ancho o peso de la vía.",
  TRANSFER_REVALIDATION_REQUIRED:
    "El paso de transbordo requiere inspección técnica reciente.",
  TRANSFER_INSPECTION_EXPIRED:
    "La inspección del paso de transbordo caducó.",
  HAZARD_INTERSECTS_TRANSFER_PATH:
    "Una zona de derrumbe intercepta el paso de transbordo.",
  TRANSFER_IVV_EXCEEDED:
    "El riesgo climático supera el máximo seguro del sector.",
  TRANSFER_CARGO_CAPACITY_EXCEEDED:
    "La carga excede el peso máximo del puente logístico.",
  TRANSFER_CARGO_NOT_ALLOWED:
    "El puente logístico no está habilitado para esta carga.",
  NO_VERIFIED_TRANSFER_CORRIDOR:
    "Sin corredor de transbordo habilitado entre ambos tramos.",
  NETWORK_DISCONNECTED:
    "Sin conexión segura disponible entre origen y destino.",
};

const MAX_SELECT_OPTIONS = 120;

export function RoutingPanel({
  features,
  nodes = [],
  isOnline,
  authToken,
  externalOriginNodeId,
  externalDestinationNodeId,
  initialVehicle,
  initialCargo,
  onRouteComputed,
  onBuildShareUrl,
  onToast,
}: RoutingPanelProps) {
  const formTitleId = useId();
  const nodeFilterId = useId();
  const payloadErrorId = useId();

  const [nodeFilter, setNodeFilter] = useState("");
  const [originNodeId, setOriginNodeId] = useState("");
  const [destinationNodeId, setDestinationNodeId] = useState("");
  const [viaNodeIds, setViaNodeIds] = useState<string[]>([]);
  const [vehicleCode, setVehicleCode] = useState<VehicleCategoryCode>(
    initialVehicle ?? "AMBULANCE_4X4"
  );
  const [cargoType, setCargoType] = useState<CargoType>(initialCargo ?? "MEDICAL_PATIENT");
  const [cargoPayloadKg, setCargoPayloadKg] = useState("1200");
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [result, setResult] = useState<RoutingCalculateResponse | null>(null);
  const [multiResult, setMultiResult] = useState<MultiStopRoutingResponse | null>(null);

  useEffect(() => {
    if (externalOriginNodeId) setOriginNodeId(externalOriginNodeId);
  }, [externalOriginNodeId]);

  useEffect(() => {
    if (externalDestinationNodeId) setDestinationNodeId(externalDestinationNodeId);
  }, [externalDestinationNodeId]);

  useEffect(() => {
    if (initialVehicle) setVehicleCode(initialVehicle);
  }, [initialVehicle]);

  useEffect(() => {
    if (initialCargo) setCargoType(initialCargo);
  }, [initialCargo]);

  const featureMap = useMemo(
    () => new Map(features.map((f) => [f.properties.segment_id, f])),
    [features]
  );

  const allNodeOptions = useMemo(() => {
    const map = new Map<string, string>();
    for (const n of nodes) {
      const prefix =
        n.node_type === "HOSPITAL"
          ? "Hospital"
          : n.node_type === "TOWN_CENTER"
            ? "Poblado"
            : "Punto";
      if (!map.has(n.node_id)) {
        map.set(n.node_id, `${n.name} · ${prefix}`);
      }
    }
    const sortedFeatures = [...features].sort((a, b) => {
      const isGeneric = (name: string) =>
        name.startsWith("Acceso ") || name.startsWith("Red Vial ") || /^HU-\d+/i.test(name);
      const aGen = isGeneric(a.properties.corridor_name) ? 1 : 0;
      const bGen = isGeneric(b.properties.corridor_name) ? 1 : 0;
      if (aGen !== bGen) return aGen - bGen;
      const aPaved = a.properties.surface_type === "PAVED" ? 0 : 1;
      const bPaved = b.properties.surface_type === "PAVED" ? 0 : 1;
      if (aPaved !== bPaved) return aPaved - bPaved;
      return b.properties.length_km - a.properties.length_km;
    });
    for (const f of sortedFeatures) {
      if (!map.has(f.properties.origin_node_id)) {
        map.set(f.properties.origin_node_id, `Nodo origen (${f.properties.corridor_name})`);
      }
      if (!map.has(f.properties.destination_node_id)) {
        map.set(
          f.properties.destination_node_id,
          `Nodo destino (${f.properties.corridor_name})`
        );
      }
    }
    return Array.from(map.entries());
  }, [nodes, features]);

  const effectiveOrigin = originNodeId || allNodeOptions[0]?.[0] || "";
  const effectiveDest =
    destinationNodeId ||
    allNodeOptions[Math.min(1, Math.max(0, allNodeOptions.length - 1))]?.[0] ||
    allNodeOptions[0]?.[0] ||
    "";

  const { visibleNodes } = useMemo(() => {
    const q = nodeFilter.trim().toLowerCase();
    const matching =
      q.length === 0
        ? allNodeOptions
        : allNodeOptions.filter(
            ([id, label]) =>
              label.toLowerCase().includes(q) || id.toLowerCase().includes(q)
          );
    const sliced = matching.slice(0, MAX_SELECT_OPTIONS);
    const seen = new Set(sliced.map(([id]) => id));
    for (const selectedId of [effectiveOrigin, ...viaNodeIds, effectiveDest]) {
      if (selectedId && !seen.has(selectedId)) {
        const found = allNodeOptions.find(([id]) => id === selectedId);
        if (found) {
          sliced.unshift(found);
          seen.add(selectedId);
        }
      }
    }
    return { visibleNodes: sliced };
  }, [allNodeOptions, nodeFilter, effectiveOrigin, effectiveDest, viaNodeIds]);

  const handleSwap = () => {
    setOriginNodeId(effectiveDest);
    setDestinationNodeId(effectiveOrigin);
    if (viaNodeIds.length > 1) {
      setViaNodeIds([...viaNodeIds].reverse());
    }
  };

  function collectRouteCoords(
    hops: Array<{ segment_id: string; direction: string }>
  ): [number, number][] {
    const coords: [number, number][] = [];
    for (const hop of hops) {
      const seg = featureMap.get(hop.segment_id);
      if (!seg) continue;
      const pts =
        hop.direction === "DEST_TO_ORIGIN"
          ? [...seg.geometry.coordinates].reverse()
          : seg.geometry.coordinates;
      for (const c of pts) {
        if (c.length >= 2) coords.push([c[0], c[1]]);
      }
    }
    return coords;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setErrorMsg(null);
    if (!isOnline) {
      setErrorMsg("El cálculo de rutas requiere conexión.");
      return;
    }
    if (!effectiveOrigin || !effectiveDest) {
      setErrorMsg("Seleccione origen y destino válidos.");
      return;
    }

    setSubmitting(true);
    setResult(null);
    setMultiResult(null);
    try {
      const parsedWeight = Number(cargoPayloadKg);
      const payloadArgs =
        cargoType === "MEDICAL_PATIENT"
          ? { patient_count: 1 }
          : { cargo_payload_kg: parsedWeight };

      const cleanVias = viaNodeIds.filter((id) => id.length > 0);
      if (cleanVias.length > 0) {
        const stops = [effectiveOrigin, ...cleanVias, effectiveDest];
        const multiResp = await postMultiStopRouting(
          {
            stop_node_ids: stops,
            vehicle_category_code: vehicleCode,
            cargo_type: cargoType,
            ...payloadArgs,
          },
          authToken
        );
        setMultiResult(multiResp);
        const allCoords: [number, number][] = [];
        for (const leg of multiResp.legs) {
          if (leg.resolution.resolution_type === "ALTERNATE_ROUTE") {
            allCoords.push(...collectRouteCoords(leg.resolution.hops));
          }
        }
        if (allCoords.length > 0 && multiResp.overall_status !== "NO_SAFE_ROUTE") {
          onRouteComputed?.({
            coordinates: allCoords,
            origin: allCoords[0] ?? null,
            destination: allCoords[allCoords.length - 1] ?? null,
            label: `${multiResp.total_distance_km.toFixed(1)} km (${stops.length} paradas)`,
          });
        } else {
          onRouteComputed?.(null);
        }
      } else {
        const response = await postRoutingCalculate(
          {
            origin_node_id: effectiveOrigin,
            destination_node_id: effectiveDest,
            vehicle_category_code: vehicleCode,
            cargo_type: cargoType,
            ...payloadArgs,
          },
          authToken
        );
        setResult(response);

        if (response.resolution_type === "ALTERNATE_ROUTE") {
          const coords = collectRouteCoords(response.hops);
          onRouteComputed?.({
            coordinates: coords,
            origin: coords[0] ?? null,
            destination: coords[coords.length - 1] ?? null,
            label: `${response.total_distance_km.toFixed(1)} km`,
          });
        } else {
          onRouteComputed?.(null);
        }
      }
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible consultar el enrutador.");
      }
    } finally {
      setSubmitting(false);
    }
  }

  const shareUrl =
    onBuildShareUrl && effectiveOrigin && effectiveDest
      ? onBuildShareUrl({
          origin: effectiveOrigin,
          dest: effectiveDest,
          vehicle: vehicleCode,
          cargo: cargoType,
        })
      : typeof window !== "undefined"
        ? window.location.href
        : "";

  async function handleCopyShareUrl() {
    if (!shareUrl) return;
    try {
      await navigator.clipboard?.writeText(shareUrl);
      onToast?.("Enlace de ruta copiado.");
    } catch {
      onToast?.("No se pudo copiar al portapapeles.");
    }
  }

  return (
    <div className="panel-stack" aria-labelledby={formTitleId}>
      <div className="panel-heading-row">
        <div>
          <h2 id={formTitleId} className="panel-title">
            Planificar ruta
          </h2>
          <p className="panel-subtitle">Rutas verificadas según vehículo y clima</p>
        </div>
        {viaNodeIds.length < 4 && allNodeOptions.length > 2 && (
          <button
            type="button"
            className="btn btn--sm"
            onClick={() =>
              setViaNodeIds((prev) => [
                ...prev,
                allNodeOptions[Math.min(prev.length + 2, allNodeOptions.length - 1)]?.[0] ??
                  effectiveOrigin,
              ])
            }
          >
            <IconPlus size={14} />
            <span>Parada</span>
          </button>
        )}
      </div>

      {!isOnline && (
        <div className="chip-alert chip-alert--amber" role="status">
          <span>Sin conexión · Cálculo deshabilitado</span>
        </div>
      )}

      <form onSubmit={handleSubmit} className="clean-form" aria-labelledby={formTitleId}>
        <div className="field field--tight">
          <label htmlFor={nodeFilterId} className="visually-hidden">
            Filtrar hospitales o localidades
          </label>
          <input
            id={nodeFilterId}
            type="search"
            placeholder="Filtrar hospital, ciudad o vía..."
            value={nodeFilter}
            onChange={(e) => setNodeFilter(e.target.value)}
          />
        </div>

        <div className="route-stops-card">
          <div className="route-stops-inputs">
            <div className="route-stop-row">
              <span className="stop-dot stop-dot--origin" aria-hidden="true" />
              <div className="field field--flush" style={{ flex: 1 }}>
                <label htmlFor="route-origin" className="visually-hidden">
                  Nodo origen
                </label>
                <select
                  id="route-origin"
                  name="origin_node_id"
                  className="brand-select"
                  required
                  value={effectiveOrigin}
                  onChange={(e) => setOriginNodeId(e.target.value)}
                  data-error="origin-error"
                >
                  {visibleNodes.length === 0 ? (
                    <option value="">Sin origen disponible</option>
                  ) : (
                    visibleNodes.map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))
                  )}
                </select>
                <span id="origin-error" className="error-msg">
                  Seleccione un origen.
                </span>
              </div>
            </div>

            {viaNodeIds.map((viaId, idx) => (
              <div key={`via-stop-${idx}`}>
                <div className="route-stop-divider" />
                <div className="route-stop-row">
                  <span className="stop-dot stop-dot--via" aria-hidden="true" />
                  <div className="field field--flush" style={{ flex: 1 }}>
                    <label htmlFor={`route-via-${idx}`} className="visually-hidden">
                      Parada intermedia {idx + 1}
                    </label>
                    <select
                      id={`route-via-${idx}`}
                      className="brand-select"
                      value={viaId}
                      onChange={(e) =>
                        setViaNodeIds((prev) =>
                          prev.map((v, i) => (i === idx ? e.target.value : v))
                        )
                      }
                    >
                      {visibleNodes.map(([id, label]) => (
                        <option key={id} value={id}>
                          {label}
                        </option>
                      ))}
                    </select>
                  </div>
                  <button
                    type="button"
                    className="icon-btn icon-btn--sm"
                    aria-label={`Quitar parada ${idx + 1}`}
                    onClick={() => setViaNodeIds((prev) => prev.filter((_, i) => i !== idx))}
                  >
                    <IconTrash size={14} />
                  </button>
                </div>
              </div>
            ))}

            <div className="route-stop-divider" />

            <div className="route-stop-row">
              <span className="stop-dot stop-dot--dest" aria-hidden="true" />
              <div className="field field--flush" style={{ flex: 1 }}>
                <label htmlFor="route-dest" className="visually-hidden">
                  Nodo destino
                </label>
                <select
                  id="route-dest"
                  name="destination_node_id"
                  className="brand-select"
                  required
                  value={effectiveDest}
                  onChange={(e) => setDestinationNodeId(e.target.value)}
                  data-error="dest-error"
                >
                  {visibleNodes.length === 0 ? (
                    <option value="">Sin destino disponible</option>
                  ) : (
                    visibleNodes.map(([id, label]) => (
                      <option key={id} value={id}>
                        {label}
                      </option>
                    ))
                  )}
                </select>
                <span id="dest-error" className="error-msg">
                  Seleccione un destino.
                </span>
              </div>
            </div>
          </div>

          <button
            type="button"
            className="swap-btn"
            aria-label="Intercambiar origen y destino"
            onClick={handleSwap}
          >
            <IconSwap size={16} />
          </button>
        </div>

        <div className="group-block">
          <span className="section-eyebrow">Vehículo</span>
          <div className="vehicle-pill-grid" role="radiogroup" aria-label="Categoría vehicular">
            {VEHICLE_OPTIONS.map((opt) => (
              <label key={opt.code} className="choice-card">
                <input
                  type="radio"
                  className="visually-hidden"
                  name="vehicle_category_code"
                  value={opt.code}
                  checked={vehicleCode === opt.code}
                  onChange={() => setVehicleCode(opt.code)}
                />
                {opt.icon}
                <span>{opt.label}</span>
              </label>
            ))}
          </div>
        </div>

        <div className="group-block">
          <span className="section-eyebrow">Traslado</span>
          <div className="segmented-pills" role="radiogroup" aria-label="Tipo de carga">
            {CARGO_OPTIONS.map((opt) => (
              <label key={opt.code} className="segmented-pill">
                <input
                  type="radio"
                  className="visually-hidden"
                  name="cargo_type"
                  value={opt.code}
                  checked={cargoType === opt.code}
                  onChange={() => setCargoType(opt.code)}
                />
                <span>{opt.label}</span>
              </label>
            ))}
          </div>
        </div>

        {cargoType !== "MEDICAL_PATIENT" && (
          <div className="field field--tight">
            <label htmlFor="cargo-payload-kg">Peso estimado (kg)</label>
            <input
              id="cargo-payload-kg"
              name="cargo_payload_kg"
              type="text"
              inputMode="decimal"
              required
              pattern="^[1-9]\d*(\.\d+)?$"
              value={cargoPayloadKg}
              onChange={(e) => setCargoPayloadKg(e.target.value)}
              data-error={payloadErrorId}
            />
            <span id={payloadErrorId} className="error-msg">
              Ingrese un peso mayor a 0 kg.
            </span>
          </div>
        )}

        <button
          type="submit"
          className="btn btn--primary btn--block"
          disabled={submitting || !isOnline || allNodeOptions.length === 0}
        >
          <IconRoute size={17} />
          <span>{submitting ? "Evaluando..." : "Calcular ruta verificable"}</span>
        </button>
      </form>

      {errorMsg && (
        <div className="banner banner--danger" role="alert">
          <span>{errorMsg}</span>
        </div>
      )}

      {multiResult && (
        <div className="result-surface" aria-live="polite">
          {multiResult.overall_status === "NO_SAFE_ROUTE" ? (
            <div className="banner banner--danger" role="alert">
              <strong>
                NO_SAFE_ROUTE ({multiResult.blocking_reason_code ?? "NETWORK_DISCONNECTED"})
              </strong>
              <p style={{ marginBlock: "0.25rem 0" }}>
                {multiResult.blocking_reason_code
                  ? REASON_DESCRIPTIONS[multiResult.blocking_reason_code]
                  : "Un tramo de la ruta multiparada no cuenta con paso verificable."}
              </p>
            </div>
          ) : (
            <>
              <div className="result-top-row">
                <strong>Ruta multiparada ({multiResult.legs.length} tramos)</strong>
                <span className="risk-badge risk-badge--GREEN">{multiResult.overall_status}</span>
              </div>
              <div className="stat-pill-grid">
                <div className="stat-mini">
                  <strong>{multiResult.total_distance_km.toFixed(1)} km</strong>
                  <span>Distancia</span>
                </div>
                <div className="stat-mini">
                  <strong>{multiResult.estimated_time_min.toFixed(0)} min</strong>
                  <span>Nominal</span>
                </div>
                <div className="stat-mini">
                  <strong>{multiResult.max_ivv_on_route.toFixed(0)}</strong>
                  <span>IVV máx</span>
                </div>
              </div>
              {multiResult.economic_impact && (
                <div className="econ-impact-strip">
                  <span>
                    Ahorro: <strong>+{multiResult.economic_impact.time_saved_vs_blockage_min.toFixed(0)} min</strong>
                  </span>
                  <span>
                    Huella: <strong>{multiResult.economic_impact.estimated_co2_kg.toFixed(1)} kg CO₂</strong>
                  </span>
                </div>
              )}
            </>
          )}
        </div>
      )}

      {result && (
        <div className="result-surface" aria-live="polite">
          {result.resolution_type === "ALTERNATE_ROUTE" && (
            <>
              <div className="result-top-row">
                <strong>Ruta recomendada</strong>
                <span className="risk-badge risk-badge--GREEN">Autorización requerida</span>
              </div>
              <div className="stat-pill-grid">
                <div className="stat-mini">
                  <strong>{result.total_distance_km.toFixed(1)} km</strong>
                  <span>Distancia</span>
                </div>
                <div className="stat-mini">
                  <strong>{result.estimated_time_min} min</strong>
                  <span>
                    <IconClock size={12} /> Nominal
                  </span>
                </div>
                <div className="stat-mini">
                  <strong>{result.max_ivv_on_route.toFixed(0)}</strong>
                  <span>IVV máx</span>
                </div>
              </div>

              {result.economic_impact && (
                <div className="econ-impact-strip">
                  <span>
                    Ahorro vs cierre 4 h:{" "}
                    <strong>+{result.economic_impact.time_saved_vs_blockage_min.toFixed(0)} min</strong>
                  </span>
                  <span>
                    Huella: <strong>{result.economic_impact.estimated_co2_kg.toFixed(1)} kg CO₂</strong>
                  </span>
                  {result.economic_impact.estimated_cargo_loss_avoided_usd > 0 && (
                    <span className="econ-savings-highlight">
                      Merma evitada:{" "}
                      <strong>
                        ${result.economic_impact.estimated_cargo_loss_avoided_usd.toFixed(2)} USD
                      </strong>
                    </span>
                  )}
                </div>
              )}

              {result.hops.length > 0 && (
                <details className="disclosure-inline">
                  <summary>Ver detalle por tramos ({result.hops.length})</summary>
                  <ol className="route-hops-list">
                    {result.hops.map((hop, idx) => {
                      const seg = featureMap.get(hop.segment_id);
                      const risk = seg?.properties.is_stale
                        ? "UNKNOWN"
                        : (seg?.properties.risk_level ?? "UNKNOWN");
                      return (
                        <li key={`${hop.segment_id}-${idx}`} className="route-hop-item">
                          <span className="route-hop-name">
                            {seg?.properties.corridor_name ?? `Tramo ${idx + 1}`}
                          </span>
                          <span className="route-hop-meta">
                            {seg ? `${seg.properties.length_km.toFixed(1)} km` : ""}
                            <span className={`risk-badge risk-badge--${risk}`}>{risk}</span>
                          </span>
                        </li>
                      );
                    })}
                  </ol>
                </details>
              )}

              <div className="ops-actions-row" style={{ marginBlockStart: "0.35rem" }}>
                <button type="button" className="btn btn--sm" onClick={() => void handleCopyShareUrl()}>
                  <IconCopy size={14} />
                  <span>Copiar enlace</span>
                </button>
                <a
                  className="btn btn--sm"
                  href={`https://wa.me/?text=${encodeURIComponent(
                    `PARADIXE SOFT · Ruta verificable: ${result.total_distance_km.toFixed(1)} km, ${result.estimated_time_min} min (IVV máx ${result.max_ivv_on_route.toFixed(0)}). ${shareUrl}`
                  )}`}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  <IconShare size={14} />
                  <span>WhatsApp</span>
                </a>
              </div>
            </>
          )}

          {result.resolution_type === "LOGISTIC_BRIDGE_REQUIRED" && (
            <div>
              <strong>Puente logístico sugerido</strong>
              <p className="panel-subtitle" style={{ marginBlockStart: "0.25rem" }}>
                Transbordo de {result.bridge_proposal.length_m} m ·{" "}
                {result.bridge_proposal.estimated_transfer_time_min} min estimados
              </p>
            </div>
          )}

          {result.resolution_type === "NO_SAFE_ROUTE" && (
            <div className="banner banner--danger" role="alert">
              <div>
                <strong>NO_SAFE_ROUTE ({result.reason_code})</strong>
                <p style={{ marginBlock: "0.25rem 0" }}>
                  {REASON_DESCRIPTIONS[result.reason_code]}
                </p>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
