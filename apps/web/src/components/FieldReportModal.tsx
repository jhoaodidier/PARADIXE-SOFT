import { useEffect, useId, useRef, useState, type ChangeEvent, type FormEvent } from "react";
import {
  postFieldReport,
  postUploadEvidence,
  type CorridorSegmentFeature,
  type ProposedPassability,
} from "../api/client.ts";
import { enqueueFieldReport } from "../offline/db.ts";
import { attachDialogLightDismissFallback, closeTopLayer } from "../utils/topLayer.ts";
import { IconCamera, IconCrosshair, IconX } from "./Icons.tsx";

async function compressEvidenceImage(file: File): Promise<Blob> {
  if (typeof createImageBitmap !== "function") {
    return file;
  }
  try {
    const bitmap = await createImageBitmap(file);
    const maxDim = 1280;
    const scale = Math.min(1, maxDim / Math.max(bitmap.width, bitmap.height));
    const width = Math.max(1, Math.round(bitmap.width * scale));
    const height = Math.max(1, Math.round(bitmap.height * scale));
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return file;
    ctx.drawImage(bitmap, 0, 0, width, height);
    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob(resolve, "image/jpeg", 0.78)
    );
    bitmap.close?.();
    return blob ?? file;
  } catch {
    return file;
  }
}

export interface FieldReportModalProps {
  open: boolean;
  onClose: () => void;
  features: CorridorSegmentFeature[];
  defaultSegmentId: string | null;
  isOnline: boolean;
  authToken: string | null;
  onQueuedOrSent: (message: string) => void;
}

