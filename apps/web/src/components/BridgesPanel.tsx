import { useId, useState, type FormEvent } from "react";
import {
  ApiClientError,
  patchBridgeStatus,
  patchInspectTransferCorridor,
  postHumanitarianWindow,
  postProposeBridge,
  postTransferCorridor,
  type BridgesOperationsResponse,
  type CargoType,
  type IncidentListItem,
  type RoadNodeItem,
  type RoleCode,
} from "../api/client.ts";
import { IconBridge } from "./Icons.tsx";

export interface BridgesPanelProps {
  operations: BridgesOperationsResponse | null;
  incidents: IncidentListItem[];
  nodes?: RoadNodeItem[];
  isOnline: boolean;
  authToken: string | null;
  userRole: RoleCode | null;
  onOpenAuthModal: () => void;
  onRefreshOperations: () => Promise<void>;
  onToast: (message: string) => void;
}

const TRANSFER_MODE_LABELS = {
  PEDESTRIAN_SAFE_PATH: "Paso peatonal seguro",
  SHUTTLE_RELAY: "Relevo vehicular",
} as const;

const CARGO_LABELS: Record<CargoType, string> = {
  MEDICAL_SUPPLIES: "Insumos médicos",
  MEDICAL_PATIENT: "Paciente crítico",
  PERISHABLE_AGRI: "Carga agrícola",
};

const BRIDGE_STATUS_LABELS = {
  PROPOSED: "Propuesto",
  ACTIVE: "Activo",
  SUSPENDED_UNSAFE: "Suspendido",
  COMPLETED: "Completado",
} as const;

const DIRECTION_LABELS = {
  ORIGIN_TO_DEST: "Ida",
  DEST_TO_ORIGIN: "Retorno",
} as const;

