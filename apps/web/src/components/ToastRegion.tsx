import { useEffect, useRef } from "react";

export interface ToastMessage {
  id: string;
  text: string;
}

export interface ToastRegionProps {
  toasts: ToastMessage[];
  modalOpen: boolean;
  onDismiss: (id: string) => void;
}

function ToastPopover({
  toast,
  onDismiss,
}: {
  toast: ToastMessage;
  onDismiss: (id: string) => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const el = ref.current as (HTMLDivElement & { showPopover?: () => void }) | null;
    if (!el) return;
    try {
      if (typeof el.showPopover === "function" && !el.matches(":popover-open")) {
        el.showPopover();
      }
    } catch {
      // Ignorar en entornos de test sin top layer nativo
    }
  }, []);

  return (
    <div
      ref={ref}
      id={`toast-${toast.id}`}
      className="toast-item"
      {...({ popover: "manual" } as Record<string, string>)}
    >
      <span>{toast.text}</span>
      <button
        type="button"
        onClick={() => onDismiss(toast.id)}
        {...({
          popoverTarget: `toast-${toast.id}`,
          popoverTargetAction: "hide",
        } as Record<string, string>)}
      >
        Cerrar
      </button>
    </div>
  );
}

export function ToastRegion({ toasts, modalOpen, onDismiss }: ToastRegionProps) {
  const latestText = toasts.length > 0 ? toasts[toasts.length - 1].text : "";

  useEffect(() => {
    if (!latestText || modalOpen) return;
    const doc = document as Document & {
      ariaNotify?: (message: string, options?: { priority?: string }) => void;
    };
    if (typeof doc.ariaNotify === "function") {
      doc.ariaNotify(latestText, { priority: "normal" });
    }
  }, [latestText, modalOpen]);

  return (
    <>
      <div className="visually-hidden" aria-live="polite">
        {modalOpen ? "" : latestText}
      </div>
      {!modalOpen && (
        <div role="region" aria-label="Notificaciones del sistema" className="toast-region">
          {toasts.map((t) => (
            <ToastPopover key={t.id} toast={t} onDismiss={onDismiss} />
          ))}
        </div>
      )}
    </>
  );
}
