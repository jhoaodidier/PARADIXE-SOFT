export type ThemePreference = "system" | "light" | "dark";

const STORAGE_KEY = "color-scheme";

export function readStoredThemePreference(): ThemePreference {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    // Ignorar error si storage esta bloqueado
  }
  return "light";
}

export function getSystemResolvedScheme(): "light" | "dark" {
  if (typeof window !== "undefined" && window.matchMedia) {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return "light";
}

export function applyThemePreference(pref: ThemePreference): void {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="color-scheme"]');
  const metaContent = pref === "system" ? "light dark" : pref;
  if (meta) {
    meta.content = metaContent;
  }
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    // Seguir aplicando visualmente aunque persistencia falle
  }
}

export function subscribeThemeChanges(
  onUpdate: (pref: ThemePreference, systemScheme: "light" | "dark") => void
): () => void {
  const onStorage = (event: StorageEvent) => {
    if (event.key === STORAGE_KEY || event.key === null) {
      const next = readStoredThemePreference();
      applyThemePreference(next);
      onUpdate(next, getSystemResolvedScheme());
    }
  };

  window.addEventListener("storage", onStorage);

  const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
  const onMediaChange = () => {
    onUpdate(readStoredThemePreference(), getSystemResolvedScheme());
  };
  mq?.addEventListener?.("change", onMediaChange);

  return () => {
    window.removeEventListener("storage", onStorage);
    mq?.removeEventListener?.("change", onMediaChange);
  };
}