export function BridgesPanel({
  operations,
  incidents,
  nodes = [],
  isOnline,
  authToken,
  userRole,
  onOpenAuthModal,
  onRefreshOperations,
  onToast,
}: BridgesPanelProps) {
  const headingId = useId();
  const [incidentId, setIncidentId] = useState("");
  const [corridorId, setCorridorId] = useState("");
  const [cargoType, setCargoType] = useState<CargoType>("MEDICAL_SUPPLIES");
  const [winIncidentId, setWinIncidentId] = useState("");
  const [vehicleCategoryId, setVehicleCategoryId] = useState("");
  const [direction, setDirection] = useState<"ORIGIN_TO_DEST" | "DEST_TO_ORIGIN">(
    "ORIGIN_TO_DEST"
  );
  const [durationMin, setDurationMin] = useState("90");
  const [sideANodeId, setSideANodeId] = useState("");
  const [sideBNodeId, setSideBNodeId] = useState("");
  const [tcMode, setTcMode] = useState<"PEDESTRIAN_SAFE_PATH" | "SHUTTLE_RELAY">(
    "PEDESTRIAN_SAFE_PATH"
  );
  const [tcLengthM, setTcLengthM] = useState("180");
  const [tcMaxSlopePct, setTcMaxSlopePct] = useState("12");
  const [busy, setBusy] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const canProposeBridge = Boolean(
    authToken &&
      (userRole === "COER_ADMIN" ||
        userRole === "CIVIL_DEFENSE" ||
        userRole === "HEALTH_OPERATOR")
  );
  const canAuthorizeOperations = Boolean(
    authToken && (userRole === "COER_ADMIN" || userRole === "CIVIL_DEFENSE")
  );

  const transferCorridors = operations?.transfer_corridors ?? [];
  const bridges = operations?.bridges ?? [];
  const windows = operations?.humanitarian_windows ?? [];
  const vehicles = operations?.vehicle_categories ?? [];
  const confirmedPartialIncidents = incidents.filter(
    (i) => i.verification_state === "CONFIRMED" && i.impact_passability === "PARTIAL_ONE_LANE"
  );

  const effIncident = incidentId || incidents[0]?.incident_id || "";
  const effCorridor = corridorId || transferCorridors[0]?.corridor_id || "";
  const effWinIncident = winIncidentId || confirmedPartialIncidents[0]?.incident_id || "";
  const effVehicleCat = vehicleCategoryId || vehicles[0]?.category_id || "";
  const effSideA = sideANodeId || nodes[0]?.node_id || "";
  const effSideB = sideBNodeId || nodes[Math.min(1, Math.max(0, nodes.length - 1))]?.node_id || "";

  async function handleInspectCorridor(targetId: string) {
    if (!authToken) return;
    setErrorMsg(null);
    setBusy(true);
    try {
      await patchInspectTransferCorridor(
        targetId,
        { is_verified_safe: true, inspection_valid_days: 30, notes: "Inspeccion vigente" },
        authToken
      );
      await onRefreshOperations();
      onToast("Inspección del paso de transbordo revalidada.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible revalidar el paso.");
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateTransferCorridor(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !effSideA || !effSideB || effSideA === effSideB) {
      setErrorMsg("Selecciona dos nodos distintos para el paso de transbordo.");
      return;
    }
    setErrorMsg(null);
    setBusy(true);
    try {
      await postTransferCorridor(
        {
          side_a_node_id: effSideA,
          side_b_node_id: effSideB,
          length_m: Math.max(10, Number(tcLengthM) || 180),
          max_slope_pct: Math.max(0, Number(tcMaxSlopePct) || 12),
          transfer_mode: tcMode,
          max_safe_ivv: 69,
          inspection_valid_days: 30,
          capabilities: [
            {
              cargo_type: "MEDICAL_SUPPLIES",
              max_payload_kg: 1500,
              requires_medical_supervision: false,
              estimated_transfer_time_min: 20,
            },
            {
              cargo_type: "PERISHABLE_AGRI",
              max_payload_kg: 3000,
              requires_medical_supervision: false,
              estimated_transfer_time_min: 35,
            },
          ],
        },
        authToken
      );
      await onRefreshOperations();
      onToast("Paso de transbordo certificado.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible registrar el paso de transbordo.");
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleProposeBridge(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !effIncident || !effCorridor) return;
    setErrorMsg(null);
    setBusy(true);
    try {
      await postProposeBridge(
        {
          incident_id: effIncident,
          corridor_id: effCorridor,
          cargo_type: cargoType,
        },
        authToken
      );
      await onRefreshOperations();
      onToast("Puente logístico propuesto.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible proponer el puente.");
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleUpdateBridgeStatus(
    bridgeId: string,
    status: "ACTIVE" | "SUSPENDED_UNSAFE" | "COMPLETED"
  ) {
    if (!authToken) return;
    setErrorMsg(null);
    setBusy(true);
    try {
      await patchBridgeStatus(bridgeId, status, authToken);
      await onRefreshOperations();
      onToast(`Puente actualizado a ${BRIDGE_STATUS_LABELS[status]}.`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible actualizar el puente.");
      }
    } finally {
      setBusy(false);
    }
  }

  async function handleCreateWindow(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !effWinIncident || !effVehicleCat) return;
    setErrorMsg(null);
    setBusy(true);
    try {
      const start = new Date();
      const mins = Math.max(15, Number(durationMin) || 90);
      const end = new Date(start.getTime() + mins * 60_000);
      await postHumanitarianWindow(
        effWinIncident,
        {
          vehicle_category_id: effVehicleCat,
          direction,
          starts_at: start.toISOString(),
          ends_at: end.toISOString(),
        },
        authToken
      );
      await onRefreshOperations();
      onToast("Ventana humanitaria programada.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible programar la ventana.");
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel-stack" aria-labelledby={headingId}>
      <div className="panel-heading-row">
        <div>
          <h2 id={headingId} className="panel-title">
            Puentes y Ventanas
          </h2>
          <p className="panel-subtitle">Transbordos y paso alternado</p>
        </div>
      </div>

      {!authToken && (
        <div className="auth-nudge-card">
          <span>Autoriza puentes con tu cuenta</span>
          <button type="button" className="btn btn--sm" onClick={onOpenAuthModal}>
            Entrar
          </button>
        </div>
      )}

      {errorMsg && (
        <div className="banner banner--danger" role="alert">
          <span>{errorMsg}</span>
        </div>
      )}

      {bridges.length === 0 && windows.length === 0 && transferCorridors.length === 0 ? (
        <div className="empty-state-card">
          <IconBridge size={22} />
          <strong>Sin puentes activos</strong>
          <span>Cuando exista un corte de vía podrás habilitar transbordos aquí.</span>
        </div>
      ) : (
        <>
          {bridges.length > 0 && (
            <div className="group-block">
              <span className="section-eyebrow">Puentes logísticos ({bridges.length})</span>
              <ul className="ops-card-list">
                {bridges.map((b) => (
                  <li key={b.bridge_id} className="ops-list-item">
                    <div className="ops-item-top">
                      <strong>{CARGO_LABELS[b.cargo_type]}</strong>
                      <span
                        className={`risk-badge ${
                          b.status === "ACTIVE"
                            ? "risk-badge--GREEN"
                            : b.status === "SUSPENDED_UNSAFE"
                              ? "risk-badge--RED"
                              : "risk-badge--AMBER"
                        }`}
                      >
                        {BRIDGE_STATUS_LABELS[b.status]}
                      </span>
                    </div>
                    {canAuthorizeOperations && b.status !== "COMPLETED" && (
                      <div className="ops-actions-row" style={{ marginBlockStart: "0.5rem" }}>
                        {b.status !== "ACTIVE" && (
                          <button
                            type="button"
                            className="btn btn--sm"
                            disabled={!isOnline || busy}
                            onClick={() => void handleUpdateBridgeStatus(b.bridge_id, "ACTIVE")}
                          >
                            Activar
                          </button>
                        )}
                        {b.status !== "SUSPENDED_UNSAFE" && (
                          <button
                            type="button"
                            className="btn btn--sm"
                            disabled={!isOnline || busy}
                            onClick={() =>
                              void handleUpdateBridgeStatus(b.bridge_id, "SUSPENDED_UNSAFE")
                            }
                          >
                            Suspender
                          </button>
                        )}
                        <button
                          type="button"
                          className="btn btn--sm"
                          disabled={!isOnline || busy}
                          onClick={() => void handleUpdateBridgeStatus(b.bridge_id, "COMPLETED")}
                        >
                          Completar
                        </button>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {windows.length > 0 && (
            <div className="group-block">
              <span className="section-eyebrow">Ventanas humanitarias ({windows.length})</span>
              <ul className="ops-card-list">
                {windows.map((w) => (
                  <li key={w.window_id} className="ops-list-item">
                    <div className="ops-item-top">
                      <strong>
                        {w.vehicle_category_code} · {DIRECTION_LABELS[w.direction]}
                      </strong>
                      <span className="risk-badge risk-badge--GREEN">{w.status}</span>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {transferCorridors.length > 0 && (
            <div className="group-block">
              <span className="section-eyebrow">
                Pasos inspeccionados ({transferCorridors.length})
              </span>
              <ul className="ops-card-list">
                {transferCorridors.map((c) => (
                  <li key={c.corridor_id} className="ops-list-item">
                    <div className="ops-item-top">
                      <strong>
                        {TRANSFER_MODE_LABELS[c.transfer_mode]} ({c.length_m} m)
                      </strong>
                      <span
                        className={`risk-badge ${
                          c.requires_revalidation || !c.is_verified_safe
                            ? "risk-badge--RED"
                            : "risk-badge--GREEN"
                        }`}
                      >
                        {c.is_verified_safe ? "Habilitado" : "Revalidar"}
                      </span>
                    </div>
                    {canAuthorizeOperations && (
                      <div className="ops-actions-row" style={{ marginBlockStart: "0.45rem" }}>
                        <button
                          type="button"
                          className="btn btn--sm"
                          disabled={!isOnline || busy}
                          onClick={() => void handleInspectCorridor(c.corridor_id)}
                        >
                          Revalidar inspección
                        </button>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </>
      )}

      {canProposeBridge && incidents.length > 0 && transferCorridors.length > 0 && (
        <details className="disclosure-card">
          <summary>Proponer nuevo puente</summary>
          <form onSubmit={handleProposeBridge} className="clean-form" style={{ marginBlockStart: "0.75rem" }}>
            <div className="field field--tight">
              <label htmlFor="bridge-incident">Incidente</label>
              <select
                id="bridge-incident"
                className="brand-select"
                value={effIncident}
                onChange={(e) => setIncidentId(e.target.value)}
              >
                {incidents.map((i) => (
                  <option key={i.incident_id} value={i.incident_id}>
                    {i.corridor_name}
                  </option>
                ))}
              </select>
            </div>
            <div className="field field--tight">
              <label htmlFor="bridge-corridor">Paso de transbordo</label>
              <select
                id="bridge-corridor"
                className="brand-select"
                value={effCorridor}
                onChange={(e) => setCorridorId(e.target.value)}
              >
                {transferCorridors.map((c) => (
                  <option key={c.corridor_id} value={c.corridor_id}>
                    {TRANSFER_MODE_LABELS[c.transfer_mode]} ({c.length_m} m)
                  </option>
                ))}
              </select>
            </div>
            <div className="field field--tight">
              <label htmlFor="bridge-cargo">Tipo de carga</label>
              <select
                id="bridge-cargo"
                className="brand-select"
                value={cargoType}
                onChange={(e) => setCargoType(e.target.value as CargoType)}
              >
                <option value="MEDICAL_SUPPLIES">Insumos médicos</option>
                <option value="MEDICAL_PATIENT">Paciente crítico</option>
                <option value="PERISHABLE_AGRI">Carga agrícola</option>
              </select>
            </div>
            <button type="submit" className="btn btn--primary btn--block" disabled={!isOnline || busy}>
              Proponer puente
            </button>
          </form>
        </details>
      )}

      {canAuthorizeOperations && confirmedPartialIncidents.length > 0 && vehicles.length > 0 && (
        <details className="disclosure-card">
          <summary>Programar ventana humanitaria</summary>
          <form onSubmit={handleCreateWindow} className="clean-form" style={{ marginBlockStart: "0.75rem" }}>
            <div className="field field--tight">
              <label htmlFor="win-incident">Incidente (1 carril)</label>
              <select
                id="win-incident"
                className="brand-select"
                value={effWinIncident}
                onChange={(e) => setWinIncidentId(e.target.value)}
              >
                {confirmedPartialIncidents.map((i) => (
                  <option key={i.incident_id} value={i.incident_id}>
                    {i.corridor_name}
                  </option>
                ))}
              </select>
            </div>
            <div className="field field--tight">
              <label htmlFor="win-vehicle">Vehículo autorizado</label>
              <select
                id="win-vehicle"
                className="brand-select"
                value={effVehicleCat}
                onChange={(e) => setVehicleCategoryId(e.target.value)}
              >
                {vehicles.map((v) => (
                  <option key={v.category_id} value={v.category_id}>
                    {v.code}
                  </option>
                ))}
              </select>
            </div>
            <div className="two-col-inline">
              <div className="field field--tight">
                <label htmlFor="win-dir">Sentido</label>
                <select
                  id="win-dir"
                  className="brand-select"
                  value={direction}
                  onChange={(e) =>
                    setDirection(e.target.value as "ORIGIN_TO_DEST" | "DEST_TO_ORIGIN")
                  }
                >
                  <option value="ORIGIN_TO_DEST">Ida</option>
                  <option value="DEST_TO_ORIGIN">Retorno</option>
                </select>
              </div>
              <div className="field field--tight">
                <label htmlFor="win-duration">Minutos</label>
                <input
                  id="win-duration"
                  type="text"
                  inputMode="numeric"
                  pattern="^\d+$"
                  value={durationMin}
                  onChange={(e) => setDurationMin(e.target.value)}
                />
              </div>
            </div>
            <button type="submit" className="btn btn--primary btn--block" disabled={!isOnline || busy}>
              Autorizar ventana
            </button>
          </form>
        </details>
      )}

      {canAuthorizeOperations && nodes.length >= 2 && (
        <details className="disclosure-card">
          <summary>Registrar paso de transbordo</summary>
          <form
            onSubmit={handleCreateTransferCorridor}
            className="clean-form"
            style={{ marginBlockStart: "0.75rem" }}
          >
            <div className="field field--tight">
              <label htmlFor="tc-side-a">Nodo lado A</label>
              <select
                id="tc-side-a"
                className="brand-select"
                value={effSideA}
                onChange={(e) => setSideANodeId(e.target.value)}
              >
                {nodes.slice(0, 60).map((n) => (
                  <option key={n.node_id} value={n.node_id}>
                    {n.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="field field--tight">
              <label htmlFor="tc-side-b">Nodo lado B</label>
              <select
                id="tc-side-b"
                className="brand-select"
                value={effSideB}
                onChange={(e) => setSideBNodeId(e.target.value)}
              >
                {nodes.slice(0, 60).map((n) => (
                  <option key={n.node_id} value={n.node_id}>
                    {n.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="field field--tight">
              <label htmlFor="tc-mode">Modalidad</label>
              <select
                id="tc-mode"
                className="brand-select"
                value={tcMode}
                onChange={(e) =>
                  setTcMode(e.target.value as "PEDESTRIAN_SAFE_PATH" | "SHUTTLE_RELAY")
                }
              >
                <option value="PEDESTRIAN_SAFE_PATH">Paso peatonal seguro</option>
                <option value="SHUTTLE_RELAY">Relevo vehicular</option>
              </select>
            </div>
            <div className="two-col-inline">
              <div className="field field--tight">
                <label htmlFor="tc-len">Longitud (m)</label>
                <input
                  id="tc-len"
                  type="text"
                  inputMode="numeric"
                  value={tcLengthM}
                  onChange={(e) => setTcLengthM(e.target.value)}
                />
              </div>
              <div className="field field--tight">
                <label htmlFor="tc-slope">Pendiente %</label>
                <input
                  id="tc-slope"
                  type="text"
                  inputMode="decimal"
                  value={tcMaxSlopePct}
                  onChange={(e) => setTcMaxSlopePct(e.target.value)}
                />
              </div>
            </div>
            <button type="submit" className="btn btn--primary btn--block" disabled={!isOnline || busy}>
              Certificar paso de transbordo
            </button>
          </form>
        </details>
      )}
    </div>
  );
}
