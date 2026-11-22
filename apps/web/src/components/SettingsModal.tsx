import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import {
  deleteB2bApiKey,
  deleteB2bWebhook,
  deleteSubscription,
  fetchAdminUsers,
  fetchAnalyticsSummary,
  fetchB2bApiKeys,
  fetchB2bWebhooks,
  fetchSubscriptions,
  postAdminUser,
  postB2bApiKey,
  postB2bWebhook,
  postSubscription,
  type AdminUserItem,
  type AlertSubscriptionItem,
  type AnalyticsSummaryResponse,
  type B2bApiKeyItem,
  type B2bWebhookItem,
  type CorridorSegmentFeature,
  type CreateApiKeyResponse,
  type CreateWebhookResponse,
  type RoleCode,
  type TerritorialUnit,
} from "../api/client.ts";
import type { EphemeralTelemetryRecord, SyncMetaRecord } from "../offline/db.ts";
import type { ThemePreference } from "../utils/theme.ts";
import { attachDialogLightDismissFallback, closeTopLayer } from "../utils/topLayer.ts";
import {
  IconBell,
  IconCheck,
  IconCopy,
  IconDownload,
  IconFileText,
  IconKey,
  IconMoon,
  IconSun,
  IconTrash,
  IconTv,
  IconUser,
  IconX,
} from "./Icons.tsx";

export interface SettingsModalProps {
  open: boolean;
  onClose: () => void;
  themePreference: ThemePreference;
  systemScheme: "light" | "dark";
  onChangeTheme: (pref: ThemePreference) => void;
  syncMeta: SyncMetaRecord;
  queuedCount: number;
  ephemeralRecord: EphemeralTelemetryRecord | null;
  authToken?: string | null;
  userRole?: RoleCode | null;
  isOnline?: boolean;
  regions?: TerritorialUnit[];
  selectedRegionCode?: string;
  features?: CorridorSegmentFeature[];
  kioskMode?: boolean;
  onToggleKioskMode?: (next: boolean) => void;
  onToast?: (message: string) => void;
}

