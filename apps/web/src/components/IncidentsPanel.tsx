import { useId, useState } from "react";
import {
  ApiClientError,
  patchIncidentVerification,
  type IncidentListItem,
  type RoleCode,
} from "../api/client.ts";

export interface IncidentsPanelProps {
  incidents: IncidentListItem[];
  isOnline: boolean;
  authToken: string | null;
  userRole: RoleCode | null;
  onOpenReportModal: () => void;
  onOpenAuthModal: () => void;
  onRefreshOperations: () => Promise<void>;
  onToast: (message: string) => void;
}

const STATE_LABELS: Record<IncidentListItem["verification_state"], string> = {
  SUSPECTED: "Sospecha",
  CONFIRMED: "Confirmado",
  RESOLVED: "Resuelto",
  DISMISSED: "Descartado",
};

const PASSABILITY_LABELS: Record<IncidentListItem["impact_passability"], string> = {
  CAUTION_SUSPECTED: "Precaución",
  PARTIAL_ONE_LANE: "Paso 1 carril",
  BLOCKED: "Bloqueado",
};

export function IncidentsPanel({
  incidents,
  isOnline,
  authToken,
  userRole,
  onOpenReportModal,
  onOpenAuthModal,
  onRefreshOperations,
  onToast,
}: IncidentsPanelProps) {
  const headingId = useId();
  const filterId = useId();
  const [stateFilter, setStateFilter] = useState<string>("ALL");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const canVerify = Boolean(
    authToken && (userRole === "COER_ADMIN" || userRole === "CIVIL_DEFENSE")
  );

  const visibleIncidents =
    stateFilter === "ALL"
      ? incidents
      : incidents.filter((inc) => inc.verification_state === stateFilter);

  async function handleVerify(
    incidentId: string,
    verificationState: "CONFIRMED" | "RESOLVED" | "DISMISSED",
    impactPassability: "CAUTION_SUSPECTED" | "PARTIAL_ONE_LANE" | "BLOCKED"
  ) {
    if (!authToken) return;
    setErrorMsg(null);
    setBusyId(incidentId);
    try {
      await patchIncidentVerification(
        incidentId,
        {
          verification_state: verificationState,
          impact_passability: impactPassability,
        },
        authToken
      );
      await onRefreshOperations();
      onToast(`Incidente actualizado a ${verificationState} (${impactPassability}).`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible actualizar la verificación del incidente.");
      }
    } finally {
      setBusyId(null);
    }
  }

  return (
    <section className="card" aria-labelledby={headingId}>
      <div className="panel-header-row">
        <h2 id={headingId} style={{ marginBlockEnd: 0 }}>
          Incidentes y Consenso
        </h2>
        <button type="button" className="btn btn--primary" onClick={onOpenReportModal}>
          Nuevo reporte
        </button>
      </div>
      <p className="hint">
        Los clústeres de telemetría solo generan estado <code>SUSPECTED</code>; la confirmación
        operativa exige consenso comunitario o validación COER/Defensa Civil.
      </p>

      {!canVerify && (
        <div className="banner banner--info" style={{ marginBlockEnd: "0.75rem" }}>
          <span>
            Solo <strong>COER_ADMIN</strong> o <strong>CIVIL_DEFENSE</strong> pueden confirmar o
            resolver incidentes.
          </span>
          {!authToken && (
            <button type="button" onClick={onOpenAuthModal}>
              Iniciar sesión
            </button>
          )}
        </div>
      )}

      {errorMsg && (
        <div className="banner banner--danger" role="alert" style={{ marginBlockEnd: "0.75rem" }}>
          <span>{errorMsg}</span>
        </div>
      )}

      <div className="field">
        <label htmlFor={filterId}>Filtrar estado</label>
        <select
          id={filterId}
          value={stateFilter}
          onChange={(e) => setStateFilter(e.target.value)}
        >
          <option value="ALL">Todos ({incidents.length})</option>
          <option value="SUSPECTED">Sospecha (SUSPECTED)</option>
          <option value="CONFIRMED">Confirmados (CONFIRMED)</option>
          <option value="RESOLVED">Resueltos (RESOLVED)</option>
          <option value="DISMISSED">Descartados (DISMISSED)</option>
        </select>
      </div>

      {visibleIncidents.length === 0 ? (
        <p className="hint">
          No hay incidentes registrados para el filtro seleccionado. Use &quot;Reportar vía&quot; o
          &quot;Nuevo reporte&quot; para registrar un evento geotécnico.
        </p>
      ) : (
        <ul className="ops-card-list">
          {visibleIncidents.map((inc) => {
            const isActive =
              inc.verification_state === "SUSPECTED" || inc.verification_state === "CONFIRMED";
            const badgeClass =
              inc.verification_state === "CONFIRMED"
                ? "risk-badge--RED"
                : inc.verification_state === "SUSPECTED"
                  ? "risk-badge--AMBER"
                  : inc.verification_state === "RESOLVED"
                    ? "risk-badge--GREEN"
                    : "risk-badge--UNKNOWN";

            return (
              <li key={inc.incident_id} className="ops-list-item">
                <div className="ops-item-top">
                  <strong>{inc.corridor_name}</strong>
                  <span className={`risk-badge ${badgeClass}`}>
                    {STATE_LABELS[inc.verification_state]} ·{" "}
                    {PASSABILITY_LABELS[inc.impact_passability]}
                  </span>
                </div>
                <p className="hint" style={{ marginBlock: "0.35rem 0.5rem" }}>
                  Origen: {inc.trigger_source} · Radio {inc.hazard_radius_m} m · (
                  {inc.hazard_lat.toFixed(4)}, {inc.hazard_lon.toFixed(4)}) ·{" "}
                  {new Date(inc.detected_at).toLocaleString("es-PE")}
                </p>

                {canVerify && isActive && (
                  <div className="ops-actions-row">
                    <button
                      type="button"
                      disabled={!isOnline || busyId === inc.incident_id}
                      onClick={() => void handleVerify(inc.incident_id, "CONFIRMED", "BLOCKED")}
                    >
                      Confirmar bloqueo
                    </button>
                    <button
                      type="button"
                      disabled={!isOnline || busyId === inc.incident_id}
                      onClick={() =>
                        void handleVerify(inc.incident_id, "CONFIRMED", "PARTIAL_ONE_LANE")
                      }
                    >
                      1 carril
                    </button>
                    <button
                      type="button"
                      disabled={!isOnline || busyId === inc.incident_id}
                      onClick={() =>
                        void handleVerify(inc.incident_id, "RESOLVED", inc.impact_passability)
                      }
                    >
                      Resolver
                    </button>
                    <button
                      type="button"
                      disabled={!isOnline || busyId === inc.incident_id}
                      onClick={() =>
                        void handleVerify(inc.incident_id, "DISMISSED", inc.impact_passability)
                      }
                    >
                      Descartar
                    </button>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