export function FieldReportModal({
  open,
  onClose,
  features,
  defaultSegmentId,
  isOnline,
  authToken,
  onQueuedOrSent,
}: FieldReportModalProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const titleId = useId();
  const latErrorId = useId();
  const lonErrorId = useId();

  const [segmentId, setSegmentId] = useState("");
  const [passability, setPassability] = useState<ProposedPassability>("BLOCKED");
  const [latitude, setLatitude] = useState("-9.7120");
  const [longitude, setLongitude] = useState("-76.0910");
  const [photoBlob, setPhotoBlob] = useState<Blob | null>(null);
  const [photoPreviewUrl, setPhotoPreviewUrl] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const effectiveSegmentId =
    segmentId || defaultSegmentId || features[0]?.properties.segment_id || "";

  useEffect(() => {
    return () => {
      if (photoPreviewUrl) URL.revokeObjectURL(photoPreviewUrl);
    };
  }, [photoPreviewUrl]);

  async function handlePhotoSelected(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) return;
    const compressed = await compressEvidenceImage(file);
    if (photoPreviewUrl) URL.revokeObjectURL(photoPreviewUrl);
    setPhotoBlob(compressed);
    setPhotoPreviewUrl(typeof URL.createObjectURL === "function" ? URL.createObjectURL(compressed) : null);
  }

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    const detachFallback = attachDialogLightDismissFallback(dialog);
    const onDialogClose = () => {
      dialog.returnValue = "";
      onClose();
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
  }, [onClose]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
    } else if (!open && dialog.open) {
      void closeTopLayer(dialog);
    }
  }, [open]);

  function handleLocate() {
    if (!navigator.geolocation) return;
    navigator.geolocation.getCurrentPosition((pos) => {
      setLatitude(pos.coords.latitude.toFixed(6));
      setLongitude(pos.coords.longitude.toFixed(6));
    });
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!effectiveSegmentId) return;
    const lat = Number(latitude);
    const lon = Number(longitude);
    if (Number.isNaN(lat) || Number.isNaN(lon)) return;

    setSubmitting(true);
    try {
      let uploadedPhotoUri: string | null = null;
      if (isOnline && authToken && photoBlob) {
        try {
          const uploaded = await postUploadEvidence(photoBlob, authToken);
          uploadedPhotoUri = uploaded.photo_uri;
        } catch {
          uploadedPhotoUri = null;
        }
      }

      const queued = await enqueueFieldReport({
        segment_id: effectiveSegmentId,
        proposed_passability: passability,
        latitude: lat,
        longitude: lon,
      });

      if (isOnline && authToken) {
        await postFieldReport(
          {
            client_uuid: queued.client_uuid,
            segment_id: queued.segment_id,
            proposed_passability: queued.proposed_passability,
            latitude: queued.latitude,
            longitude: queued.longitude,
            captured_at_client: queued.captured_at_client,
            photo_uri: uploadedPhotoUri,
            channel: "MOBILE_APP",
          },
          authToken
        );
        onQueuedOrSent(
          uploadedPhotoUri
            ? "Reporte con evidencia fotográfica enviado al sistema vial."
            : "Reporte enviado al sistema de consenso vial."
        );
      } else {
        onQueuedOrSent("Reporte guardado localmente para envío automático al recuperar red.");
      }
      setPhotoBlob(null);
      if (photoPreviewUrl) URL.revokeObjectURL(photoPreviewUrl);
      setPhotoPreviewUrl(null);
      if (dialogRef.current) {
        await closeTopLayer(dialogRef.current);
      }
    } catch {
      onQueuedOrSent("El reporte quedó protegido en cola offline para reintento.");
      if (dialogRef.current) {
        await closeTopLayer(dialogRef.current);
      }
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <dialog
      ref={dialogRef}
      className="modal-dialog"
      aria-labelledby={titleId}
      {...({ closedby: "any" } as Record<string, string>)}
    >
      <div className="modal-content">
        <div className="modal-header">
          <div>
            <h2 id={titleId}>Reportar estado de vía</h2>
            <p className="panel-subtitle">Ayuda a otros conductores en tiempo real</p>
          </div>
          <button
            type="button"
            className="icon-btn"
            aria-label="Cerrar diálogo de reporte"
            onClick={() => dialogRef.current && void closeTopLayer(dialogRef.current)}
          >
            <IconX size={18} />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="clean-form" aria-labelledby={titleId}>
          <div className="field">
            <label htmlFor="report-segment">Tramo observado</label>
            <select
              id="report-segment"
              name="segment_id"
              className="brand-select"
              required
              autoFocus
              value={effectiveSegmentId}
              onChange={(e) => setSegmentId(e.target.value)}
              data-error="report-segment-error"
            >
              {features.length === 0 ? (
                <option value="">Sin tramos disponibles</option>
              ) : (
                features.map((f) => (
                  <option key={f.properties.segment_id} value={f.properties.segment_id}>
                    {f.properties.corridor_name} ({f.properties.length_km.toFixed(1)} km)
                  </option>
                ))
              )}
            </select>
            <span id="report-segment-error" className="error-msg">
              Seleccione un tramo.
            </span>
          </div>

          <div className="group-block">
            <span className="section-eyebrow">¿Cómo está el paso?</span>
            <div className="choice-grid" role="radiogroup" aria-label="Transitabilidad observada">
              <label className="choice-card">
                <input
                  type="radio"
                  className="visually-hidden"
                  name="proposed_passability"
                  value="BLOCKED"
                  checked={passability === "BLOCKED"}
                  onChange={() => setPassability("BLOCKED")}
                />
                <span className="status-dot status-dot--red" aria-hidden="true" />
                <span>Cerrada / Derrumbe</span>
              </label>
              <label className="choice-card">
                <input
                  type="radio"
                  className="visually-hidden"
                  name="proposed_passability"
                  value="PARTIAL_ONE_LANE"
                  checked={passability === "PARTIAL_ONE_LANE"}
                  onChange={() => setPassability("PARTIAL_ONE_LANE")}
                />
                <span className="status-dot status-dot--amber" aria-hidden="true" />
                <span>Paso en un carril</span>
              </label>
              <label className="choice-card">
                <input
                  type="radio"
                  className="visually-hidden"
                  name="proposed_passability"
                  value="OPEN"
                  checked={passability === "OPEN"}
                  onChange={() => setPassability("OPEN")}
                />
                <span className="status-dot status-dot--green" aria-hidden="true" />
                <span>Vía libre</span>
              </label>
            </div>
          </div>

          <div className="two-col-inline">
            <div className="field field--tight">
              <label htmlFor="report-lat">Latitud</label>
              <input
                id="report-lat"
                name="latitude"
                type="text"
                inputMode="decimal"
                required
                pattern="^-?([1-8]?\d(\.\d+)?|90(\.0+)?)$"
                value={latitude}
                onChange={(e) => setLatitude(e.target.value)}
                data-error={latErrorId}
              />
              <span id={latErrorId} className="error-msg">
                Latitud inválida.
              </span>
            </div>

            <div className="field field--tight">
              <label htmlFor="report-lon">Longitud</label>
              <input
                id="report-lon"
                name="longitude"
                type="text"
                inputMode="decimal"
                required
                pattern="^-?(1[0-7]\d(\.\d+)?|[1-9]?\d(\.\d+)?|180(\.0+)?)$"
                value={longitude}
                onChange={(e) => setLongitude(e.target.value)}
                data-error={lonErrorId}
              />
              <span id={lonErrorId} className="error-msg">
                Longitud inválida.
              </span>
            </div>
          </div>

          <div className="evidence-upload-row">
            <label className="btn btn--sm evidence-file-label">
              <IconCamera size={15} />
              <span>{photoBlob ? "Cambiar foto" : "Adjuntar foto (opcional)"}</span>
              <input
                type="file"
                className="visually-hidden"
                accept="image/jpeg,image/png,image/webp"
                capture="environment"
                onChange={(e) => void handlePhotoSelected(e)}
              />
            </label>
            {photoPreviewUrl && (
              <div className="evidence-thumb-wrap">
                <img src={photoPreviewUrl} alt="Vista previa de evidencia vial" className="evidence-thumb" />
                <button
                  type="button"
                  className="icon-btn icon-btn--sm"
                  aria-label="Quitar fotografía"
                  onClick={() => {
                    if (photoPreviewUrl) URL.revokeObjectURL(photoPreviewUrl);
                    setPhotoBlob(null);
                    setPhotoPreviewUrl(null);
                  }}
                >
                  <IconX size={14} />
                </button>
              </div>
            )}
          </div>

          <div className="modal-actions">
            <button type="button" className="btn" onClick={handleLocate}>
              <IconCrosshair size={16} />
              <span>Usar GPS actual</span>
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => dialogRef.current && void closeTopLayer(dialogRef.current)}
            >
              Cancelar
            </button>
            <button
              type="submit"
              className="btn btn--primary"
              disabled={submitting || !effectiveSegmentId}
            >
              {submitting ? "Guardando..." : isOnline ? "Enviar reporte" : "Guardar offline"}
            </button>
          </div>
        </form>
      </div>
    </dialog>
  );
}
