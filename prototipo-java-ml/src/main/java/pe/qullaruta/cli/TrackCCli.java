package pe.qullaruta.cli;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import pe.qullaruta.ivv.*;
import pe.qullaruta.simulator.*;

import java.io.InputStream;
import java.io.PrintStream;
import java.math.BigDecimal;
import java.util.ArrayList;
import java.util.List;

/**
 * Interfaz CLI JSON pura conforme al Artículo II del SDD (stdin -> stdout).
 * Permite invocar el motor IVV y la simulación desde cualquier lenguaje, script o subproceso.
 */
public final class TrackCCli {

    private final ObjectMapper mapper = new ObjectMapper();
    private final IvvModelVersion defaultModel = new IvvModelVersion(
            "ivv-v1.0-rainy-andes",
            new BigDecimal("0.400"), new BigDecimal("0.300"), new BigDecimal("0.200"), new BigDecimal("0.100"),
            new BigDecimal("80.00"), new BigDecimal("180.00"), new BigDecimal("45.0"), 500,
            new BigDecimal("40.00"), new BigDecimal("70.00"), 3
    );
    private final ImpactParameters defaultImpact = new ImpactParameters(
            "impact-v1.0-huanuco-2026", "HEAVY_TRAILER",
            new BigDecimal("2.500"), new BigDecimal("0.850"),
            new BigDecimal("18000.00"), new BigDecimal("0.0500"), new BigDecimal("0.080")
    );

    public void process(InputStream in, PrintStream out) {
        try {
            JsonNode root = mapper.readTree(in);
            if (root == null || root.isEmpty()) {
                out.println("{\"error\": \"Empty JSON input\"}");
                return;
            }

            String command = root.path("command").asText("CALCULATE_IVV");
            ObjectNode response = mapper.createObjectNode();

            switch (command) {
                case "CALCULATE_IVV" -> {
                    WeatherInput weather = new WeatherInput(
                            new BigDecimal(root.path("precip_24h_mm").asText("0.0")),
                            new BigDecimal(root.path("precip_72h_mm").asText("0.0"))
                    );
                    TerrainInput terrain = new TerrainInput(
                            new BigDecimal(root.path("max_slope_pct").asText("0.0")),
                            root.path("distance_to_ravine_m").asInt(500),
                            new BigDecimal(root.path("soil_susceptibility_idx").asText("0.0")),
                            root.path("is_historical_hotspot").asBoolean(false)
                    );
                    BigDecimal score = IvvCalculator.calculate(defaultModel, weather, terrain);
                    RiskLevel risk = IvvCalculator.classifyRisk(score, defaultModel);

                    response.put("status", "SUCCESS");
                    response.put("ivv_score", score.doubleValue());
                    response.put("risk_level", risk.name());
                    response.put("model_version", defaultModel.versionTag());
                }
                case "RUN_WFEO_DEMO" -> {
                    List<String> auditLogs = new ArrayList<>();
                    NotificationFirewall firewall = new NotificationFirewall((p, m) -> {}, auditLogs::add);
                    WfeoDemoRunner runner = new WfeoDemoRunner(defaultModel, defaultImpact, firewall);
                    WfeoDemoResult res = runner.runCarpishEmergencyScenario();

                    response.put("status", "SUCCESS");
                    response.put("scenario_id", res.scenarioId());
                    response.put("ivv_score", res.ivvScore().doubleValue());
                    response.put("risk_level", res.riskLevel().name());
                    response.put("ambulance_route", res.ambulanceResolution());
                    response.put("heavy_trailer_route", res.heavyTrailerResolution());
                    response.put("time_saved_min", res.trailerImpact().timeDeltaMin().doubleValue());
                    response.put("co2_saved_kg", res.trailerImpact().netCo2DeltaKg().doubleValue());
                    response.put("agri_saved_usd", res.trailerImpact().netAgriSavingsUsd().doubleValue());
                    response.put("execution_time_ms", res.executionTimeMs());
                }
                default -> {
                    response.put("status", "ERROR");
                    response.put("message", "Unknown command: " + command);
                }
            }

            out.println(mapper.writeValueAsString(response));
        } catch (Exception e) {
            try {
                ObjectNode err = mapper.createObjectNode();
                err.put("status", "ERROR");
                err.put("message", e.getMessage());
                out.println(mapper.writeValueAsString(err));
            } catch (Exception ignored) {}
        }
    }

    public static void main(String[] args) {
        new TrackCCli().process(System.in, System.out);
    }
}
