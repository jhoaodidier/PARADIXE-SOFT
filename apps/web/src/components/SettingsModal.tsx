import { useEffect, useId, useRef } from "react";
import type { EphemeralTelemetryRecord, SyncMetaRecord } from "../offline/db.ts";
import type { ThemePreference } from "../utils/theme.ts";
import { attachDialogLightDismissFallback, closeTopLayer } from "../utils/topLayer.ts";

export interface SettingsModalProps {
  open: boolean;
  onClose: () => void;
  themePreference: ThemePreference;
  systemScheme: "light" | "dark";
  onChangeTheme: (pref: ThemePreference) => void;
  syncMeta: SyncMetaRecord;
  queuedCount: number;
  ephemeralRecord: EphemeralTelemetryRecord | null;
}

export function SettingsModal({
  open,
  onClose,
  themePreference,
  systemScheme,
  onChangeTheme,
  syncMeta,
  queuedCount,
  ephemeralRecord,
}: SettingsModalProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const titleId = useId();

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

  const systemLabel = systemScheme === "dark" ? "Oscuro" : "Claro";

  return (
    <dialog
      ref={dialogRef}
      className="modal-dialog"
      aria-labelledby={titleId}
      {...({ closedby: "any" } as Record<string, string>)}
    >
      <div className="modal-content">
        <div className="modal-header">
          <h2 id={titleId}>Configuración y Sincronización Offline</h2>
          <button
            type="button"
            autoFocus
            aria-label="Cerrar diálogo de configuración"
            onClick={() => dialogRef.current && void closeTopLayer(dialogRef.current)}
          >
            Cerrar
          </button>
        </div>

        <fieldset>
          <legend>Esquema de color de la interfaz</legend>
          <div className="radio-group">
            <label className="radio-option">
              <input
                type="radio"
                name="theme_pref"
                value="system"
                checked={themePreference === "system"}
                onChange={() => onChangeTheme("system")}
              />
              <span>Sistema (actualmente {systemLabel})</span>
            </label>
            <label className="radio-option">
              <input
                type="radio"
                name="theme_pref"
                value="light"
                checked={themePreference === "light"}
                onChange={() => onChangeTheme("light")}
              />
              <span>Claro</span>
            </label>
            <label className="radio-option">
              <input
                type="radio"
                name="theme_pref"
                value="dark"
                checked={themePreference === "dark"}
                onChange={() => onChangeTheme("dark")}
              />
              <span>Oscuro</span>
            </label>
          </div>
        </fieldset>

        <section className="card">
          <h3>Estado de Caché y Privacidad GPS</h3>
          <dl>
            <dt>Último cursor sincronizado (sync_control)</dt>
            <dd>{syncMeta.last_seq}</dd>
            <dt>Última conectividad registrada</dt>
            <dd>
              {syncMeta.last_synced_at
                ? new Date(syncMeta.last_synced_at).toLocaleString("es-PE")
                : "Sin sincronizar"}
            </dd>
            <dt>Reportes pendientes en outbox_queue</dt>
            <dd>{queuedCount}</dd>
            <dt>Rotación seudónima GPS diaria (UTC)</dt>
            <dd>
              {ephemeralRecord
                ? `${ephemeralRecord.ephemeral_id_date_utc} (ID efímero rotativo activo)`
                : "Inicializando"}
            </dd>
          </dl>
        </section>
      </div>
    </dialog>
  );
}
