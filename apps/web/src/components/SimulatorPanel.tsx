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

export interface SimulatorPanelProps {
  isOnline: boolean;
  authToken: string | null;
  userRole: RoleCode | null;
  onOpenAuthModal: () => void;
  onToast: (message: string) => void;
}

const NODE_HUANUCO = "10000000-0000-4000-8000-000000000001";
const NODE_TINGO_MARIA = "10000000-0000-4000-8000-000000000005";
const SEG_CARPISH_TUNNEL = "20000000-0000-4000-8000-000000000002";
const SEG_HUANUCO_CARPISH = "20000000-0000-4000-8000-000000000001";
const SEG_CARPISH_TINGO = "20000000-0000-4000-8000-000000000003";

export function SimulatorPanel({
  isOnline,
  authToken,
  userRole,
  onOpenAuthModal,
  onToast,
}: SimulatorPanelProps) {
  const headingId = useId();
  const [scenarioName, setScenarioName] = useState(
    "Simulacro Huánuco – Carpish (Lluvias Extremas)"
  );
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
      onToast("Escenario de simulacro creado en módulo aislado.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible crear el escenario de simulacro.");
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
      onToast("Evento sintético inyectado en entorno [SIMULATION].");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible inyectar el evento sintético.");
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
      onToast("Evaluación de impacto socioeconómico completada.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("Error al ejecutar la evaluación del simulacro.");
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
    <section className="card" aria-labelledby={headingId}>
      <h2 id={headingId}>Simulador de Crisis y Contingencia</h2>
      <div className="banner banner--warning" role="status" style={{ marginBlockEnd: "0.75rem" }}>
        <span>
          <strong>[SIMULATION] Módulo de Contingencia Aislado:</strong> opera sobre el fixture{" "}
          <code>huanuco-carpish-drill-v1</code> con pool de solo lectura operativo y pasarela de
          mensajería simulada (nunca afecta la operación real).
        </span>
      </div>

      {!isCoerAdmin ? (
        <div className="banner banner--info">
          <span>
            Ejecutar escenarios What-If e impacto requiere una sesión con rol{" "}
            <strong>COER_ADMIN</strong>.
          </span>
          <button type="button" className="btn btn--primary" onClick={onOpenAuthModal}>
            Iniciar sesión (COER_ADMIN)
          </button>
        </div>
      ) : (
        <div>
          {errorMsg && (
            <div className="banner banner--danger" role="alert" style={{ marginBlockEnd: "1rem" }}>
              <span>{errorMsg}</span>
            </div>
          )}

          <form onSubmit={handleCreateScenario} className="subform-box">
            <h3>1. Escenario Huánuco – Carpish</h3>
            <div className="field">
              <label htmlFor="sim-scenario-name">Nombre del simulacro</label>
              <input
                id="sim-scenario-name"
                type="text"
                required
                maxLength={120}
                value={scenarioName}
                onChange={(e) => setScenarioName(e.target.value)}
              />
            </div>
            <button type="submit" className="btn btn--primary" disabled={!isOnline || busy}>
              {scenario ? "Reiniciar nuevo escenario" : "Crear escenario aislado"}
            </button>
            {scenario && (
              <p className="hint" style={{ marginBlock: "0.5rem 0" }}>
                Escenario activo: <code>{scenario.scenario_id}</code> · Revisión:{" "}
                <strong>{revision}</strong>
              </p>
            )}
          </form>

          {scenario && (
            <>
              <form onSubmit={handleInjectEvent} className="subform-box">
                <h3>2. Inyectar Evento Geotécnico / Climático</h3>
                <div className="field">
                  <label htmlFor="sim-segment">Tramo del fixture Carpish</label>
                  <select
                    id="sim-segment"
                    value={targetSegId}
                    onChange={(e) => setTargetSegId(e.target.value)}
                  >
                    <option value={SEG_CARPISH_TUNNEL}>Túnel Carpish (8 km, 27%)</option>
                    <option value={SEG_HUANUCO_CARPISH}>Huánuco – Carpish Sur (45 km)</option>
                    <option value={SEG_CARPISH_TINGO}>Carpish Norte – Tingo María (65 km)</option>
                  </select>
                </div>

                <div className="field">
                  <label htmlFor="sim-event-mode">Tipo de evento</label>
                  <select
                    id="sim-event-mode"
                    value={eventMode}
                    onChange={(e) =>
                      setEventMode(e.target.value as "WEATHER" | "BLOCKED" | "PARTIAL")
                    }
                  >
                    <option value="WEATHER">WEATHER_OVERRIDE (Lluvia extrema)</option>
                    <option value="BLOCKED">BLOCKAGE_OVERRIDE (Bloqueo total)</option>
                    <option value="PARTIAL">BLOCKAGE_OVERRIDE (Paso 1 carril)</option>
                  </select>
                </div>

                {eventMode === "WEATHER" ? (
                  <div className="two-col-inline">
                    <div className="field">
                      <label htmlFor="sim-p24">Lluvia 24h (mm)</label>
                      <input
                        id="sim-p24"
                        type="text"
                        inputMode="decimal"
                        value={precip24h}
                        onChange={(e) => setPrecip24h(e.target.value)}
                      />
                    </div>
                    <div className="field">
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
                  <div className="field">
                    <label htmlFor="sim-radius">Radio de peligro (m)</label>
                    <input
                      id="sim-radius"
                      type="text"
                      inputMode="numeric"
                      value={hazardRadiusM}
                      onChange={(e) => setHazardRadiusM(e.target.value)}
                    />
                  </div>
                )}

                <button type="submit" disabled={!isOnline || busy}>
                  Inyectar evento (rev. {revision + 1})
                </button>

                {notifications.length > 0 && (
                  <ul className="ops-card-list" style={{ marginBlockStart: "0.75rem" }}>
                    {notifications.map((n, idx) => (
                      <li key={idx} className="ops-list-item">
                        <strong>[{n.channel}]</strong> {n.preview}
                      </li>
                    ))}
                  </ul>
                )}
              </form>

              <form onSubmit={handleRunEvaluation} className="subform-box">
                <h3>3. Evaluar Enrutamiento e Impacto (Huánuco → Tingo María)</h3>
                <div className="field">
                  <label htmlFor="sim-vehicle">Vehículo evaluado</label>
                  <select
                    id="sim-vehicle"
                    value={vehicleCode}
                    onChange={(e) => setVehicleCode(e.target.value as VehicleCategoryCode)}
                  >
                    <option value="PICKUP_4X4">PICKUP_4X4 (Camioneta 4x4)</option>
                    <option value="AMBULANCE_4X4">AMBULANCE_4X4 (Ambulancia 4x4)</option>
                    <option value="LIGHT_TRUCK">LIGHT_TRUCK (Camión agrícola)</option>
                    <option value="HEAVY_TRAILER">HEAVY_TRAILER (Tráiler pesado)</option>
                    <option value="MOTORCYCLE">MOTORCYCLE (Motocicleta)</option>
                  </select>
                </div>

                <div className="field">
                  <label htmlFor="sim-cargo">Carga</label>
                  <select
                    id="sim-cargo"
                    value={cargoType}
                    onChange={(e) => setCargoType(e.target.value as CargoType)}
                  >
                    <option value="PERISHABLE_AGRI">PERISHABLE_AGRI (Agrícola perecible)</option>
                    <option value="MEDICAL_PATIENT">MEDICAL_PATIENT (Paciente crítico)</option>
                    <option value="MEDICAL_SUPPLIES">MEDICAL_SUPPLIES (Insumos médicos)</option>
                  </select>
                </div>

                <div className="two-col-inline">
                  {cargoType !== "MEDICAL_PATIENT" && (
                    <div className="field">
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
                  <div className="field">
                    <label htmlFor="sim-wait">Espera sin mitigación (min)</label>
                    <input
                      id="sim-wait"
                      type="text"
                      inputMode="numeric"
                      value={baselineWaitMin}
                      onChange={(e) => setBaselineWaitMin(e.target.value)}
                    />
                  </div>
                </div>

                <div className="field">
                  <label htmlFor="sim-eval-time">Reloj de evaluación (UTC ISO-8601)</label>
                  <input
                    id="sim-eval-time"
                    type="text"
                    value={evalTimeIso}
                    onChange={(e) => setEvalTimeIso(e.target.value)}
                  />
                </div>

                <button type="submit" className="btn btn--primary" disabled={!isOnline || busy}>
                  {busy ? "Calculando..." : "Ejecutar ruta + evaluar impacto"}
                </button>
              </form>
            </>
          )}

          {(simRouting || latestImpact) && (
            <div className="impact-cards-wrap" aria-live="polite">
              <h3>Resultados de Simulación ({simEval?.param_version_tag ?? "impact"})</h3>
              {simRouting && (
                <div
                  className={`banner ${
                    simRouting.resolution_type === "NO_SAFE_ROUTE"
                      ? "banner--danger"
                      : "banner--info"
                  }`}
                  style={{ marginBlockEnd: "0.75rem" }}
                >
                  <span>
                    <strong>Decisión:</strong> {simRouting.resolution_type}
                    {simRouting.resolution_type === "ALTERNATE_ROUTE" &&
                      ` · ${simRouting.total_distance_km.toFixed(1)} km / ${simRouting.estimated_time_min.toFixed(1)} min`}
                    {simRouting.resolution_type === "LOGISTIC_BRIDGE_REQUIRED" &&
                      ` · ${simRouting.bridge_proposal.transfer_mode} (${simRouting.bridge_proposal.length_m} m)`}
                    {simRouting.resolution_type === "NO_SAFE_ROUTE" &&
                      ` (${simRouting.reason_code})`}
                  </span>
                </div>
              )}

              {latestImpact && (
                <div className="impact-grid">
                  <div className="metric-box">
                    <span className="metric-label">Tiempo (Sin mitig. → Mitigado)</span>
                    <span className="metric-value">
                      {latestImpact.baseline.travel_min.toFixed(1)} →{" "}
                      {latestImpact.mitigated.travel_min !== null
                        ? `${latestImpact.mitigated.travel_min.toFixed(1)} min`
                        : "null"}
                    </span>
                    <span className="hint" style={{ marginBlockEnd: 0 }}>
                      Delta:{" "}
                      {latestImpact.deltas.time_delta_min !== null
                        ? `${latestImpact.deltas.time_delta_min.toFixed(1)} min (${latestImpact.deltas.time_saved_pct?.toFixed(1)}%)`
                        : "Sin ruta segura"}
                    </span>
                  </div>

                  <div className="metric-box">
                    <span className="metric-label">CO₂ neto (kg)</span>
                    <span className="metric-value">
                      {latestImpact.baseline.co2_kg.toFixed(2)} →{" "}
                      {latestImpact.mitigated.co2_kg !== null
                        ? `${latestImpact.mitigated.co2_kg.toFixed(2)} kg`
                        : "null"}
                    </span>
                    <span className="hint" style={{ marginBlockEnd: 0 }}>
                      Delta CO₂:{" "}
                      {latestImpact.deltas.net_co2_delta_kg !== null
                        ? `${latestImpact.deltas.net_co2_delta_kg.toFixed(2)} kg`
                        : "Sin mitigación"}
                    </span>
                  </div>

                  <div className="metric-box">
                    <span className="metric-label">Merma agrícola (USD)</span>
                    <span className="metric-value">
                      ${latestImpact.baseline.cargo_loss_usd.toFixed(2)} →{" "}
                      {latestImpact.mitigated.cargo_loss_usd !== null
                        ? `$${latestImpact.mitigated.cargo_loss_usd.toFixed(2)}`
                        : "null"}
                    </span>
                    <span className="hint" style={{ marginBlockEnd: 0 }}>
                      Ahorro neto:{" "}
                      {latestImpact.deltas.net_agri_savings_usd !== null
                        ? `$${latestImpact.deltas.net_agri_savings_usd.toFixed(2)}`
                        : latestImpact.mitigation_status}
                    </span>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
