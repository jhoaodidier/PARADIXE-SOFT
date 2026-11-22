import { useEffect, useId, useRef, useState } from "react";
import {
  ApiClientError,
  patchIncidentVerification,
  type IncidentListItem,
  type RoleCode,
} from "../api/client.ts";
import { attachDialogLightDismissFallback, closeTopLayer } from "../utils/topLayer.ts";
import { IconAlert, IconPlus, IconX } from "./Icons.tsx";

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
  const lightboxTitleId = useId();
  const lightboxRef = useRef<HTMLDialogElement | null>(null);

  const [stateFilter, setStateFilter] = useState<string>("ALL");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [lightboxPhoto, setLightboxPhoto] = useState<{
    uri: string;
    corridor: string;
    detectedAt: string;
  } | null>(null);

  const canVerify = Boolean(
    authToken && (userRole === "COER_ADMIN" || userRole === "CIVIL_DEFENSE")
  );

  const visibleIncidents =
    stateFilter === "ALL"
      ? incidents
      : incidents.filter((inc) => inc.verification_state === stateFilter);

  useEffect(() => {
    const dialog = lightboxRef.current;
    if (!dialog) return;

    const detachFallback = attachDialogLightDismissFallback(dialog);
    const onDialogClose = () => {
      dialog.returnValue = "";
      setLightboxPhoto(null);
    };
    const onCancel = (e: Event) => {
      e.preventDefault();
      void closeTopLayer(dialog);
    };

    dialog.addEventListener("close", onDialogClose);
    dialog.addEventListener("cancel", onCancel);
    return () => {
      detachFallback();
      dialog.removeEventListener("close", onDialogClose);
      dialog.removeEventListener("cancel", onCancel);
    };
  }, []);

  useEffect(() => {
    const dialog = lightboxRef.current;
    if (!dialog) return;
    if (lightboxPhoto && !dialog.open) {
      dialog.showModal();
    } else if (!lightboxPhoto && dialog.open) {
      void closeTopLayer(dialog);
    }
  }, [lightboxPhoto]);

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
            const photos = inc.evidence_photos ?? [];

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

                {photos.length > 0 && (
                  <div className="evidence-thumb-strip" role="group" aria-label="Fotos de evidencia">
                    {photos.map((uri, idx) => (
                      <button
                        key={`${inc.incident_id}-pic-${idx}`}
                        type="button"
                        className="evidence-thumb-btn"
                        aria-label={`Ampliar foto ${idx + 1} de ${inc.corridor_name}`}
                        onClick={() =>
                          setLightboxPhoto({
                            uri,
                            corridor: inc.corridor_name,
                            detectedAt: inc.detected_at,
                          })
                        }
                      >
                        <img
                          src={uri}
                          alt={`Evidencia en ${inc.corridor_name}`}
                          width={72}
                          height={54}
                          loading="lazy"
                          className="evidence-list-thumb"
                        />
                      </button>
                    ))}
                  </div>
                )}

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

      <dialog
        ref={lightboxRef}
        className="modal-dialog modal-dialog--lightbox"
        aria-labelledby={lightboxTitleId}
        {...({ closedby: "any" } as Record<string, string>)}
      >
        <div className="modal-content">
          <div className="modal-header">
            <div>
              <h2 id={lightboxTitleId}>{lightboxPhoto?.corridor ?? "Evidencia en campo"}</h2>
              {lightboxPhoto && (
                <p className="panel-subtitle">
                  Capturada el {new Date(lightboxPhoto.detectedAt).toLocaleString("es-PE")}
                </p>
              )}
            </div>
            <button
              type="button"
              className="icon-btn"
              autoFocus
              aria-label="Cerrar foto ampliada"
              onClick={() => lightboxRef.current && void closeTopLayer(lightboxRef.current)}
            >
              <IconX size={18} />
            </button>
          </div>
          {lightboxPhoto && (
            <img
              src={lightboxPhoto.uri}
              alt={`Evidencia ampliada de ${lightboxPhoto.corridor}`}
              className="lightbox-full-img"
            />
          )}
        </div>
      </dialog>
    </div>
  );
}
