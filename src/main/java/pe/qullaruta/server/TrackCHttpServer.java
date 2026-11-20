package pe.qullaruta.server;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpHandler;
import com.sun.net.httpserver.HttpServer;
import pe.qullaruta.ivv.*;
import pe.qullaruta.simulator.*;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.math.BigDecimal;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.Executors;

/**
 * Servidor HTTP Nativo y Reactivo para Track C (Cero polling, bajo consumo de memoria).
 * Expone contratos REST para IVV y Simulador WFEO conforme a spec.md sección 3.
 */
public final class TrackCHttpServer {

    private final HttpServer server;
    private final ObjectMapper mapper = new ObjectMapper();
    private final IvvModelVersion activeModel;
    private final ImpactParameters activeImpactParams;
    private final NotificationFirewall firewall;

    // Estado en memoria para simulación (aislado de producción)
    private final Map<String, ScenarioState> scenarios = new ConcurrentHashMap<>();

    record ScenarioState(
            String id,
            String name,
            String status,
            List<JsonNode> events,
            WfeoDemoResult lastResult
    ) {}

    public TrackCHttpServer(
            int port,
            IvvModelVersion activeModel,
            ImpactParameters activeImpactParams,
            NotificationFirewall firewall
    ) throws IOException {
        this.activeModel = Objects.requireNonNull(activeModel);
        this.activeImpactParams = Objects.requireNonNull(activeImpactParams);
        this.firewall = Objects.requireNonNull(firewall);

        this.server = HttpServer.create(new InetSocketAddress(port), 0);
        this.server.setExecutor(Executors.newVirtualThreadPerTaskExecutor()); // Virtual Threads (Java 21)

        registerRoutes();
    }

    private void registerRoutes() {
        // Capa 1: IVV y Mapa
        server.createContext("/api/v1/ivv/recalculate", this::handleIvvRecalculate);
        server.createContext("/api/v1/corridors/map", this::handleCorridorsMap);

        // Simulador WFEO
        server.createContext("/api/v1/simulator/scenarios", this::handleSimulatorScenarios);
    }

    private void handleIvvRecalculate(HttpExchange exchange) throws IOException {
        if (!"POST".equalsIgnoreCase(exchange.getRequestMethod())) {
            sendResponse(exchange, 405, Map.of("error", "Method Not Allowed"));
            return;
        }

        JsonNode body = mapper.readTree(exchange.getRequestBody());
        WeatherInput weather = new WeatherInput(
                new BigDecimal(body.path("precip_24h_mm").asText("0.0")),
                new BigDecimal(body.path("precip_72h_mm").asText("0.0"))
        );
        TerrainInput terrain = new TerrainInput(
                new BigDecimal(body.path("max_slope_pct").asText("0.0")),
                body.path("distance_to_ravine_m").asInt(500),
                new BigDecimal(body.path("soil_susceptibility_idx").asText("0.0")),
                body.path("is_historical_hotspot").asBoolean(false)
        );

        BigDecimal score = IvvCalculator.calculate(activeModel, weather, terrain);
        RiskLevel risk = IvvCalculator.classifyRisk(score, activeModel);

        ObjectNode response = mapper.createObjectNode();
        response.put("ivv_score", score.doubleValue());
        response.put("risk_level", risk.name());
        response.put("model_version_tag", activeModel.versionTag());
        response.put("calculated_at", Instant.now().toString());

        sendResponse(exchange, 200, response);
    }

    private void handleCorridorsMap(HttpExchange exchange) throws IOException {
        if (!"GET".equalsIgnoreCase(exchange.getRequestMethod())) {
            sendResponse(exchange, 405, Map.of("error", "Method Not Allowed"));
            return;
        }

        // FeatureCollection GeoJSON estándar del Corredor Central (Huánuco – Carpish – Tingo María)
        ObjectNode featureCollection = mapper.createObjectNode();
        featureCollection.put("type", "FeatureCollection");
        ArrayNode features = featureCollection.putArray("features");

        ObjectNode feature = features.addObject();
        feature.put("type", "Feature");
        feature.put("id", "seg-carpish-tunnel-01");

        ObjectNode properties = feature.putObject("properties");
        properties.put("corridor_name", "Carretera Central - Tramo Túnel Carpish");
        properties.put("ivv_score", 91.27);
        properties.put("risk_level", RiskLevel.RED.name());
        properties.put("model_version_tag", activeModel.versionTag());
        properties.put("is_stale", false);

        ObjectNode geometry = feature.putObject("geometry");
        geometry.put("type", "LineString");
        ArrayNode coordinates = geometry.putArray("coordinates");
        // Coordenadas WGS84 [lon, lat]
        coordinates.addArray().add(-76.109).add(-9.718);
        coordinates.addArray().add(-76.085).add(-9.695);

        sendResponse(exchange, 200, featureCollection);
    }

