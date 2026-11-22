import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App.tsx";
import type { CorridorSegmentFeature } from "../api/client.ts";

vi.mock("maplibre-gl", () => {
  class MockMap {
    on() {}
    addControl() {}
    addSource() {}
    addLayer() {}
    getLayer() {
      return null;
    }
    setLayoutProperty() {}
    setPaintProperty() {}
    setFilter() {}
    fitBounds() {}
    flyTo() {}
    zoomIn() {}
    zoomOut() {}
    getCanvas() {
      return { style: { cursor: "" } };
    }
    getSource() {
      return { setData: () => {} };
    }
    remove() {}
  }
  class MockPopup {
    setLngLat() {
      return this;
    }
    setDOMContent() {
      return this;
    }
    addTo() {
      return this;
    }
    remove() {}
  }
  class MockNavigationControl {}
  return {
    default: {
      Map: MockMap,
      Popup: MockPopup,
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

const SAMPLE_REGIONS = {
  regions: [
    {
      unit_id: "aaaa0000-0000-4000-8000-000000000001",
      ubigeo_code: "10",
      iso_code: "PE-HUC",
      parent_unit_id: null,
      name: "Departamento de Huánuco",
      admin_level: 4,
      center_lon: -76.1,
      center_lat: -9.6,
      min_lon: -77.3,
      min_lat: -10.5,
      max_lon: -74.6,
      max_lat: -8.4,
      pmtiles_uri: "http://localhost:9000/paradixe-tiles/pe-huc.pmtiles",
      segment_count: 10703,
    },
    {
      unit_id: "aaaa0000-0000-4000-8000-000000000002",
      ubigeo_code: "1001",
      iso_code: null,
      parent_unit_id: "aaaa0000-0000-4000-8000-000000000001",
      name: "Provincia de Huánuco",
      admin_level: 6,
      center_lon: -76.24,
      center_lat: -9.93,
      min_lon: -76.6,
      min_lat: -10.2,
      max_lon: -75.8,
      max_lat: -9.5,
      pmtiles_uri: null,
      segment_count: 1840,
    },
    {
      unit_id: "aaaa0000-0000-4000-8000-000000000003",
      ubigeo_code: "100102",
      iso_code: null,
      parent_unit_id: "aaaa0000-0000-4000-8000-000000000002",
      name: "Distrito de Amarilis",
      admin_level: 8,
      center_lon: -76.23,
      center_lat: -9.94,
      min_lon: -76.35,
      min_lat: -10.05,
      max_lon: -76.1,
      max_lat: -9.85,
      pmtiles_uri: null,
      segment_count: 120,
    },
  ],
};

function createDefaultFetchMock(overrides?: (url: string, init?: RequestInit) => Response | null) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url = typeof input === "string" ? input : input.toString();
    if (overrides) {
      const custom = overrides(url, init);
      if (custom) return custom;
    }
    if (url.includes("/regions")) {
      return new Response(JSON.stringify(SAMPLE_REGIONS), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }
    if (url.includes("/nodes")) {
      return new Response(JSON.stringify({ nodes: [] }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }
    if (url.includes("/incidents")) {
      return new Response(JSON.stringify({ incidents: [] }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }
    if (url.includes("/operations/bridges")) {
      return new Response(
        JSON.stringify({
          transfer_corridors: [],
          bridges: [],
          humanitarian_windows: [],
          vehicle_categories: [],
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    }
    if (url.includes("/sync/snapshot")) {
      return new Response(
        JSON.stringify({
          next_since_version: 23,
          server_time: "2026-11-22T14:00:00Z",
          has_more: false,
          changes: [],
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    }
    return new Response(
      JSON.stringify({
        type: "FeatureCollection",
        generated_at: "2026-11-22T14:00:00Z",
        features: SAMPLE_FEATURES,
      }),
      { status: 200, headers: { "Content-Type": "application/json" } }
    );
  });
}

describe("UI Comercial, Fail-Safe y Accesibilidad PARADIXE SOFT", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    try {
      localStorage.clear();
    } catch {
      // Ignore if jsdom localStorage is restricted
    }
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

  it("expone landmarks semanticos unicos, un h1, controles de mapa y aria-invalid=false inicial", async () => {
    createDefaultFetchMock();

    render(<App initialFeatures={SAMPLE_FEATURES} />);

    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.getByRole("banner")).toBeInTheDocument();
    expect(screen.getByRole("main")).toBeInTheDocument();
    expect(screen.getByRole("complementary")).toBeInTheDocument();
    expect(screen.getByText(/STALE_DATA:/)).toBeInTheDocument();

    const originSelect = screen.getByLabelText("Nodo origen");
    expect(originSelect).toHaveAttribute("aria-invalid", "false");

    const ivvMeter = screen.getByRole("progressbar", { name: "Índice de Vulnerabilidad Vial" });
    expect(ivvMeter).toHaveAttribute("max", "100");
    expect(screen.getByLabelText("Camioneta 4x4").closest("label")).toHaveClass("choice-card");
    expect(screen.getByRole("button", { name: "Capas" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mapa" })).toBeInTheDocument();
  });

  it("renderiza NO_SAFE_ROUTE con reason_code unico sin inventar ruta", async () => {
    createDefaultFetchMock((url) => {
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
      return null;
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

  it("filtra el mapa por provincia territorial enviando region y bbox al API", async () => {
    const fetchSpy = createDefaultFetchMock();

    render(<App initialFeatures={SAMPLE_FEATURES} />);

    fireEvent.click(screen.getByRole("tab", { name: "Explorar" }));
    const provinceSelect = await screen.findByLabelText(/Provincia \(1\)/);
    fireEvent.change(provinceSelect, {
      target: { value: "aaaa0000-0000-4000-8000-000000000002" },
    });

    await waitFor(() => {
      const calls = fetchSpy.mock.calls.map(([u]) => String(u));
      expect(
        calls.some(
          (u) =>
            u.includes("/corridors/map") &&
            u.includes("region=PE-HUC") &&
            u.includes("bbox=-76.6%2C-10.2%2C-75.8%2C-9.5")
        )
      ).toBe(true);
    });
  });

  it("cumple autofill-sign-in-form y sms-otp-form en el modal de acceso", async () => {
    createDefaultFetchMock((url) => {
      if (url.endsWith("/auth/otp/request")) {
        return new Response(
          JSON.stringify({
            challenge_id: "99999999-9999-4999-8999-999999999999",
            expires_in_sec: 300,
            message: "Dispatched",
          }),
          { status: 202, headers: { "Content-Type": "application/json" } }
        );
      }
      return null;
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);
    const header = screen.getByRole("banner");
    fireEvent.click(within(header).getByRole("button", { name: "Iniciar sesión" }));

    const phoneInput = screen.getByLabelText("Teléfono institucional (E.164)");
    const pwInput = screen.getByLabelText("Contraseña");
    expect(phoneInput).toHaveAttribute("autocomplete", "username tel");
    expect(pwInput).toHaveAttribute("autocomplete", "current-password");
    expect(pwInput).toHaveAttribute("type", "password");

    const togglePwBtn = screen.getByRole("button", { name: "Mostrar" });
    fireEvent.click(togglePwBtn);
    expect(pwInput).toHaveAttribute("type", "text");
    expect(togglePwBtn).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", { name: "SMS OTP (6 dígitos)" }));
    fireEvent.click(screen.getByRole("button", { name: "Solicitar código SMS" }));

    const otpInput = await screen.findByLabelText("Código de verificación OTP");
    expect(otpInput).toHaveAttribute("autocomplete", "one-time-code");
    expect(otpInput).toHaveAttribute("inputmode", "numeric");
    expect(otpInput).toHaveAttribute("pattern", "\\d{6}");

    const hintEl = screen.getByText(/Ingrese el código de 6 dígitos enviado por SMS/);
    expect(hintEl.compareDocumentPosition(otpInput) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("cambia pestañas operativas y ejecuta flujo aislado del Simulador COER_ADMIN", async () => {
    createDefaultFetchMock((url, init) => {
      if (url.endsWith("/auth/login")) {
        return new Response(
          JSON.stringify({
            access_token: "mock.jwt.token",
            refresh_token: "mock-refresh",
            token_type: "Bearer",
            expires_in_sec: 900,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "00000000-0000-4000-8000-000000000001",
            phone_e164: "+51962000100",
            full_name: "COER Huánuco (Admin)",
            role_code: "COER_ADMIN",
            reputation_score: 1.0,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/simulator/scenarios")) {
        return new Response(
          JSON.stringify({
            scenario_id: "55555555-5555-4555-8555-555555555555",
            name: "Simulacro Carpish",
            fixture_tag: "huanuco-carpish-drill-v1",
            revision: 0,
            status: "DRAFT",
            created_at: "2026-11-22T14:00:00Z",
          }),
          { status: 201, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/simulator/scenarios/") && url.endsWith("/events")) {
        return new Response(
          JSON.stringify({
            sim_event_id: "66666666-6666-4666-8666-666666666666",
            scenario_id: "55555555-5555-4555-8555-555555555555",
            execution_context: "SIMULATION",
            simulated_notifications: [
              { channel: "SIMULATION", preview: "[SIMULATION] Alerta geotecnica Carpish." },
            ],
          }),
          { status: 201, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/simulator/scenarios/") && url.endsWith("/routing")) {
        return new Response(
          JSON.stringify({
            execution_context: "SIMULATION",
            scenario_id: "55555555-5555-4555-8555-555555555555",
            resolution_type: "ALTERNATE_ROUTE",
            routing_config_tag: "rtc-v1.0-initial",
            total_distance_km: 170.0,
            estimated_time_min: 260.0,
            max_ivv_on_route: 32.0,
            hops: [],
            eta_quality: "NOMINAL",
            operational_authorization: "REQUIRED",
            warnings: [],
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/simulator/scenarios/") && url.endsWith("/evaluate") && init?.method === "POST") {
        return new Response(
          JSON.stringify({
            sim_log_id: "77777777-7777-4777-8777-777777777777",
            resolution_type: "ALTERNATE_ROUTE",
            mitigation_status: "MITIGATED",
            scenario_revision: 1,
            evaluation_time: "2026-11-20T09:00:00Z",
            param_version_tag: "imp-v1.0-initial",
            model_version_tag: "ivv-v1.0-initial",
            routing_config_tag: "rtc-v1.0-initial",
          }),
          { status: 201, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/simulator/scenarios/") && url.endsWith("/impact")) {
        return new Response(
          JSON.stringify({
            scenario_id: "55555555-5555-4555-8555-555555555555",
            evaluations: [
              {
                sim_log_id: "77777777-7777-4777-8777-777777777777",
                resolution_type: "ALTERNATE_ROUTE",
                mitigation_status: "MITIGATED",
                baseline: { travel_min: 505.0, co2_kg: 38.6, cargo_loss_usd: 151.67 },
                mitigated: { travel_min: 260.0, co2_kg: 42.5, cargo_loss_usd: 0.0 },
                deltas: {
                  time_delta_min: 245.0,
                  time_saved_pct: 48.51,
                  net_co2_delta_kg: -3.9,
                  net_agri_savings_usd: 151.67,
                },
              },
            ],
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      return null;
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);

    const header = screen.getByRole("banner");
    fireEvent.click(within(header).getByRole("button", { name: "Iniciar sesión" }));
    fireEvent.change(screen.getByLabelText("Contraseña"), {
      target: { value: "ParadixeDev2026!" },
    });
    const authDialog = screen.getByRole("dialog", { name: "Acceso de Operador Territorial" });
    fireEvent.click(within(authDialog).getByRole("button", { name: "Iniciar sesión" }));
    await within(header).findByText(/COER Huánuco \(Admin\)/);

    const routingTab = screen.getByRole("tab", { name: "Rutas" });
    const simTab = screen.getByRole("tab", { name: "Simulador" });
    fireEvent.click(simTab);
    expect(simTab).toHaveAttribute("aria-selected", "true");
    expect(routingTab).toHaveAttribute("aria-selected", "false");
    const routingPanel = screen
      .getAllByRole("tabpanel", { hidden: true })
      .find((el) => el.getAttribute("aria-labelledby") === routingTab.id);
    expect(routingPanel).toBeDefined();
    expect(routingPanel).toHaveAttribute("hidden", "until-found");
    expect(screen.getByText(/\[SIMULATION\]/)).toBeInTheDocument();

    const createBtn = await screen.findByRole("button", { name: "Crear escenario aislado" });
    fireEvent.click(createBtn);

    const injectBtn = await screen.findByRole("button", { name: /Inyectar evento/ });
    fireEvent.click(injectBtn);
    await screen.findByText(/\[SIMULATION\] Alerta geotecnica Carpish\./);

    fireEvent.click(screen.getByRole("button", { name: "Ejecutar ruta + evaluar impacto" }));
    await waitFor(() => {
      expect(screen.getByText(/Delta CO₂: -3\.90 kg/)).toBeInTheDocument();
      expect(screen.getByText(/Ahorro neto: \$151\.67/)).toBeInTheDocument();
    });
  });

  it("permite a COER_ADMIN certificar atributos fisicos desde el inspector contextual", async () => {
    const fetchSpy = createDefaultFetchMock((url, init) => {
      if (url.endsWith("/auth/login")) {
        return new Response(
          JSON.stringify({
            access_token: "mock.jwt.token",
            refresh_token: "mock-refresh",
            token_type: "Bearer",
            expires_in_sec: 900,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "00000000-0000-4000-8000-000000000001",
            phone_e164: "+51962000100",
            full_name: "COER Huánuco (Admin)",
            role_code: "COER_ADMIN",
            reputation_score: 1.0,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/segments/") && url.endsWith("/attributes") && init?.method === "PATCH") {
        return new Response(
          JSON.stringify({
            segment_id: "33333333-3333-4333-8333-333333333333",
            corridor_name: "Huánuco – Túnel Carpish",
            surface_type: "PAVED",
            road_width_m: 6.5,
            max_supported_tons: 30,
            max_slope_forward_pct: 18,
            max_slope_reverse_pct: 18,
            is_historical_hotspot: true,
            geometry_source: "FIELD_SURVEY",
            attributes_verified_at: "2026-11-22T17:20:00Z",
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/corridors/forecast")) {
        return new Response(
          JSON.stringify({
            region: "PE-HUC",
            horizon_hours: 24,
            advisory_only: true,
            generated_at: "2026-11-22T19:57:00Z",
            summary: { green: 0, amber: 1, red: 0 },
            items: [
              {
                segment_id: "33333333-3333-4333-8333-333333333333",
                corridor_name: "Huánuco – Túnel Carpish",
                precip_24h_mm: 42.5,
                precip_72h_mm: 110.0,
                predicted_ivv_score: 58.5,
                predicted_risk_level: "AMBER",
                generated_at: "2026-11-22T19:57:00Z",
              },
            ],
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/analytics/summary")) {
        return new Response(
          JSON.stringify({
            region: "PE-HUC",
            generated_at: "2026-11-22T19:57:00Z",
            total_segments: 10703,
            certified_segments: 1369,
            certified_pct: 12.8,
            risk_counts: { green: 10480, amber: 220, red: 3, unknown: 0 },
            active_incidents_total: 0,
            blocked_segments: 0,
            partial_segments: 0,
            active_bridges: 0,
            active_windows: 0,
            verified_transfer_corridors: 0,
            hospitals_count: 217,
            regions_count: 4,
            provinces_count: 27,
            districts_count: 424,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      return null;
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);

    const header = screen.getByRole("banner");
    fireEvent.click(within(header).getByRole("button", { name: "Iniciar sesión" }));
    fireEvent.change(screen.getByLabelText("Contraseña"), {
      target: { value: "ParadixeDev2026!" },
    });
    const authDialog = screen.getByRole("dialog", { name: "Acceso de Operador Territorial" });
    fireEvent.click(within(authDialog).getByRole("button", { name: "Iniciar sesión" }));
    await within(header).findByText(/COER Huánuco \(Admin\)/);

    const certifyBtn = await screen.findByRole("button", { name: "Certificar tramo" });
    fireEvent.click(certifyBtn);

    await waitFor(() => {
      expect(
        fetchSpy.mock.calls.some(
          ([u, init]) =>
            String(u).endsWith("/segments/33333333-3333-4333-8333-333333333333/attributes") &&
            (init as RequestInit | undefined)?.method === "PATCH"
        )
      ).toBe(true);
    });
  });

  it("consulta pronostico IVV predictivo +24h y abre centro de control con KPIs", async () => {
    const fetchSpy = createDefaultFetchMock((url) => {
      if (url.includes("/corridors/forecast")) {
        return new Response(
          JSON.stringify({
            region: "PE-HUC",
            horizon_hours: 24,
            advisory_only: true,
            generated_at: "2026-11-22T19:57:00Z",
            summary: { green: 0, amber: 1, red: 0 },
            items: [
              {
                segment_id: "33333333-3333-4333-8333-333333333333",
                corridor_name: "Huánuco – Túnel Carpish",
                precip_24h_mm: 42.5,
                precip_72h_mm: 110.0,
                predicted_ivv_score: 58.5,
                predicted_risk_level: "AMBER",
                generated_at: "2026-11-22T19:57:00Z",
              },
            ],
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.includes("/analytics/summary")) {
        return new Response(
          JSON.stringify({
            region: "PE-HUC",
            generated_at: "2026-11-22T19:57:00Z",
            total_segments: 10703,
            certified_segments: 1369,
            certified_pct: 12.8,
            risk_counts: { green: 10480, amber: 220, red: 3, unknown: 0 },
            active_incidents_total: 0,
            blocked_segments: 0,
            partial_segments: 0,
            active_bridges: 0,
            active_windows: 0,
            verified_transfer_corridors: 0,
            hospitals_count: 217,
            regions_count: 4,
            provinces_count: 27,
            districts_count: 424,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      return null;
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);

    fireEvent.click(screen.getByRole("button", { name: "+24h" }));
    await waitFor(() => {
      expect(screen.getByText("Pronóstico +24h")).toBeInTheDocument();
      expect(
        fetchSpy.mock.calls.some(([u]) => String(u).includes("/corridors/forecast?horizon_hours=24"))
      ).toBe(true);
    });

    fireEvent.click(screen.getByRole("button", { name: "Configuración" }));
    expect(await screen.findByText(/Centro de Control y Ajustes/)).toBeInTheDocument();
    expect(await screen.findByText("12.8%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Exportar CSV" })).toBeInTheDocument();
  });

  it("muestra miniaturas de evidencia en Alertas y abre visor lightbox nativo", async () => {
    createDefaultFetchMock((url) => {
      if (url.includes("/incidents")) {
        return new Response(
          JSON.stringify({
            incidents: [
              {
                incident_id: "inc-pic-1",
                segment_id: "33333333-3333-4333-8333-333333333333",
                corridor_name: "Huánuco – Túnel Carpish",
                trigger_source: "FIELD_CONSENSUS",
                verification_state: "CONFIRMED",
                impact_passability: "BLOCKED",
                hazard_lat: -9.71,
                hazard_lon: -76.09,
                hazard_radius_m: 150,
                detected_at: "2026-11-22T15:00:00Z",
                confirmed_at: "2026-11-22T15:05:00Z",
                resolved_at: null,
                evidence_photos: ["/evidence/20261122/sample.jpg"],
              },
            ],
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      return null;
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);
    fireEvent.click(screen.getByRole("tab", { name: /Alertas/ }));

    const thumb = await screen.findByAltText("Evidencia en Huánuco – Túnel Carpish");
    expect(thumb).toHaveAttribute("loading", "lazy");
    expect(thumb).toHaveAttribute("src", "/evidence/20261122/sample.jpg");

    fireEvent.click(
      screen.getByRole("button", { name: "Ampliar foto 1 de Huánuco – Túnel Carpish" })
    );
    const fullImg = await screen.findByAltText("Evidencia ampliada de Huánuco – Túnel Carpish");
    expect(fullImg).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cerrar foto ampliada" }));
    await waitFor(() => {
      expect(screen.queryByAltText("Evidencia ampliada de Huánuco – Túnel Carpish")).toBeNull();
    });
  });

  it("renueva JWT automaticamente ante 401 en /routing/calculate y reintenta la peticion", async () => {
    let routingAttempts = 0;
    const fetchSpy = createDefaultFetchMock((url, init) => {
      if (url.endsWith("/auth/login")) {
        return new Response(
          JSON.stringify({
            access_token: "expired-jwt",
            refresh_token: "valid-refresh-1",
            token_type: "Bearer",
            expires_in_sec: 900,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "u-coer",
            phone_e164: "+51962000100",
            full_name: "COER Huánuco (Admin)",
            role_code: "COER_ADMIN",
            reputation_score: 1.0,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/auth/refresh")) {
        return new Response(
          JSON.stringify({
            access_token: "fresh-jwt-2",
            refresh_token: "valid-refresh-2",
            token_type: "Bearer",
            expires_in_sec: 900,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/routing/calculate")) {
        routingAttempts += 1;
        const headers = new Headers(init?.headers);
        if (routingAttempts === 1) {
          return new Response(
            JSON.stringify({
              error_code: "UNAUTHORIZED",
              message: "Expired token",
              details: {},
              request_id: "r1",
            }),
            { status: 401, headers: { "Content-Type": "application/json" } }
          );
        }
        expect(headers.get("Authorization")).toBe("Bearer fresh-jwt-2");
        return new Response(
          JSON.stringify({
            resolution_type: "ALTERNATE_ROUTE",
            routing_config_tag: "routing-v1.0",
            total_distance_km: 12.4,
            estimated_time_min: 18,
            max_ivv_on_route: 28,
            hops: [],
            eta_quality: "NOMINAL",
            operational_authorization: "REQUIRES_AUTHORITY_APPROVAL",
            warnings: [],
            economic_impact: {
              estimated_co2_kg: 3.1,
              baseline_blockage_co2_kg: 9.2,
              co2_delta_kg: 6.1,
              estimated_cargo_loss_avoided_usd: 420.5,
              time_saved_vs_blockage_min: 222,
              reference_blockage_hours: 4.0,
            },
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      return null;
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);
    const header = screen.getByRole("banner");
    fireEvent.click(within(header).getByRole("button", { name: "Iniciar sesión" }));
    fireEvent.change(screen.getByLabelText("Contraseña"), {
      target: { value: "ParadixeDev2026!" },
    });
    const authDialog = screen.getByRole("dialog", { name: "Acceso de Operador Territorial" });
    fireEvent.click(within(authDialog).getByRole("button", { name: "Iniciar sesión" }));
    await within(header).findByText(/COER Huánuco \(Admin\)/);

    fireEvent.click(screen.getByRole("button", { name: "Calcular ruta verificable" }));
    expect(await screen.findByText("12.4 km")).toBeInTheDocument();
    expect(
      fetchSpy.mock.calls.some(([u]) => String(u).endsWith("/auth/refresh"))
    ).toBe(true);
  });

  it("hidrata region, vehiculo y carga desde deep-link URL al iniciar", async () => {
    window.history.replaceState(
      {},
      "",
      "/?region=PE-JUN&vehicle=PICKUP_4X4&cargo=PERISHABLE_AGRI"
    );
    try {
      const fetchSpy = createDefaultFetchMock();
      render(<App initialFeatures={SAMPLE_FEATURES} />);

      expect(screen.getByLabelText("Camioneta 4x4")).toBeChecked();
      expect(screen.getByLabelText("Agrícola")).toBeChecked();
      await waitFor(() => {
        expect(
          fetchSpy.mock.calls.some(([u]) => String(u).includes("region=PE-JUN"))
        ).toBe(true);
      });
    } finally {
      window.history.replaceState({}, "", "/");
    }
  });

  it("permite a COER_ADMIN consultar equipo y emitir API Key B2B en Ajustes", async () => {
    createDefaultFetchMock((url, init) => {
      if (url.endsWith("/auth/login")) {
        return new Response(
          JSON.stringify({
            access_token: "admin-jwt",
            refresh_token: "admin-ref",
            token_type: "Bearer",
            expires_in_sec: 900,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "u-coer",
            phone_e164: "+51962000100",
            full_name: "COER Huánuco (Admin)",
            role_code: "COER_ADMIN",
            reputation_score: 1.0,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/admin/users")) {
        return new Response(
          JSON.stringify({
            users: [
              {
                user_id: "u-def",
                phone_e164: "+51962000200",
                full_name: "Defensa Civil Huánuco",
                role_code: "CIVIL_DEFENSE",
                reputation_score: 0.9,
                is_active: true,
                created_at: "2026-11-22T00:00:00Z",
              },
            ],
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/b2b/api-keys") && init?.method === "POST") {
        return new Response(
          JSON.stringify({
            api_key_id: "key-1",
            organization_name: "AgroExport Centro SAC",
            key_prefix: "pxk_live_demo123",
            raw_api_key: "pxk_live_demo1234567890abcdef",
            role_code: "DRIVER",
            rate_limit_rpm: 120,
            created_at: "2026-11-22T15:40:00Z",
          }),
          { status: 201, headers: { "Content-Type": "application/json" } }
        );
      }
      if (url.endsWith("/b2b/api-keys")) {
        return new Response(JSON.stringify({ api_keys: [] }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      if (url.endsWith("/b2b/webhooks")) {
        return new Response(JSON.stringify({ webhooks: [] }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      if (url.endsWith("/subscriptions")) {
        return new Response(JSON.stringify({ subscriptions: [] }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      }
      if (url.includes("/analytics/summary")) {
        return new Response(
          JSON.stringify({
            region: "PE-HUC",
            generated_at: "2026-11-22T19:57:00Z",
            total_segments: 10694,
            certified_segments: 1369,
            certified_pct: 12.8,
            risk_counts: { green: 10000, amber: 690, red: 4, unknown: 0 },
            active_incidents_total: 0,
            blocked_segments: 0,
            partial_segments: 0,
            active_bridges: 0,
            active_windows: 0,
            verified_transfer_corridors: 1,
            hospitals_count: 215,
            regions_count: 4,
            provinces_count: 27,
            districts_count: 424,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }
      return null;
    });

    render(<App initialFeatures={SAMPLE_FEATURES} />);
    const header = screen.getByRole("banner");
    fireEvent.click(within(header).getByRole("button", { name: "Iniciar sesión" }));
    fireEvent.change(screen.getByLabelText("Contraseña"), {
      target: { value: "ParadixeDev2026!" },
    });
    const authDialog = screen.getByRole("dialog", { name: "Acceso de Operador Territorial" });
    fireEvent.click(within(authDialog).getByRole("button", { name: "Iniciar sesión" }));
    await within(header).findByText(/COER Huánuco \(Admin\)/);

    fireEvent.click(screen.getByRole("button", { name: "Configuración" }));
    expect(await screen.findByText(/Equipo COER y Operadores \(1\)/)).toBeInTheDocument();
    expect(screen.getByText("Defensa Civil Huánuco")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Empresa / Flota"), {
      target: { value: "AgroExport Centro SAC" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Emitir API Key B2B" }));
    expect(await screen.findByText("pxk_live_demo1234567890abcdef")).toBeInTheDocument();
  });
});
