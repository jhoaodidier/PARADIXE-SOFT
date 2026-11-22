export function ensureFocusgroupPolyfill(): void {
  if (typeof HTMLElement !== "undefined" && !Object.hasOwn(HTMLElement.prototype, "focusGroup")) {
    void import("@microsoft/focusgroup-polyfill")
      .then((mod) => {
        if (typeof document === "undefined" || !document.body) return;
        if (typeof mod.polyfillBodyAndObserve === "function") {
          mod.polyfillBodyAndObserve();
        } else if (typeof mod.polyfill === "function") {
          mod.polyfill(document.body);
        }
      })
      .catch(() => {
        // Safe ignore in constrained test/offline environments
      });
  }
}
