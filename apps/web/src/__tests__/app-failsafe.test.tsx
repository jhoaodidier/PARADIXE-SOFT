import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App.tsx";
import type { CorridorSegmentFeature } from "../api/client.ts";

vi.mock("maplibre-gl", () => {
  class MockMap {
    on() {}
    addControl() {}
    addSource() {}
    addLayer() {}
    getSource() {
      return { setData: () => {} };
    }
    remove() {}
  }
  class MockNavigationControl {}
  return {
    default: {
      Map: MockMap,
      NavigationControl: MockNavigationControl,
      addProtocol: () => {},
    },
  };
});

vi.mock("pmtiles", () => ({
  Protocol: class {
    tile = () => {};
  },
}));

const SAMPLE_FEATURES: CorridorSegmentFeature[] = [
  {
    type: "Feature",
    geometry: {
      type: "LineString",
      coordinates: [
        [-76.24, -9.93],
        [-76.09, -9.71],
      ],
    },
    properties: {
      segment_id: "33333333-3333-4333-8333-333333333333",
      origin_node_id: "11111111-1111-4111-8111-111111111111",
      destination_node_id: "22222222-2222-4222-8222-222222222222",
      corridor_name: "Huánuco – Túnel Carpish",
      direction_type: "BIDIRECTIONAL",
      length_km: 42.5,
      surface_type: "PAVED",
      passability_state: "BLOCKED",
      ivv_score: 84,
      risk_level: "RED",
      observed_at: "2026-11-21T20:00:00Z",
      calculated_at: "2026-11-21T20:05:00Z",
      model_version_tag: "ivv-v1.0-initial",
      is_stale: true,
      is_attributes_verified: false,
    },
  },
];

describe("UI Fail-Safe y Accesibilidad PARADIXE SOFT", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    Object.defineProperty(window.navigator, "onLine", {
      configurable: true,
      value: true,
    });
    if (!HTMLDialogElement.prototype.showModal) {
      HTMLDialogElement.prototype.showModal = function () {
        this.setAttribute("open", "");
      };
      HTMLDialogElement.prototype.close = function () {
        this.removeAttribute("open");
        this.dispatchEvent(new Event("close"));
      };
    }
  });

  it("expone landmarks semanticos unicos, un h1 y aria-invalid=false inicial", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async () => {
      return new Response(
        JSON.stringify({
          type: "FeatureCollection",
          generated_at: "2026-11-21T23:00:00Z",
          features: SAMPLE_FEATURES,
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);

    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary")).toBeInTheDocument();
    expect(screen.getByText(/STALE_DATA:/)).toBeInTheDocument();

    const originSelect = screen.getByLabelText("Nodo origen");
    expect(originSelect).toHaveAttribute("aria-invalid", "false");
  });

  it("renderiza NO_SAFE_ROUTE con reason_code unico sin inventar ruta", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(async (input) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("/routing/calculate")) {
        return new Response(
          JSON.stringify({
            resolution_type: "NO_SAFE_ROUTE",
            blocking_incident_id: null,
            reason_code: "STALE_IVV",
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      return new Response(
        JSON.stringify({
          type: "FeatureCollection",
          generated_at: "2026-11-21T23:00:00Z",
          features: SAMPLE_FEATURES,
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);
    const submitBtn = screen.getByRole("button", { name: "Calcular ruta verificable" });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(screen.getByText(/NO_SAFE_ROUTE \(STALE_IVV\)/)).toBeInTheDocument();
    });
  });

  it("bloquea el calculo de rutas cuando esta offline y encola reporte en IndexedDB", async () => {
    Object.defineProperty(window.navigator, "onLine", {
      configurable: true,
      value: false,
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);

    expect(screen.getByText(/Modo Sin Conexión \(Offline-First\)/)).toBeInTheDocument();
    const routeSubmitBtn = screen.getByRole("button", { name: "Calcular ruta verificable" });
    expect(routeSubmitBtn).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Reportar vía" }));
    const saveOfflineBtn = await screen.findByRole("button", { name: "Guardar offline" });
    fireEvent.click(saveOfflineBtn);

    await waitFor(() => {
      expect(screen.getByText(/Cola outbox_queue:/)).toBeInTheDocument();
    });
  });
});
