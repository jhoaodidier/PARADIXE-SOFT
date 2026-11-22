import { useEffect, useId, useRef } from "react";
import type { EphemeralTelemetryRecord, SyncMetaRecord } from "../offline/db.ts";
import type { ThemePreference } from "../utils/theme.ts";
import { attachDialogLightDismissFallback, closeTopLayer } from "../utils/topLayer.ts";
import { IconMoon, IconSun, IconX } from "./Icons.tsx";

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
          <div>
            <h2 id={titleId}>Ajustes</h2>
            <p className="panel-subtitle">Apariencia y sincronización</p>
          </div>
          <button
            type="button"
            className="icon-btn"
            autoFocus
            aria-label="Cerrar diálogo de configuración"
            onClick={() => dialogRef.current && void closeTopLayer(dialogRef.current)}
          >
            <IconX size={18} />
          </button>
        </div>

        <div className="group-block" style={{ marginBlockEnd: "1.25rem" }}>
          <span className="section-eyebrow">Tema visual</span>
          <div className="segmented-pills" role="radiogroup" aria-label="Esquema de color de la interfaz">
            <label className="segmented-pill">
              <input
                type="radio"
                className="visually-hidden"
                name="theme_pref"
                value="light"
                checked={themePreference === "light"}
                onChange={() => onChangeTheme("light")}
              />
              <IconSun size={15} />
              <span>Claro</span>
            </label>
            <label className="segmented-pill">
              <input
                type="radio"
                className="visually-hidden"
                name="theme_pref"
                value="dark"
                checked={themePreference === "dark"}
                onChange={() => onChangeTheme("dark")}
              />
              <IconMoon size={15} />
              <span>Oscuro</span>
            </label>
            <label className="segmented-pill">
              <input
                type="radio"
                className="visually-hidden"
                name="theme_pref"
                value="system"
                checked={themePreference === "system"}
                onChange={() => onChangeTheme("system")}
              />
              <span>Sistema ({systemLabel})</span>
            </label>
          </div>
        </div>

        <details className="disclosure-card">
          <summary>Estado offline y privacidad GPS</summary>
          <dl className="settings-dl">
            <dt>Cursor de sincronización</dt>
            <dd>#{syncMeta.last_seq}</dd>
            <dt>Última actualización</dt>
            <dd>
              {syncMeta.last_synced_at
                ? new Date(syncMeta.last_synced_at).toLocaleString("es-PE")
                : "Sin sincronizar"}
            </dd>
            <dt>Reportes en cola offline</dt>
            <dd>{queuedCount}</dd>
            <dt>Rotación anónima diaria GPS</dt>
            <dd>
              {ephemeralRecord
                ? `${ephemeralRecord.ephemeral_id_date_utc} (Activo)`
                : "Inicializando"}
            </dd>
          </dl>
        </details>
      </div>
    </dialog>
  );
}