function downloadTextFile(filename: string, mimeType: string, content: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
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
  authToken = null,
  userRole = null,
  isOnline = true,
  regions = [],
  selectedRegionCode = "PE-HUC",
  features = [],
  kioskMode = false,
  onToggleKioskMode,
  onToast,
}: SettingsModalProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const titleId = useId();

  const [analytics, setAnalytics] = useState<AnalyticsSummaryResponse | null>(null);
  const [subscriptions, setSubscriptions] = useState<AlertSubscriptionItem[]>([]);
  const [cachedSlugs, setCachedSlugs] = useState<Record<string, boolean>>({});
  const [downloadingSlug, setDownloadingSlug] = useState<string | null>(null);

  const [subLabel, setSubLabel] = useState("");
  const [subCorridor, setSubCorridor] = useState("");
  const [subSeverity, setSubSeverity] = useState<"AMBER" | "RED" | "BLOCKED">("RED");
  const [subChannel, setSubChannel] = useState<"WHATSAPP_BOT" | "SMS">("WHATSAPP_BOT");
  const [subBusy, setSubBusy] = useState(false);

  const [adminUsers, setAdminUsers] = useState<AdminUserItem[]>([]);
  const [newUserPhone, setNewUserPhone] = useState("+51962");
  const [newUserName, setNewUserName] = useState("");
  const [newUserPass, setNewUserPass] = useState("");
  const [newUserRole, setNewUserRole] = useState<RoleCode>("CIVIL_DEFENSE");
  const [userBusy, setUserBusy] = useState(false);

  const [apiKeys, setApiKeys] = useState<B2bApiKeyItem[]>([]);
  const [issuedKey, setIssuedKey] = useState<CreateApiKeyResponse | null>(null);
  const [b2bKeyOrg, setB2bKeyOrg] = useState("");
  const [b2bKeyRole, setB2bKeyRole] = useState<RoleCode>("DRIVER");
  const [b2bKeyBusy, setB2bKeyBusy] = useState(false);

  const [webhooks, setWebhooks] = useState<B2bWebhookItem[]>([]);
  const [issuedWebhook, setIssuedWebhook] = useState<CreateWebhookResponse | null>(null);
  const [whOrg, setWhOrg] = useState("");
  const [whUrl, setWhUrl] = useState("");
  const [whRisk, setWhRisk] = useState<"AMBER" | "RED" | "BLOCKED">("RED");
  const [whBusy, setWhBusy] = useState(false);

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

  const deptRegions = regions.filter((r) => r.admin_level === 4);

  useEffect(() => {
    if (!open || !isOnline) return;
    let cancelled = false;

    void fetchAnalyticsSummary(selectedRegionCode)
      .then((res) => {
        if (!cancelled) setAnalytics(res);
      })
      .catch(() => {});

    if (authToken) {
      void fetchSubscriptions(authToken)
        .then((res) => {
          if (!cancelled) setSubscriptions(res.subscriptions);
        })
        .catch(() => {});
      void fetchB2bApiKeys(authToken)
        .then((res) => {
          if (!cancelled) setApiKeys(res.api_keys);
        })
        .catch(() => {});
      void fetchB2bWebhooks(authToken)
        .then((res) => {
          if (!cancelled) setWebhooks(res.webhooks);
        })
        .catch(() => {});
      if (userRole === "COER_ADMIN") {
        void fetchAdminUsers(authToken)
          .then((res) => {
            if (!cancelled) setAdminUsers(res.users);
          })
          .catch(() => {});
      }
    }

    if (typeof window !== "undefined" && "caches" in window) {
      void window.caches
        .open("paradixe-pmtiles-cache")
        .then(async (cache) => {
          const next: Record<string, boolean> = {};
          for (const d of regions.filter((r) => r.admin_level === 4)) {
            const code = (d.iso_code ?? d.ubigeo_code).toLowerCase();
            const hit = await cache.match(`/tiles/${code}.pmtiles`);
            if (hit) next[code] = true;
          }
          if (!cancelled) setCachedSlugs(next);
        })
        .catch(() => {});
    }

    return () => {
      cancelled = true;
    };
  }, [open, isOnline, authToken, userRole, selectedRegionCode, regions]);

  async function handleDownloadPack(unit: TerritorialUnit) {
    const slug = (unit.iso_code ?? unit.ubigeo_code).toLowerCase();
    const url = `/tiles/${slug}.pmtiles`;
    setDownloadingSlug(slug);
    try {
      const resp = await fetch(url);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      if (typeof window !== "undefined" && "caches" in window) {
        const cache = await window.caches.open("paradixe-pmtiles-cache");
        await cache.put(url, resp);
      } else {
        await resp.arrayBuffer();
      }
      setCachedSlugs((prev) => ({ ...prev, [slug]: true }));
      onToast?.(`Mapa vectorial ${unit.name} listo para uso sin conexión.`);
    } catch {
      onToast?.(`No se pudo descargar el paquete offline de ${unit.name}.`);
    } finally {
      setDownloadingSlug(null);
    }
  }

  async function handleCreateSub(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !subLabel.trim()) return;
    setSubBusy(true);
    try {
      const created = await postSubscription(
        {
          label: subLabel.trim(),
          region_iso: selectedRegionCode,
          corridor_name: subCorridor.trim() || null,
          min_risk_level: subSeverity,
          channel: subChannel,
        },
        authToken
      );
      setSubscriptions((prev) => [created, ...prev]);
      setSubLabel("");
      setSubCorridor("");
      onToast?.("Alerta de flota activada.");
    } catch {
      onToast?.("No fue posible guardar la suscripción.");
    } finally {
      setSubBusy(false);
    }
  }

  async function handleDeleteSub(id: string) {
    if (!authToken) return;
    try {
      await deleteSubscription(id, authToken);
      setSubscriptions((prev) => prev.filter((s) => s.subscription_id !== id));
    } catch {
      onToast?.("No fue posible eliminar la alerta.");
    }
  }

  async function handleCreateAdminUser(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !newUserName.trim() || !newUserPhone.trim()) return;
    setUserBusy(true);
    try {
      const created = await postAdminUser(
        {
          phone_e164: newUserPhone.trim(),
          full_name: newUserName.trim(),
          password: newUserPass,
          role_code: newUserRole,
        },
        authToken
      );
      setAdminUsers((prev) => [created, ...prev]);
      setNewUserName("");
      setNewUserPass("");
      onToast?.(`Operador ${created.full_name} creado.`);
    } catch {
      onToast?.("No se pudo crear la cuenta COER.");
    } finally {
      setUserBusy(false);
    }
  }

  async function handleCreateB2bKey(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !b2bKeyOrg.trim()) return;
    setB2bKeyBusy(true);
    try {
      const created = await postB2bApiKey(
        {
          organization_name: b2bKeyOrg.trim(),
          role_code: b2bKeyRole,
          rate_limit_rpm: 120,
        },
        authToken
      );
      setIssuedKey(created);
      setApiKeys((prev) => [
        {
          api_key_id: created.api_key_id,
          organization_name: created.organization_name,
          key_prefix: created.key_prefix,
          role_code: created.role_code,
          rate_limit_rpm: created.rate_limit_rpm,
          is_active: true,
          last_used_at: null,
          created_at: created.created_at,
          revoked_at: null,
        },
        ...prev,
      ]);
      setB2bKeyOrg("");
      onToast?.("API Key B2B emitida.");
    } catch {
      onToast?.("No fue posible emitir la API Key.");
    } finally {
      setB2bKeyBusy(false);
    }
  }

  async function handleRevokeB2bKey(apiKeyId: string) {
    if (!authToken) return;
    try {
      await deleteB2bApiKey(apiKeyId, authToken);
      setApiKeys((prev) =>
        prev.map((k) => (k.api_key_id === apiKeyId ? { ...k, is_active: false } : k))
      );
      onToast?.("API Key revocada.");
    } catch {
      onToast?.("No fue posible revocar la API Key.");
    }
  }

  async function handleCreateB2bWebhook(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!authToken || !whOrg.trim() || !whUrl.trim()) return;
    setWhBusy(true);
    try {
      const created = await postB2bWebhook(
        {
          organization_name: whOrg.trim(),
          target_url: whUrl.trim(),
          region_iso: selectedRegionCode,
          min_risk_level: whRisk,
        },
        authToken
      );
      setIssuedWebhook(created);
      setWebhooks((prev) => [
        {
          webhook_id: created.webhook_id,
          organization_name: created.organization_name,
          target_url: created.target_url,
          masked_secret: `whsec_****${created.signing_secret.slice(-4)}`,
          region_iso: created.region_iso,
          min_risk_level: created.min_risk_level,
          is_active: true,
          last_delivery_status: null,
          last_delivered_at: null,
          created_at: created.created_at,
        },
        ...prev,
      ]);
      setWhOrg("");
      setWhUrl("");
      onToast?.("Webhook B2B registrado.");
    } catch {
      onToast?.("URL inválida o bloqueada por seguridad (use HTTPS público).");
    } finally {
      setWhBusy(false);
    }
  }

  async function handleDeleteB2bWebhook(webhookId: string) {
    if (!authToken) return;
    try {
      await deleteB2bWebhook(webhookId, authToken);
      setWebhooks((prev) => prev.filter((w) => w.webhook_id !== webhookId));
      onToast?.("Webhook eliminado.");
    } catch {
      onToast?.("No se pudo eliminar el webhook.");
    }
  }

  function handleExportCsv() {
    const header = "segment_id,corridor_name,surface_type,passability_state,ivv_score,risk_level,length_km,verified";
    const lines = features.map((f) => {
      const p = f.properties;
      const safeName = `"${p.corridor_name.replace(/"/g, '""')}"`;
      return `${p.segment_id},${safeName},${p.surface_type},${p.passability_state},${p.ivv_score ?? ""},${p.is_stale ? "UNKNOWN" : p.risk_level},${p.length_km},${p.is_attributes_verified}`;
    });
    downloadTextFile(
      `paradixe-${selectedRegionCode.toLowerCase()}-reporte.csv`,
      "text/csv;charset=utf-8",
      [header, ...lines].join("\n")
    );
    onToast?.("Reporte CSV exportado.");
  }

  function handleExportGeoJson() {
    downloadTextFile(
      `paradixe-${selectedRegionCode.toLowerCase()}-corredores.geojson`,
      "application/geo+json;charset=utf-8",
      JSON.stringify({ type: "FeatureCollection", features }, null, 2)
    );
    onToast?.("GeoJSON exportado.");
  }

  const systemLabel = systemScheme === "dark" ? "Oscuro" : "Claro";

  return (
    <dialog
      ref={dialogRef}
      className="modal-dialog modal-dialog--wide"
      aria-labelledby={titleId}
      {...({ closedby: "any" } as Record<string, string>)}
    >
      <div className="modal-content">
        <div className="modal-header">
          <div>
            <h2 id={titleId}>Centro de Control y Ajustes</h2>
            <p className="panel-subtitle">Apariencia, paquetes offline, alertas B2B y reportes</p>
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

        <div className="group-block" style={{ marginBlockEnd: "1rem" }}>
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

        {deptRegions.length > 0 && (
          <div className="group-block" style={{ marginBlockEnd: "1rem" }}>
            <span className="section-eyebrow">Mapas Offline por Departamento (PMTiles v3)</span>
            <div className="offline-pack-list">
              {deptRegions.map((unit) => {
                const slug = (unit.iso_code ?? unit.ubigeo_code).toLowerCase();
                const isCached = Boolean(cachedSlugs[slug]);
                const busy = downloadingSlug === slug;
                return (
                  <div key={unit.unit_id} className="offline-pack-row">
                    <div>
                      <strong>{unit.name}</strong>
                      <small>{unit.segment_count.toLocaleString("es-PE")} tramos viales</small>
                    </div>
                    <button
                      type="button"
                      className={isCached ? "btn btn--sm" : "btn btn--primary btn--sm"}
                      disabled={!isOnline || busy}
                      onClick={() => void handleDownloadPack(unit)}
                    >
                      {isCached ? <IconCheck size={14} /> : <IconDownload size={14} />}
                      <span>{busy ? "Bajando..." : isCached ? "En caché" : "Descargar"}</span>
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        <div className="group-block" style={{ marginBlockEnd: "1rem" }}>
          <span className="section-eyebrow">Reporte Ejecutivo y Sala Situacional ({selectedRegionCode})</span>
          {analytics && typeof analytics.total_segments === "number" && (
            <div className="kpi-mini-grid" style={{ marginBlockEnd: "0.65rem" }}>
              <div className="stat-mini">
                <strong>{analytics.total_segments.toLocaleString("es-PE")}</strong>
                <span>Tramos</span>
              </div>
              <div className="stat-mini">
                <strong>{analytics.certified_pct}%</strong>
                <span>Certificado</span>
              </div>
              <div className="stat-mini">
                <strong>{analytics.active_incidents_total}</strong>
                <span>Alertas</span>
              </div>
              <div className="stat-mini">
                <strong>{analytics.hospitals_count}</strong>
                <span>Hospitales</span>
              </div>
            </div>
          )}
          <div className="ops-actions-row">
            <button type="button" className="btn btn--sm" onClick={handleExportCsv}>
              <IconFileText size={14} />
              <span>Exportar CSV</span>
            </button>
            <button type="button" className="btn btn--sm" onClick={handleExportGeoJson}>
              <IconDownload size={14} />
              <span>GeoJSON</span>
            </button>
            <button
              type="button"
              className="btn btn--sm"
              onClick={() => window.print?.()}
            >
              <span>Imprimir / PDF</span>
            </button>
            {onToggleKioskMode && (
              <button
                type="button"
                className="btn btn--primary btn--sm"
                onClick={() => {
                  onToggleKioskMode(!kioskMode);
                  if (dialogRef.current) void closeTopLayer(dialogRef.current);
                }}
              >
                <IconTv size={14} />
                <span>{kioskMode ? "Salir Sala COER" : "Pantalla COER"}</span>
              </button>
            )}
          </div>
        </div>

        <details className="disclosure-card" style={{ marginBlockEnd: "0.75rem" }}>
          <summary>
            <span style={{ display: "inline-flex", alignItems: "center", gap: "0.35rem" }}>
              <IconBell size={14} />
              <span>Alertas automáticas de flota ({subscriptions.length})</span>
            </span>
          </summary>
          {!authToken ? (
            <p className="hint" style={{ marginBlock: "0.5rem 0" }}>
              Inicia sesión para suscribir tu WhatsApp o SMS a alertas por corredor.
            </p>
          ) : (
            <div style={{ marginBlockStart: "0.65rem" }}>
              <form onSubmit={handleCreateSub} className="clean-form" style={{ marginBlockEnd: "0.65rem" }}>
                <div className="two-col-inline">
                  <div className="field field--tight">
                    <label htmlFor="sub-label">Nombre de ruta</label>
                    <input
                      id="sub-label"
                      type="text"
                      required
                      placeholder="Ej. Flota Tingo - Lima"
                      value={subLabel}
                      onChange={(e) => setSubLabel(e.target.value)}
                    />
                  </div>
                  <div className="field field--tight">
                    <label htmlFor="sub-corridor">Corredor (opcional)</label>
                    <input
                      id="sub-corridor"
                      type="text"
                      placeholder="Ej. Carretera Central"
                      value={subCorridor}
                      onChange={(e) => setSubCorridor(e.target.value)}
                    />
                  </div>
                </div>
                <div className="two-col-inline">
                  <div className="field field--tight">
                    <label htmlFor="sub-sev">Umbral alerta</label>
                    <select
                      id="sub-sev"
                      className="brand-select"
                      value={subSeverity}
                      onChange={(e) => setSubSeverity(e.target.value as "AMBER" | "RED" | "BLOCKED")}
                    >
                      <option value="AMBER">Desde Precaución</option>
                      <option value="RED">Solo Crítico (RED)</option>
                      <option value="BLOCKED">Solo Bloqueos</option>
                    </select>
                  </div>
                  <div className="field field--tight">
                    <label htmlFor="sub-chan">Canal</label>
                    <select
                      id="sub-chan"
                      className="brand-select"
                      value={subChannel}
                      onChange={(e) => setSubChannel(e.target.value as "WHATSAPP_BOT" | "SMS")}
                    >
                      <option value="WHATSAPP_BOT">WhatsApp</option>
                      <option value="SMS">SMS</option>
                    </select>
                  </div>
                </div>
                <button type="submit" className="btn btn--primary btn--sm" disabled={subBusy || !isOnline}>
                  Guardar alerta de flota
                </button>
              </form>

              {subscriptions.length > 0 && (
                <ul className="ops-card-list">
                  {subscriptions.map((s) => (
                    <li key={s.subscription_id} className="ops-list-item" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <div>
                        <strong>{s.label}</strong>
                        <div className="hint" style={{ margin: 0 }}>
                          {s.corridor_name ?? `Todo ${s.region_iso ?? ""}`} · {s.min_risk_level} · {s.channel}
                        </div>
                      </div>
                      <button
                        type="button"
                        className="icon-btn icon-btn--sm"
                        aria-label={`Eliminar alerta ${s.label}`}
                        onClick={() => void handleDeleteSub(s.subscription_id)}
                      >
                        <IconTrash size={14} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </details>

        {authToken && userRole === "COER_ADMIN" && (
          <details className="disclosure-card" style={{ marginBlockEnd: "0.75rem" }}>
            <summary>
              <span style={{ display: "inline-flex", alignItems: "center", gap: "0.35rem" }}>
                <IconUser size={14} />
                <span>Equipo COER y Operadores ({adminUsers.length})</span>
              </span>
            </summary>
            <div style={{ marginBlockStart: "0.65rem" }}>
              <form
                onSubmit={handleCreateAdminUser}
                className="clean-form"
                style={{ marginBlockEnd: "0.65rem" }}
              >
                <div className="two-col-inline">
                  <div className="field field--tight">
                    <label htmlFor="admin-user-phone">Teléfono E.164</label>
                    <input
                      id="admin-user-phone"
                      type="tel"
                      required
                      value={newUserPhone}
                      onChange={(e) => setNewUserPhone(e.target.value)}
                    />
                  </div>
                  <div className="field field--tight">
                    <label htmlFor="admin-user-name">Nombre completo</label>
                    <input
                      id="admin-user-name"
                      type="text"
                      required
                      placeholder="Ej. Def. Civil Ambo"
                      value={newUserName}
                      onChange={(e) => setNewUserName(e.target.value)}
                    />
                  </div>
                </div>
                <div className="two-col-inline">
                  <div className="field field--tight">
                    <label htmlFor="admin-user-role">Rol operativo</label>
                    <select
                      id="admin-user-role"
                      className="brand-select"
                      value={newUserRole}
                      onChange={(e) => setNewUserRole(e.target.value as RoleCode)}
                    >
                      <option value="CIVIL_DEFENSE">CIVIL_DEFENSE</option>
                      <option value="HEALTH_OPERATOR">HEALTH_OPERATOR</option>
                      <option value="DRIVER">DRIVER</option>
                      <option value="COER_ADMIN">COER_ADMIN</option>
                    </select>
                  </div>
                  <div className="field field--tight">
                    <label htmlFor="admin-user-pass">Contraseña (min 10)</label>
                    <input
                      id="admin-user-pass"
                      type="password"
                      required
                      minLength={10}
                      value={newUserPass}
                      onChange={(e) => setNewUserPass(e.target.value)}
                    />
                  </div>
                </div>
                <button
                  type="submit"
                  className="btn btn--primary btn--sm"
                  disabled={userBusy || !isOnline}
                >
                  Crear operador
                </button>
              </form>

              {adminUsers.length > 0 && (
                <ul className="ops-card-list">
                  {adminUsers.slice(0, 8).map((u) => (
                    <li key={u.user_id} className="ops-list-item">
                      <div className="ops-item-top">
                        <strong>{u.full_name}</strong>
                        <span className="risk-badge risk-badge--GREEN">{u.role_code}</span>
                      </div>
                      <div className="hint" style={{ margin: 0 }}>
                        {u.phone_e164} · Rep {u.reputation_score.toFixed(2)}
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </details>
        )}

        {authToken && (
          <details className="disclosure-card" style={{ marginBlockEnd: "0.75rem" }}>
            <summary>
              <span style={{ display: "inline-flex", alignItems: "center", gap: "0.35rem" }}>
                <IconKey size={14} />
                <span>Integración B2B (API Keys y Webhooks)</span>
              </span>
            </summary>
            <div style={{ marginBlockStart: "0.65rem" }}>
              {userRole === "COER_ADMIN" && (
                <form
                  onSubmit={handleCreateB2bKey}
                  className="clean-form"
                  style={{ marginBlockEnd: "0.75rem" }}
                >
                  <div className="two-col-inline">
                    <div className="field field--tight">
                      <label htmlFor="b2b-key-org">Empresa / Flota</label>
                      <input
                        id="b2b-key-org"
                        type="text"
                        required
                        placeholder="Ej. AgroExport Centro SAC"
                        value={b2bKeyOrg}
                        onChange={(e) => setB2bKeyOrg(e.target.value)}
                      />
                    </div>
                    <div className="field field--tight">
                      <label htmlFor="b2b-key-role">Alcance API</label>
                      <select
                        id="b2b-key-role"
                        className="brand-select"
                        value={b2bKeyRole}
                        onChange={(e) => setB2bKeyRole(e.target.value as RoleCode)}
                      >
                        <option value="DRIVER">Flota / Logística (DRIVER)</option>
                        <option value="HEALTH_OPERATOR">Red de Salud (HEALTH_OPERATOR)</option>
                        <option value="CIVIL_DEFENSE">Defensa Civil (CIVIL_DEFENSE)</option>
                      </select>
                    </div>
                  </div>
                  <button
                    type="submit"
                    className="btn btn--primary btn--sm"
                    disabled={b2bKeyBusy || !isOnline}
                  >
                    Emitir API Key B2B
                  </button>
                </form>
              )}

              {issuedKey && (
                <div className="key-secret-callout" role="status">
                  <div>
                    <strong>Copia tu API Key (se muestra una sola vez):</strong>
                    <code>{issuedKey.raw_api_key}</code>
                  </div>
                  <button
                    type="button"
                    className="btn btn--sm"
                    onClick={() => {
                      void navigator.clipboard?.writeText(issuedKey.raw_api_key);
                      onToast?.("API Key copiada.");
                    }}
                  >
                    <IconCopy size={14} />
                    <span>Copiar</span>
                  </button>
                </div>
              )}

              {apiKeys.length > 0 && (
                <ul className="ops-card-list" style={{ marginBlockEnd: "0.75rem" }}>
                  {apiKeys.map((k) => (
                    <li
                      key={k.api_key_id}
                      className="ops-list-item"
                      style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}
                    >
                      <div>
                        <strong>{k.organization_name}</strong>
                        <div className="hint" style={{ margin: 0 }}>
                          <code>{k.key_prefix}...</code> · {k.role_code} ·{" "}
                          {k.is_active ? `${k.rate_limit_rpm} rpm` : "Revocada"}
                        </div>
                      </div>
                      {k.is_active && (
                        <button
                          type="button"
                          className="icon-btn icon-btn--sm"
                          aria-label={`Revocar llave ${k.organization_name}`}
                          onClick={() => void handleRevokeB2bKey(k.api_key_id)}
                        >
                          <IconTrash size={14} />
                        </button>
                      )}
                    </li>
                  ))}
                </ul>
              )}

              <form onSubmit={handleCreateB2bWebhook} className="clean-form">
                <div className="two-col-inline">
                  <div className="field field--tight">
                    <label htmlFor="wh-org">Sistema ERP / TMS</label>
                    <input
                      id="wh-org"
                      type="text"
                      required
                      placeholder="Ej. TMS Ruta Norte"
                      value={whOrg}
                      onChange={(e) => setWhOrg(e.target.value)}
                    />
                  </div>
                  <div className="field field--tight">
                    <label htmlFor="wh-risk">Umbral alerta</label>
                    <select
                      id="wh-risk"
                      className="brand-select"
                      value={whRisk}
                      onChange={(e) => setWhRisk(e.target.value as "AMBER" | "RED" | "BLOCKED")}
                    >
                      <option value="AMBER">AMBER+</option>
                      <option value="RED">RED+</option>
                      <option value="BLOCKED">BLOCKED</option>
                    </select>
                  </div>
                </div>
                <div className="field field--tight">
                  <label htmlFor="wh-url">Endpoint HTTPS firmado (HMAC)</label>
                  <input
                    id="wh-url"
                    type="url"
                    required
                    placeholder="https://erp.miempresa.pe/webhooks/paradixe"
                    value={whUrl}
                    onChange={(e) => setWhUrl(e.target.value)}
                  />
                </div>
                <button type="submit" className="btn btn--sm" disabled={whBusy || !isOnline}>
                  Registrar Webhook B2B
                </button>
              </form>

              {issuedWebhook && (
                <div className="key-secret-callout" role="status" style={{ marginBlockStart: "0.5rem" }}>
                  <div>
                    <strong>Secreto HMAC del Webhook:</strong>
                    <code>{issuedWebhook.signing_secret}</code>
                  </div>
                </div>
              )}

              {webhooks.length > 0 && (
                <ul className="ops-card-list" style={{ marginBlockStart: "0.6rem" }}>
                  {webhooks.map((w) => (
                    <li
                      key={w.webhook_id}
                      className="ops-list-item"
                      style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}
                    >
                      <div>
                        <strong>{w.organization_name}</strong>
                        <div className="hint" style={{ margin: 0 }}>
                          {w.target_url} · {w.min_risk_level} · {w.masked_secret}
                        </div>
                      </div>
                      <button
                        type="button"
                        className="icon-btn icon-btn--sm"
                        aria-label={`Eliminar webhook ${w.organization_name}`}
                        onClick={() => void handleDeleteB2bWebhook(w.webhook_id)}
                      >
                        <IconTrash size={14} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </details>
        )}

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
