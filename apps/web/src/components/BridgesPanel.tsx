import { useId, useState, type FormEvent } from "react";
import {
  ApiClientError,
  patchBridgeStatus,
  postHumanitarianWindow,
  postProposeBridge,
  type BridgesOperationsResponse,
  type CargoType,
  type IncidentListItem,
  type RoleCode,
} from "../api/client.ts";

export interface BridgesPanelProps {
  operations: BridgesOperationsResponse | null;
  incidents: IncidentListItem[];
  isOnline: boolean;
  authToken: string | null;
  userRole: RoleCode | null;
  onOpenAuthModal: () => void;
  onRefreshOperations: () => Promise<void>;
  onToast: (message: string) => void;
}

export function BridgesPanel({
  operations,
  incidents,
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
      onToast("Puente logístico propuesto correctamente.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible proponer el puente logístico.");
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
      onToast(`Estado de puente actualizado a ${status}.`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible actualizar el puente logístico.");
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
      onToast("Ventana humanitaria programada correctamente.");
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible programar la ventana humanitaria.");
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card" aria-labelledby={headingId}>
      <h2 id={headingId}>Puentes Logísticos y Ventanas Humanitarias</h2>
      <p className="hint">
        Gestión de transbordos verificados y ventanas direccionales en paso alternado. Un operador
        de salud puede proponer puentes, pero solo COER o Defensa Civil activan el corredor.
      </p>

      {!authToken && (
        <div className="banner banner--info" style={{ marginBlockEnd: "0.75rem" }}>
          <span>Inicie sesión institucional para proponer o autorizar puentes y ventanas.</span>
          <button type="button" onClick={onOpenAuthModal}>
            Iniciar sesión
          </button>
        </div>
      )}

      {errorMsg && (
        <div className="banner banner--danger" role="alert" style={{ marginBlockEnd: "0.75rem" }}>
          <span>{errorMsg}</span>
        </div>
      )}

      <h3>Corredores de Transbordo Inspeccionados ({transferCorridors.length})</h3>
      {transferCorridors.length === 0 ? (
        <p className="hint">No hay corredores peatonales o de relevo cargados aún.</p>
      ) : (
        <ul className="ops-card-list" style={{ marginBlockEnd: "1rem" }}>
          {transferCorridors.map((c) => (
            <li key={c.corridor_id} className="ops-list-item">
              <div className="ops-item-top">
                <strong>
                  {c.transfer_mode} ({c.length_m} m)
                </strong>
                <span
                  className={`risk-badge ${
                    c.requires_revalidation || !c.is_verified_safe
                      ? "risk-badge--RED"
                      : "risk-badge--GREEN"
                  }`}
                >
                  {c.requires_revalidation
                    ? "REVALIDACIÓN REQUERIDA"
                    : c.is_verified_safe
                      ? `IVV máx ${c.max_safe_ivv.toFixed(0)}`
                      : "NO VERIFICADO"}
                </span>
              </div>
              <p className="hint" style={{ marginBlock: "0.25rem 0" }}>
                Pendiente: {c.max_slope_pct.toFixed(1)}% · Cargas: {c.cargo_types.join(", ") || "—"}
              </p>
            </li>
          ))}
        </ul>
      )}

      <h3>Puentes Logísticos Activos / Propuestos ({bridges.length})</h3>
      {bridges.length === 0 ? (
        <p className="hint">No hay puentes logísticos registrados actualmente.</p>
      ) : (
        <ul className="ops-card-list" style={{ marginBlockEnd: "1rem" }}>
          {bridges.map((b) => (
            <li key={b.bridge_id} className="ops-list-item">
              <div className="ops-item-top">
                <strong>{b.cargo_type}</strong>
                <span
                  className={`risk-badge ${
                    b.status === "ACTIVE"
                      ? "risk-badge--GREEN"
                      : b.status === "SUSPENDED_UNSAFE"
                        ? "risk-badge--RED"
                        : "risk-badge--AMBER"
                  }`}
                >
                  {b.status}
                </span>
              </div>
              <p className="hint" style={{ marginBlock: "0.25rem 0.5rem" }}>
                Creado: {new Date(b.created_at).toLocaleString("es-PE")}
              </p>
              {canAuthorizeOperations && b.status !== "COMPLETED" && (
                <div className="ops-actions-row">
                  {b.status !== "ACTIVE" && (
                    <button
                      type="button"
                      disabled={!isOnline || busy}
                      onClick={() => void handleUpdateBridgeStatus(b.bridge_id, "ACTIVE")}
                    >
                      Activar
                    </button>
                  )}
                  {b.status !== "SUSPENDED_UNSAFE" && (
                    <button
                      type="button"
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
      )}

      {canProposeBridge && incidents.length > 0 && transferCorridors.length > 0 && (
        <form onSubmit={handleProposeBridge} className="subform-box">
          <h3>Proponer Puente Logístico</h3>
          <div className="field">
            <label htmlFor="bridge-incident">Incidente asociado</label>
            <select
              id="bridge-incident"
              className="brand-select"
              value={effIncident}
              onChange={(e) => setIncidentId(e.target.value)}
            >
              {incidents.map((i) => (
                <option key={i.incident_id} value={i.incident_id}>
                  {i.corridor_name} ({i.verification_state} · {i.impact_passability})
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="bridge-corridor">Corredor de transbordo</label>
            <select
              id="bridge-corridor"
              className="brand-select"
              value={effCorridor}
              onChange={(e) => setCorridorId(e.target.value)}
            >
              {transferCorridors.map((c) => (
                <option key={c.corridor_id} value={c.corridor_id}>
                  {c.transfer_mode} ({c.length_m} m)
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="bridge-cargo">Tipo de carga</label>
            <select
              id="bridge-cargo"
              className="brand-select"
              value={cargoType}
              onChange={(e) => setCargoType(e.target.value as CargoType)}
            >
              <option value="MEDICAL_SUPPLIES">MEDICAL_SUPPLIES</option>
              <option value="MEDICAL_PATIENT">MEDICAL_PATIENT</option>
              <option value="PERISHABLE_AGRI">PERISHABLE_AGRI</option>
            </select>
          </div>
          <button type="submit" className="btn btn--primary" disabled={!isOnline || busy}>
            Proponer puente
          </button>
        </form>
      )}

      <h3 style={{ marginBlockStart: "1rem" }}>Ventanas Humanitarias ({windows.length})</h3>
      {windows.length === 0 ? (
        <p className="hint">No existen ventanas humanitarias registradas.</p>
      ) : (
        <ul className="ops-card-list" style={{ marginBlockEnd: "1rem" }}>
          {windows.map((w) => (
            <li key={w.window_id} className="ops-list-item">
              <div className="ops-item-top">
                <strong>
                  {w.vehicle_category_code} ({w.direction})
                </strong>
                <span className="risk-badge risk-badge--GREEN">{w.status}</span>
              </div>
              <p className="hint" style={{ marginBlock: "0.25rem 0" }}>
                {new Date(w.starts_at).toLocaleString("es-PE")} →{" "}
                {new Date(w.ends_at).toLocaleString("es-PE")}
              </p>
            </li>
          ))}
        </ul>
      )}

      {canAuthorizeOperations &&
        (confirmedPartialIncidents.length === 0 || vehicles.length === 0 ? (
          <p className="hint">
            Para programar una ventana humanitaria confirme primero un incidente{" "}
            <code>PARTIAL_ONE_LANE</code> en la pestaña Incidentes.
          </p>
        ) : (
          <form onSubmit={handleCreateWindow} className="subform-box">
            <h3>Programar Ventana Humanitaria</h3>
            <div className="field">
              <label htmlFor="win-incident">Incidente confirmado (1 carril)</label>
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
            <div className="field">
              <label htmlFor="win-vehicle">Categoría vehicular autorizada</label>
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
            <div className="field">
              <label htmlFor="win-dir">Sentido habilitado</label>
              <select
                id="win-dir"
                className="brand-select"
                value={direction}
                onChange={(e) =>
                  setDirection(e.target.value as "ORIGIN_TO_DEST" | "DEST_TO_ORIGIN")
                }
              >
                <option value="ORIGIN_TO_DEST">ORIGIN_TO_DEST (Ida)</option>
                <option value="DEST_TO_ORIGIN">DEST_TO_ORIGIN (Retorno)</option>
              </select>
            </div>
            <div className="field">
              <label htmlFor="win-duration">Duración desde ahora (minutos)</label>
              <input
                id="win-duration"
                type="text"
                inputMode="numeric"
                pattern="^\d+$"
                value={durationMin}
                onChange={(e) => setDurationMin(e.target.value)}
              />
            </div>
            <button type="submit" className="btn btn--primary" disabled={!isOnline || busy}>
              Autorizar ventana
            </button>
          </form>
        ))}
    </section>
  );
}
