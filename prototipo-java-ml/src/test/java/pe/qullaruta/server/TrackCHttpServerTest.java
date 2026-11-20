package pe.qullaruta.server;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import pe.qullaruta.ivv.IvvModelVersion;
import pe.qullaruta.simulator.ImpactParameters;
import pe.qullaruta.simulator.NotificationFirewall;

import java.io.IOException;
import java.math.BigDecimal;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.util.ArrayList;
import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;

@DisplayName("Track C: Integración de Endpoints HTTP REST (/api/v1)")
class TrackCHttpServerTest {

    private static TrackCHttpServer server;
    private static int port;
    private static HttpClient client;
    private static final ObjectMapper mapper = new ObjectMapper();

    @BeforeAll
    static void startServer() throws IOException {
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
        List<String> simLogs = new ArrayList<>();
        NotificationFirewall firewall = new NotificationFirewall((p, m) -> {}, simLogs::add);

        // Puerto 0 para asignación efímera libre sin conflictos
        server = new TrackCHttpServer(0, model, impact, firewall);
        server.start();
        port = server.getPort();
        client = HttpClient.newHttpClient();
    }

    @AfterAll
    static void stopServer() {
        if (server != null) {
            server.stop();
        }
    }

    private String url(String path) {
        return "http://localhost:" + port + path;
    }

    @Test
    @DisplayName("POST /api/v1/ivv/recalculate retorna cálculo determinista y clasificación")
    void testRecalculateIvvEndpoint() throws Exception {
        String jsonRequest = """
            {
              "precip_24h_mm": "40.00",
              "precip_72h_mm": "90.00",
              "max_slope_pct": "22.5",
              "distance_to_ravine_m": 250,
              "soil_susceptibility_idx": "0.50",
              "is_historical_hotspot": true
            }
            """;

        HttpRequest req = HttpRequest.newBuilder()
                .uri(URI.create(url("/api/v1/ivv/recalculate")))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString(jsonRequest))
                .build();

        HttpResponse<String> res = client.send(req, HttpResponse.BodyHandlers.ofString());

        assertThat(res.statusCode()).isEqualTo(200);
        JsonNode root = mapper.readTree(res.body());
        assertThat(root.path("ivv_score").asDouble()).isEqualTo(55.00);
        assertThat(root.path("risk_level").asText()).isEqualTo("AMBER");
        assertThat(root.path("model_version_tag").asText()).isEqualTo("ivv-v1.0-rainy-andes");
    }

    @Test
    @DisplayName("GET /api/v1/corridors/map retorna FeatureCollection GeoJSON válida")
    void testCorridorsMapEndpoint() throws Exception {
        HttpRequest req = HttpRequest.newBuilder()
                .uri(URI.create(url("/api/v1/corridors/map")))
                .GET()
                .build();

        HttpResponse<String> res = client.send(req, HttpResponse.BodyHandlers.ofString());

        assertThat(res.statusCode()).isEqualTo(200);
        JsonNode root = mapper.readTree(res.body());
        assertThat(root.path("type").asText()).isEqualTo("FeatureCollection");
        assertThat(root.path("features")).isNotEmpty();

        JsonNode firstFeature = root.path("features").get(0);
        assertThat(firstFeature.path("geometry").path("type").asText()).isEqualTo("LineString");
        assertThat(firstFeature.path("properties").path("risk_level").asText()).isEqualTo("RED");
    }

    @Test
    @DisplayName("Flujo completo del Simulador: Crear escenario -> Agregar evento -> Evaluar -> Obtener impacto")
    void testSimulatorFlow() throws Exception {
        // 1. Crear Escenario
        HttpRequest createReq = HttpRequest.newBuilder()
                .uri(URI.create(url("/api/v1/simulator/scenarios")))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString("{\"name\": \"Demo Huánuco - Tingo María\"}"))
                .build();
        HttpResponse<String> createRes = client.send(createReq, HttpResponse.BodyHandlers.ofString());
        assertThat(createRes.statusCode()).isEqualTo(201);
        String scenarioId = mapper.readTree(createRes.body()).path("scenario_id").asText();

        // 2. Inyectar Evento Sintético
        HttpRequest eventReq = HttpRequest.newBuilder()
                .uri(URI.create(url("/api/v1/simulator/scenarios/" + scenarioId + "/events")))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.ofString("{\"event_type\": \"BLOCKAGE_OVERRIDE\"}"))
                .build();
        HttpResponse<String> eventRes = client.send(eventReq, HttpResponse.BodyHandlers.ofString());
        assertThat(eventRes.statusCode()).isEqualTo(200);

        // 3. Evaluar
        HttpRequest evalReq = HttpRequest.newBuilder()
                .uri(URI.create(url("/api/v1/simulator/scenarios/" + scenarioId + "/evaluate")))
                .header("Content-Type", "application/json")
                .POST(HttpRequest.BodyPublishers.noBody())
                .build();
        HttpResponse<String> evalRes = client.send(evalReq, HttpResponse.BodyHandlers.ofString());
        assertThat(evalRes.statusCode()).isEqualTo(200);
        JsonNode evalJson = mapper.readTree(evalRes.body());
        assertThat(evalJson.path("status").asText()).isEqualTo("COMPLETED");

        // 4. Consultar Impacto
        HttpRequest impactReq = HttpRequest.newBuilder()
                .uri(URI.create(url("/api/v1/simulator/scenarios/" + scenarioId + "/impact")))
                .GET()
                .build();
        HttpResponse<String> impactRes = client.send(impactReq, HttpResponse.BodyHandlers.ofString());
        assertThat(impactRes.statusCode()).isEqualTo(200);
        JsonNode impactJson = mapper.readTree(impactRes.body());
        assertThat(impactJson.path("time_delta_min").asDouble()).isGreaterThan(0.0);
        assertThat(impactJson.path("net_agri_savings_usd").asDouble()).isGreaterThan(0.0);
    }
}