    private void handleSimulatorScenarios(HttpExchange exchange) throws IOException {
        String path = exchange.getRequestURI().getPath();
        String method = exchange.getRequestMethod();

        // 1. POST /api/v1/simulator/scenarios (Crear escenario)
        if (path.equals("/api/v1/simulator/scenarios") && "POST".equalsIgnoreCase(method)) {
            JsonNode body = mapper.readTree(exchange.getRequestBody());
            String id = UUID.randomUUID().toString();
            String name = body.path("name").asText("Escenario Simulado");
            scenarios.put(id, new ScenarioState(id, name, "DRAFT", new ArrayList<>(), null));

            ObjectNode res = mapper.createObjectNode();
            res.put("scenario_id", id);
            res.put("name", name);
            res.put("status", "DRAFT");
            sendResponse(exchange, 201, res);
            return;
        }

        // Sub-rutas: /api/v1/simulator/scenarios/{id}/...
        String[] parts = path.split("/");
        if (parts.length >= 6) {
            String scenarioId = parts[5];
            ScenarioState state = scenarios.get(scenarioId);
            if (state == null) {
                sendResponse(exchange, 404, Map.of("error", "Scenario not found"));
                return;
            }

            // POST /events
            if (parts.length == 7 && parts[6].equals("events") && "POST".equalsIgnoreCase(method)) {
                JsonNode eventBody = mapper.readTree(exchange.getRequestBody());
                state.events().add(eventBody);
                sendResponse(exchange, 200, Map.of("status", "EVENT_ACCEPTED", "total_events", state.events().size()));
                return;
            }

            // POST /evaluate
            if (parts.length == 7 && parts[6].equals("evaluate") && "POST".equalsIgnoreCase(method)) {
                WfeoDemoRunner runner = new WfeoDemoRunner(activeModel, activeImpactParams, firewall);
                WfeoDemoResult demoResult = runner.runCarpishEmergencyScenario();
                scenarios.put(scenarioId, new ScenarioState(scenarioId, state.name(), "COMPLETED", state.events(), demoResult));

                ObjectNode res = mapper.createObjectNode();
                res.put("scenario_id", scenarioId);
                res.put("status", "COMPLETED");
                res.put("ivv_score", demoResult.ivvScore().doubleValue());
                res.put("risk_level", demoResult.riskLevel().name());
                sendResponse(exchange, 200, res);
                return;
            }

            // GET /impact
            if (parts.length == 7 && parts[6].equals("impact") && "GET".equalsIgnoreCase(method)) {
                if (state.lastResult() == null) {
                    sendResponse(exchange, 400, Map.of("error", "Scenario has not been evaluated yet"));
                    return;
                }
                ImpactDelta delta = state.lastResult().trailerImpact();
                ObjectNode res = mapper.createObjectNode();
                res.put("scenario_id", scenarioId);
                res.put("time_delta_min", delta.timeDeltaMin().doubleValue());
                res.put("net_co2_delta_kg", delta.netCo2DeltaKg().doubleValue());
                res.put("net_agri_savings_usd", delta.netAgriSavingsUsd().doubleValue());
                res.put("is_beneficial_in_time", delta.isBeneficialInTime());
                res.put("is_beneficial_in_emissions", delta.isBeneficialInEmissions());
                sendResponse(exchange, 200, res);
                return;
            }
        }

        sendResponse(exchange, 404, Map.of("error", "Endpoint not found"));
    }

    private void sendResponse(HttpExchange exchange, int statusCode, Object data) throws IOException {
        byte[] bytes = mapper.writeValueAsBytes(data);
        exchange.getResponseHeaders().set("Content-Type", "application/json; charset=UTF-8");
        exchange.sendResponseHeaders(statusCode, bytes.length);
        try (OutputStream os = exchange.getResponseBody()) {
            os.write(bytes);
        }
    }

    public void start() {
        server.start();
    }

    public void stop() {
        server.stop(0);
    }

    public int getPort() {
        return server.getAddress().getPort();
    }

    public static void main(String[] args) throws IOException {
        int port = 8080;
        IvvModelVersion model = new IvvModelVersion(
                "ivv-v1.0-rainy-andes",
                new BigDecimal("0.400"), new BigDecimal("0.300"), new BigDecimal("0.200"), new BigDecimal("0.100"),
                new BigDecimal("80.00"), new BigDecimal("180.00"), new BigDecimal("45.0"), 500,
                new BigDecimal("40.00"), new BigDecimal("70.00"), 3
        );
        ImpactParameters impact = new ImpactParameters(
                "impact-v1.0-huanuco-2026", "HEAVY_TRAILER",
                new BigDecimal("2.500"), new BigDecimal("0.850"),
                new BigDecimal("18000.00"), new BigDecimal("0.0500"), new BigDecimal("0.080")
        );
        NotificationFirewall firewall = new NotificationFirewall(
                (phone, msg) -> System.out.println("[PROD GATEWAY ERROR] " + phone + " -> " + msg),
                simLog -> System.out.println("  [FIREWALL AUDIT] " + simLog)
        );

        TrackCHttpServer server = new TrackCHttpServer(port, model, impact, firewall);
        server.start();

        System.out.println("================================================================================");
        System.out.println(" Servidor HTTP QULLA-RUTA Activo (Track C: Geotecnia & Simulador WFEO)");
        System.out.println(" URL Dashboard Web: http://localhost:" + port);
        System.out.println(" Endpoints REST    : http://localhost:" + port + "/api/v1/corridors/map");
        System.out.println(" Presiona Ctrl+C para detener.");
        System.out.println("================================================================================");
    }
}
