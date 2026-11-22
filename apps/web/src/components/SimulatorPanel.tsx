import { useId, useState, type FormEvent } from "react";
import {
  ApiClientError,
  fetchSimImpact,
  postSimCreateScenario,
  postSimEvaluate,
  postSimInjectEvent,
  postSimRouting,
  type CargoType,
  type RoleCode,
  type SimEvaluateResponse,
  type SimImpactResponse,
  type SimRoutingResponse,
  type SimScenarioResponse,
  type VehicleCategoryCode,
} from "../api/client.ts";
import type { RouteOverlayData } from "./CorridorMap.tsx";
import { IconSparkles } from "./Icons.tsx";

export interface SimulatorPanelProps {
  isOnline: boolean;
  authToken: string | null;
  userRole: RoleCode | null;
  onOpenAuthModal: () => void;
  onToast: (message: string) => void;
  onVisualizeSimulation?: (overlay: RouteOverlayData | null) => void;
}

const NODE_HUANUCO = "10000000-0000-4000-8000-000000000001";
const NODE_TINGO_MARIA = "10000000-0000-4000-8000-000000000005";
const SEG_CARPISH_TUNNEL = "20000000-0000-4000-8000-000000000002";
const SEG_HUANUCO_CARPISH = "20000000-0000-4000-8000-000000000001";
const SEG_CARPISH_TINGO = "20000000-0000-4000-8000-000000000003";

const CARPISH_DRILL_LINE: [number, number][] = [
  [-76.2422, -9.9306],
  [-76.165, -9.82],
  [-76.091, -9.712],
  [-75.998, -9.301],
];

function ImpactBar({
  label,
  baselineValue,
  mitigatedValue,
  unit,
  deltaText,
}: {
  label: string;
  baselineValue: number;
  mitigatedValue: number | null;
  unit: string;
  deltaText: string;
}) {
  const max = Math.max(baselineValue, mitigatedValue ?? 0, 1);
  const basePct = Math.round((baselineValue / max) * 100);
  const mitPct = mitigatedValue !== null ? Math.round((mitigatedValue / max) * 100) : 0;

  return (
    <div className="impact-bar-card">
      <div className="impact-bar-head">
        <strong>{label}</strong>
        <span className="impact-delta-tag">{deltaText}</span>
      </div>
      <div className="impact-bar-row">
        <span>Sin mitigación</span>
        <div className="impact-track">
          <div className="impact-fill impact-fill--base" style={{ width: `${basePct}%` }} />
        </div>
        <small>
          {baselineValue.toFixed(1)} {unit}
        </small>
      </div>
      <div className="impact-bar-row">
        <span>Con ruta</span>
        <div className="impact-track">
          <div className="impact-fill impact-fill--mit" style={{ width: `${mitPct}%` }} />
        </div>
        <small>
          {mitigatedValue !== null ? `${mitigatedValue.toFixed(1)} ${unit}` : "—"}
        </small>
      </div>
    </div>
  );
}

