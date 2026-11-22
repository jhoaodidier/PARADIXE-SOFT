type DirtyState = { hasInteracted: boolean; hasBlurred: boolean };

function supportsUserInvalid(): boolean {
  try {
    return typeof CSS !== "undefined" && CSS.supports("selector(:user-invalid)");
  } catch {
    return false;
  }
}

function linkError(el: Element, shown: boolean): void {
  const errorId = el.getAttribute("data-error");
  if (!errorId) return;
  const ids = (el.getAttribute("aria-describedby") ?? "")
    .split(/\s+/)
    .filter((id) => id && id !== errorId);
  if (shown) ids.push(errorId);
  if (ids.length > 0) {
    el.setAttribute("aria-describedby", ids.join(" "));
  } else {
    el.removeAttribute("aria-describedby");
  }
}

function spokenText(node: HTMLElement): string {
  const copy = node.cloneNode(true) as HTMLElement;
  copy.querySelectorAll('[aria-hidden="true"]').forEach((n) => n.remove());
  copy.querySelectorAll('[role="img"][aria-label]').forEach((n) => {
    n.replaceWith(`${n.getAttribute("aria-label")}: `);
  });
  return (copy.textContent ?? "").replace(/\s+/g, " ").trim();
}

export function initFormAriaSync(root: HTMLElement | Document = document): () => void {
  const useNativeUserInvalid = supportsUserInvalid();
  const dirtyState = new WeakMap<Element, DirtyState>();
  const flagged = new WeakSet<Element>();
  const announced = new WeakSet<Element>();

  const announcer = document.createElement("div");
  announcer.setAttribute("aria-live", "polite");
  announcer.classList.add("visually-hidden");
  document.body.append(announcer);

  let announceTimer: ReturnType<typeof setTimeout> | undefined;
  let announcedField: Element | undefined;

  const announceError = (el: Element) => {
    const errorId = el.getAttribute("data-error");
    if (!errorId) return;
    const errorEl = document.getElementById(errorId);
    if (!errorEl) return;
    const labels = (el as HTMLInputElement).labels;
    const labelText = labels?.[0] ? `${spokenText(labels[0])}: ` : "";
    clearTimeout(announceTimer);
    announcer.textContent = "";
    announcedField = el;
    announceTimer = setTimeout(() => {
      announcer.textContent = labelText + spokenText(errorEl);
    }, 500);
  };

  const applyValidity = (el: Element, isInvalid: boolean, announce: boolean) => {
    if (isInvalid) {
      if (announce && !announced.has(el)) {
        announceError(el);
        announced.add(el);
      }
      el.setAttribute("aria-invalid", "true");
      linkError(el, true);
      flagged.add(el);
    } else if (flagged.has(el) || el.getAttribute("aria-invalid") !== "true") {
      el.setAttribute("aria-invalid", "false");
      linkError(el, false);
      flagged.delete(el);
      announced.delete(el);
      if (announcedField === el) {
        clearTimeout(announceTimer);
        announcer.textContent = "";
      }
    }
  };

  const syncNative = (el: Element, announce = false) => {
    if (!el.matches?.("input, select, textarea")) return;
    const isInvalid = el.matches(":user-invalid");
    applyValidity(el, isInvalid, announce);
  };

  const syncFallback = (el: Element, announce = false) => {
    const control = el as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement;
    if (typeof control.validity === "undefined") return;
    const isInvalid = !control.validity.valid;
    control.classList.toggle("user-invalid-fallback", isInvalid);
    control.classList.toggle("user-valid-fallback", !isInvalid);
    applyValidity(control, isInvalid, announce);
  };

  root.querySelectorAll("input, select, textarea").forEach((el) => {
    if (!el.hasAttribute("aria-invalid")) {
      el.setAttribute("aria-invalid", "false");
    }
    if (useNativeUserInvalid) {
      syncNative(el);
    }
  });

  const onBlur = (e: Event) => {
    const target = e.target as Element | null;
    if (!target) return;
    if (useNativeUserInvalid) {
      syncNative(target, true);
      return;
    }
    const state = dirtyState.get(target) ?? { hasInteracted: false, hasBlurred: false };
    state.hasBlurred = true;
    dirtyState.set(target, state);
    if (state.hasInteracted) syncFallback(target, true);
  };

  const onInput = (e: Event) => {
    const target = e.target as Element | null;
    if (!target) return;
    if (useNativeUserInvalid) {
      if (target.hasAttribute("aria-invalid")) syncNative(target, false);
      return;
    }
    const state = dirtyState.get(target) ?? { hasInteracted: false, hasBlurred: false };
    state.hasInteracted = true;
    dirtyState.set(target, state);
    if (state.hasBlurred) syncFallback(target, false);
  };

  const onChange = (e: Event) => {
    const target = e.target as Element | null;
    if (!target) return;
    if (useNativeUserInvalid) {
      syncNative(target, true);
      return;
    }
    const state = dirtyState.get(target) ?? { hasInteracted: false, hasBlurred: false };
    state.hasInteracted = true;
    dirtyState.set(target, state);
    syncFallback(target, true);
  };

  const onInvalid = (e: Event) => {
    const target = e.target as Element | null;
    if (!target) return;
    announced.add(target);
    if (useNativeUserInvalid) {
      syncNative(target, false);
      return;
    }
    dirtyState.set(target, { hasInteracted: true, hasBlurred: true });
    syncFallback(target, false);
  };

  const onReset = (e: Event) => {
    const form = e.target as HTMLFormElement | null;
    if (!form?.elements) return;
    setTimeout(() => {
      for (const el of Array.from(form.elements)) {
        dirtyState.delete(el);
        el.classList.remove("user-invalid-fallback", "user-valid-fallback");
        if (el.matches("input, select, textarea")) {
          el.setAttribute("aria-invalid", "false");
          linkError(el, false);
        }
      }
    });
  };

  root.addEventListener("blur", onBlur, true);
  root.addEventListener("input", onInput, true);
  root.addEventListener("change", onChange, true);
  root.addEventListener("invalid", onInvalid, true);
  root.addEventListener("reset", onReset, true);

  return () => {
    clearTimeout(announceTimer);
    root.removeEventListener("blur", onBlur, true);
    root.removeEventListener("input", onInput, true);
    root.removeEventListener("change", onChange, true);
    root.removeEventListener("invalid", onInvalid, true);
    root.removeEventListener("reset", onReset, true);
    announcer.remove();
  };
}
