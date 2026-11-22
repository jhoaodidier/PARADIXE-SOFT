let cachedSupportsDisplayTransition: boolean | undefined;

export function ensurePopoverPolyfill(): void {
  if (typeof HTMLElement !== "undefined" && !Object.hasOwn(HTMLElement.prototype, "popover")) {
    void import("@oddbird/popover-polyfill");
  }
}

export function canTransitionDisplay(): boolean {
  if (cachedSupportsDisplayTransition !== undefined) {
    return cachedSupportsDisplayTransition;
  }
  if (
    typeof window === "undefined" ||
    !window.CSS?.supports?.("transition-behavior", "allow-discrete") ||
    !document.body
  ) {
    return false;
  }
  const probe = document.createElement("div");
  probe.style.cssText = "transition: display 1s allow-discrete !important; display: block;";
  document.body.appendChild(probe);
  getComputedStyle(probe).display;
  probe.style.display = "none";
  cachedSupportsDisplayTransition = getComputedStyle(probe).display === "block";
  probe.remove();
  return cachedSupportsDisplayTransition;
}

export function supportsTopLayerExit(): boolean {
  return Boolean(
    typeof window !== "undefined" &&
      window.CSS?.supports?.("overlay", "auto") &&
      canTransitionDisplay()
  );
}

export async function closeTopLayer(dialog: HTMLDialogElement): Promise<void> {
  if (!dialog.open) return;
  if (!supportsTopLayerExit() && typeof dialog.getAnimations === "function") {
    dialog.setAttribute("data-closing", "");
    const animations = dialog.getAnimations({ subtree: true });
    if (animations.length > 0) {
      await Promise.race([
        Promise.allSettled(animations.map((a) => a.finished)),
        new Promise((resolve) => setTimeout(resolve, 2000)),
      ]);
    }
    if (!dialog.hasAttribute("data-closing")) return;
    dialog.removeAttribute("data-closing");
  }
  dialog.close();
}

export function attachDialogLightDismissFallback(dialog: HTMLDialogElement): () => void {
  if (typeof HTMLDialogElement === "undefined" || "closedBy" in HTMLDialogElement.prototype) {
    return () => {};
  }
  const outside = (e: PointerEvent): boolean => {
    const r = dialog.getBoundingClientRect();
    return e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom;
  };
  let pressedOutside = false;
  const onPointerDown = (e: PointerEvent) => {
    pressedOutside = outside(e);
  };
  const onPointerUp = (e: PointerEvent) => {
    if (pressedOutside && outside(e)) {
      void closeTopLayer(dialog);
    }
    pressedOutside = false;
  };
  dialog.addEventListener("pointerdown", onPointerDown);
  dialog.addEventListener("pointerup", onPointerUp);
  return () => {
    dialog.removeEventListener("pointerdown", onPointerDown);
    dialog.removeEventListener("pointerup", onPointerUp);
  };
}
