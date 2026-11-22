import { useId, useState } from "react";
import {
  ApiClientError,
  patchIncidentVerification,
  type IncidentListItem,
  type RoleCode,
} from "../api/client.ts";
import { IconAlert, IconPlus } from "./Icons.tsx";

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
  PARTIAL_ONE_LANE: "1 carril",
  BLOCKED: "Bloqueado",
};

const FILTER_OPTIONS = [
  { id: "ALL", label: "Todos" },
  { id: "SUSPECTED", label: "Sospecha" },
  { id: "CONFIRMED", label: "Confirmados" },
  { id: "RESOLVED", label: "Resueltos" },
] as const;

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
      onToast(`Incidente marcado como ${STATE_LABELS[verificationState]}.`);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible actualizar el incidente.");
      }
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="panel-stack" aria-labelledby={headingId}>
      <div className="panel-heading-row">
        <div>
          <h2 id={headingId} className="panel-title">
            Alertas en vía
          </h2>
          <p className="panel-subtitle">Derrumbes y cortes reportados</p>
        </div>
        <button type="button" className="btn btn--primary btn--sm" onClick={onOpenReportModal}>
          <IconPlus size={15} />
          <span>Reportar</span>
        </button>
      </div>

      <div className="filter-chips-row" role="group" aria-label="Filtrar estado de incidentes">
        {FILTER_OPTIONS.map((opt) => (
          <button
            key={opt.id}
            type="button"
            className={`filter-chip${stateFilter === opt.id ? " is-active" : ""}`}
            aria-pressed={stateFilter === opt.id}
            onClick={() => setStateFilter(opt.id)}
          >
            {opt.label}
          </button>
        ))}
      </div>

      {!canVerify && !authToken && (
        <div className="auth-nudge-card">
          <span>Personal autorizado valida incidentes</span>
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

      {visibleIncidents.length === 0 ? (
        <div className="empty-state-card">
          <IconAlert size={22} />
          <strong>Sin incidencias activas</strong>
          <span>Las rutas monitoreadas no registran cortes en este filtro.</span>
        </div>
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
                <p className="panel-subtitle" style={{ marginBlock: "0.25rem 0.5rem" }}>
                  Radio {inc.hazard_radius_m} m ·{" "}
                  {new Date(inc.detected_at).toLocaleString("es-PE", {
                    day: "2-digit",
                    month: "short",
                    hour: "2-digit",
                    minute: "2-digit",
                  })}
                </p>

                {canVerify && isActive && (
                  <div className="ops-actions-row">
                    <button
                      type="button"
                      className="btn btn--sm"
                      disabled={!isOnline || busyId === inc.incident_id}
                      onClick={() => void handleVerify(inc.incident_id, "CONFIRMED", "BLOCKED")}
                    >
                      Bloqueo
                    </button>
                    <button
                      type="button"
                      className="btn btn--sm"
                      disabled={!isOnline || busyId === inc.incident_id}
                      onClick={() =>
                        void handleVerify(inc.incident_id, "CONFIRMED", "PARTIAL_ONE_LANE")
                      }
                    >
                      1 carril
                    </button>
                    <button
                      type="button"
                      className="btn btn--sm"
                      disabled={!isOnline || busyId === inc.incident_id}
                      onClick={() =>
                        void handleVerify(inc.incident_id, "RESOLVED", inc.impact_passability)
                      }
                    >
                      Resolver
                    </button>
                    <button
                      type="button"
                      className="btn btn--sm"
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
    </div>
  );
}