export function SimulatorPanel({
  isOnline,
  authToken,
  userRole,
  onOpenAuthModal,
  onToast,
  onVisualizeSimulation,
}: SimulatorPanelProps) {
  const headingId = useId();
  const [scenarioName, setScenarioName] = useState("Simulacro Huánuco – Carpish");
  const [scenario, setScenario] = useState<SimScenarioResponse | null>(null);
  const [revision, setRevision] = useState(0);

  const [eventMode, setEventMode] = useState<"WEATHER" | "BLOCKED" | "PARTIAL">("WEATHER");
  const [targetSegId, setTargetSegId] = useState(SEG_CARPISH_TUNNEL);
  const [precip24h, setPrecip24h] = useState("85");
  const [precip72h, setPrecip72h] = useState("190");
  const [hazardRadiusM, setHazardRadiusM] = useState("200");
  const [notifications, setNotifications] = useState<Array<{ channel: string; preview: string }>>(
    []
  );

  const [vehicleCode, setVehicleCode] = useState<VehicleCategoryCode>("PICKUP_4X4");
  const [cargoType, setCargoType] = useState<CargoType>("PERISHABLE_AGRI");
  const [payloadKg, setPayloadKg] = useState("1200");
  const [baselineWaitMin, setBaselineWaitMin] = useState("360");
  const [evalTimeIso, setEvalTimeIso] = useState("2026-11-20T09:00:00Z");

  const [simRouting, setSimRouting] = useState<SimRoutingResponse | null>(null);
  const [simEval, setSimEval] = useState<SimEvaluateResponse | null>(null);
  const [simImpact, setSimImpact] = useState<SimImpactResponse | null>(null);
  const [busy, setBusy] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const isCoerAdmin = Boolean(authToken && userRole === "COER_ADMIN");

  async function handleCreateScenario(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken) return;
    setErrorMsg(null);
    setBusy(true);
    try {
      const created = await postSimCreateScenario(
        {
          name: scenarioName.trim() || "Simulacro Carpish",
          fixture_tag: "huanuco-carpish-drill-v1",
        },
        authToken
      );
      setScenario(created);
      setRevision(created.revision);
      setNotifications([]);
      setSimRouting(null);
      setSimEval(null);
      setSimImpact(null);
      onVisualizeSimulation?.({
        coordinates: CARPISH_DRILL_LINE,
        origin: CARPISH_DRILL_LINE[0],
        destination: CARPISH_DRILL_LINE[CARPISH_DRILL_LINE.length - 1],
        label: "Simulacro Carpish",
      });
      onToast("Simulacro listo.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible crear el simulacro.");
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleInjectEvent(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !scenario) return;
    setErrorMsg(null);
    setBusy(true);
    try {
      const payload =
        eventMode === "WEATHER"
          ? ({
              event_type: "WEATHER_OVERRIDE",
              segment_id: targetSegId,
              precip_24h_mm: Number(precip24h) || 0,
              precip_72h_mm: Number(precip72h) || 0,
            } as const)
          : ({
              event_type: "BLOCKAGE_OVERRIDE",
              segment_id: targetSegId,
              simulated_passability: eventMode === "BLOCKED" ? "BLOCKED" : "PARTIAL_ONE_LANE",
              hazard_lat: -9.712,
              hazard_lon: -76.091,
              hazard_radius_m: Math.max(10, Number(hazardRadiusM) || 200),
            } as const);

      const res = await postSimInjectEvent(scenario.scenario_id, payload, authToken);
      setRevision((r) => r + 1);
      setNotifications((prev) => [...res.simulated_notifications, ...prev]);
      onVisualizeSimulation?.({
        coordinates: CARPISH_DRILL_LINE,
        origin: CARPISH_DRILL_LINE[0],
        destination: CARPISH_DRILL_LINE[CARPISH_DRILL_LINE.length - 1],
        hazard: { lat: -9.712, lon: -76.091, label: "Evento simulado Carpish" },
      });
      onToast("Evento inyectado en [SIMULATION].");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible inyectar el evento.");
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleRunEvaluation(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !scenario) return;
    setErrorMsg(null);
    setBusy(true);
    try {
      const cargoFields =
        cargoType === "MEDICAL_PATIENT"
          ? { patient_count: 1 }
          : { cargo_payload_kg: Number(payloadKg) || 1200 };

      const routingRes = await postSimRouting(
        scenario.scenario_id,
        {
          origin_node_id: NODE_HUANUCO,
          destination_node_id: NODE_TINGO_MARIA,
          vehicle_category_code: vehicleCode,
          cargo_type: cargoType,
          ...cargoFields,
          scenario_revision: revision,
          evaluation_time: evalTimeIso,
        },
        authToken
      );
      setSimRouting(routingRes);

      const evalRes = await postSimEvaluate(
        scenario.scenario_id,
        {
          origin_node_id: NODE_HUANUCO,
          destination_node_id: NODE_TINGO_MARIA,
          vehicle_category_code: vehicleCode,
          cargo_type: cargoType,
          ...cargoFields,
          baseline_wait_min: Number(baselineWaitMin) || 0,
          scenario_revision: revision,
          evaluation_time: evalTimeIso,
        },
        authToken
      );
      setSimEval(evalRes);

      const impactRes = await fetchSimImpact(scenario.scenario_id, authToken);
      setSimImpact(impactRes);
      onToast("Impacto simulado calculado.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("Error al evaluar el simulacro.");
      }
    } finally {
      setBusy(false);
    }
  }

  const latestImpact =
    simImpact && simImpact.evaluations.length > 0
      ? simImpact.evaluations[simImpact.evaluations.length - 1]
      : null;

  return (
    <div className="panel-stack" aria-labelledby={headingId}>
      <div className="panel-heading-row">
        <div>
          <h2 id={headingId} className="panel-title">
            Simulador de Crisis
          </h2>
          <p className="panel-subtitle">[SIMULATION] Entorno aislado · sin efecto real</p>
        </div>
        <span className="risk-badge risk-badge--AMBER">
          <IconSparkles size={13} />
          <span>Modo prueba</span>
        </span>
      </div>

      {!isCoerAdmin ? (
        <div className="empty-state-card">
          <IconSparkles size={22} />
          <strong>Acceso de administrador COER</strong>
          <span>Inicia sesión como COER_ADMIN para simular escenarios.</span>
          <button type="button" className="btn btn--primary btn--sm" onClick={onOpenAuthModal}>
            Iniciar sesión (COER_ADMIN)
          </button>
        </div>
      ) : (
        <div className="panel-stack">
          {errorMsg && (
            <div className="banner banner--danger" role="alert">
              <span>{errorMsg}</span>
            </div>
          )}

          <form onSubmit={handleCreateScenario} className="subform-box">
            <div className="field field--tight">
              <label htmlFor="sim-scenario-name">Nombre del escenario</label>
              <input
                id="sim-scenario-name"
                type="text"
                required
                maxLength={120}
                value={scenarioName}
                onChange={(e) => setScenarioName(e.target.value)}
              />
            </div>
            <button type="submit" className="btn btn--primary btn--block" disabled={!isOnline || busy}>
              {scenario ? "Reiniciar escenario" : "Crear escenario aislado"}
            </button>
          </form>

          {scenario && (
            <>
              <form onSubmit={handleInjectEvent} className="subform-box">
                <span className="section-eyebrow">1. Evento en corredor Carpish</span>
                <div className="filter-chips-row" style={{ marginBlockEnd: "0.65rem" }}>
                  {(
                    [
                      ["WEATHER", "Lluvia extrema"],
                      ["BLOCKED", "Bloqueo total"],
                      ["PARTIAL", "Paso 1 carril"],
                    ] as const
                  ).map(([mode, label]) => (
                    <button
                      key={mode}
                      type="button"
                      className={`filter-chip${eventMode === mode ? " is-active" : ""}`}
                      onClick={() => setEventMode(mode)}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                <div className="field field--tight">
                  <label htmlFor="sim-segment">Tramo Carpish</label>
                  <select
                    id="sim-segment"
                    className="brand-select"
                    value={targetSegId}
                    onChange={(e) => setTargetSegId(e.target.value)}
                  >
                    <option value={SEG_CARPISH_TUNNEL}>Túnel Carpish (8 km)</option>
                    <option value={SEG_HUANUCO_CARPISH}>Huánuco – Carpish Sur (45 km)</option>
                    <option value={SEG_CARPISH_TINGO}>Carpish Norte – Tingo María (65 km)</option>
                  </select>
                </div>

                {eventMode === "WEATHER" ? (
                  <div className="two-col-inline">
                    <div className="field field--tight">
                      <label htmlFor="sim-p24">Lluvia 24h (mm)</label>
                      <input
                        id="sim-p24"
                        type="text"
                        inputMode="decimal"
                        value={precip24h}
                        onChange={(e) => setPrecip24h(e.target.value)}
                      />
                    </div>
                    <div className="field field--tight">
                      <label htmlFor="sim-p72">Lluvia 72h (mm)</label>
                      <input
                        id="sim-p72"
                        type="text"
                        inputMode="decimal"
                        value={precip72h}
                        onChange={(e) => setPrecip72h(e.target.value)}
                      />
                    </div>
                  </div>
                ) : (
                  <div className="field field--tight">
                    <label htmlFor="sim-radius">Radio de derrumbe (m)</label>
                    <input
                      id="sim-radius"
                      type="text"
                      inputMode="numeric"
                      value={hazardRadiusM}
                      onChange={(e) => setHazardRadiusM(e.target.value)}
                    />
                  </div>
                )}

                <button type="submit" className="btn btn--block" disabled={!isOnline || busy}>
                  Inyectar evento (rev. {revision + 1})
                </button>

                {notifications.length > 0 && (
                  <ul className="ops-card-list" style={{ marginBlockStart: "0.65rem" }}>
                    {notifications.map((n, idx) => (
                      <li key={idx} className="ops-list-item">
                        <small>{n.preview}</small>
                      </li>
                    ))}
                  </ul>
                )}
              </form>

              <form onSubmit={handleRunEvaluation} className="subform-box">
                <span className="section-eyebrow">2. Evaluar ruta e impacto</span>
                <div className="two-col-inline">
                  <div className="field field--tight">
                    <label htmlFor="sim-vehicle">Vehículo</label>
                    <select
                      id="sim-vehicle"
                      className="brand-select"
                      value={vehicleCode}
                      onChange={(e) => setVehicleCode(e.target.value as VehicleCategoryCode)}
                    >
                      <option value="PICKUP_4X4">Camioneta 4x4</option>
                      <option value="AMBULANCE_4X4">Ambulancia 4x4</option>
                      <option value="LIGHT_TRUCK">Camión agrícola</option>
                      <option value="HEAVY_TRAILER">Tráiler pesado</option>
                      <option value="MOTORCYCLE">Motocicleta</option>
                    </select>
                  </div>

                  <div className="field field--tight">
                    <label htmlFor="sim-cargo">Carga</label>
                    <select
                      id="sim-cargo"
                      className="brand-select"
                      value={cargoType}
                      onChange={(e) => setCargoType(e.target.value as CargoType)}
                    >
                      <option value="PERISHABLE_AGRI">Agrícola perecible</option>
                      <option value="MEDICAL_PATIENT">Paciente crítico</option>
                      <option value="MEDICAL_SUPPLIES">Insumos médicos</option>
                    </select>
                  </div>
                </div>

                <details className="disclosure-inline">
                  <summary>Parámetros avanzados</summary>
                  <div className="two-col-inline" style={{ marginBlockStart: "0.5rem" }}>
                    {cargoType !== "MEDICAL_PATIENT" && (
                      <div className="field field--tight">
                        <label htmlFor="sim-payload">Carga (kg)</label>
                        <input
                          id="sim-payload"
                          type="text"
                          inputMode="decimal"
                          value={payloadKg}
                          onChange={(e) => setPayloadKg(e.target.value)}
                        />
                      </div>
                    )}
                    <div className="field field--tight">
                      <label htmlFor="sim-wait">Espera base (min)</label>
                      <input
                        id="sim-wait"
                        type="text"
                        inputMode="numeric"
                        value={baselineWaitMin}
                        onChange={(e) => setBaselineWaitMin(e.target.value)}
                      />
                    </div>
                  </div>
                  <div className="field field--tight">
                    <label htmlFor="sim-eval-time">Fecha evaluación (UTC)</label>
                    <input
                      id="sim-eval-time"
                      type="text"
                      value={evalTimeIso}
                      onChange={(e) => setEvalTimeIso(e.target.value)}
                    />
                  </div>
                </details>

                <button type="submit" className="btn btn--primary btn--block" disabled={!isOnline || busy}>
                  {busy ? "Calculando..." : "Ejecutar ruta + evaluar impacto"}
                </button>
              </form>
            </>
          )}

          {(simRouting || latestImpact) && (
            <div className="impact-cards-wrap" aria-live="polite">
              {simRouting && (
                <div
                  className={`banner ${
                    simRouting.resolution_type === "NO_SAFE_ROUTE"
                      ? "banner--danger"
                      : "banner--info"
                  }`}
                >
                  <span>
                    <strong>
                      {simRouting.resolution_type === "ALTERNATE_ROUTE"
                        ? "Ruta alterna"
                        : simRouting.resolution_type === "LOGISTIC_BRIDGE_REQUIRED"
                          ? "Puente logístico"
                          : "Sin ruta segura"}
                    </strong>
                    {simRouting.resolution_type === "ALTERNATE_ROUTE" &&
                      ` · ${simRouting.total_distance_km.toFixed(1)} km · ${simRouting.estimated_time_min.toFixed(0)} min`}
                    {simRouting.resolution_type === "LOGISTIC_BRIDGE_REQUIRED" &&
                      ` · ${simRouting.bridge_proposal.length_m} m`}
                    {simRouting.resolution_type === "NO_SAFE_ROUTE" &&
                      ` (${simRouting.reason_code})`}
                  </span>
                </div>
              )}

              {latestImpact && (
                <div className="panel-stack">
                  <span className="section-eyebrow">
                    Comparativa ({simEval?.param_version_tag ?? "impact"})
                  </span>
                  <ImpactBar
                    label="Tiempo de viaje"
                    baselineValue={latestImpact.baseline.travel_min}
                    mitigatedValue={latestImpact.mitigated.travel_min}
                    unit="min"
                    deltaText={
                      latestImpact.deltas.time_saved_pct !== null
                        ? `-${latestImpact.deltas.time_saved_pct.toFixed(1)}%`
                        : "Sin ruta"
                    }
                  />
                  <ImpactBar
                    label="Emisión neta CO₂"
                    baselineValue={latestImpact.baseline.co2_kg}
                    mitigatedValue={latestImpact.mitigated.co2_kg}
                    unit="kg"
                    deltaText={
                      latestImpact.deltas.net_co2_delta_kg !== null
                        ? `Delta CO₂: ${latestImpact.deltas.net_co2_delta_kg.toFixed(2)} kg`
                        : "Sin ruta"
                    }
                  />
                  <ImpactBar
                    label="Merma agrícola"
                    baselineValue={latestImpact.baseline.cargo_loss_usd}
                    mitigatedValue={latestImpact.mitigated.cargo_loss_usd}
                    unit="USD"
                    deltaText={
                      latestImpact.deltas.net_agri_savings_usd !== null
                        ? `Ahorro neto: $${latestImpact.deltas.net_agri_savings_usd.toFixed(2)}`
                        : "Sin mitigación"
                    }
                  />
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
