package pe.qullaruta.cli;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;

import static org.assertj.core.api.Assertions.assertThat;

@DisplayName("Track C: Interfaz CLI JSON (stdin/stdout)")
class TrackCCliTest {

    private final ObjectMapper mapper = new ObjectMapper();

    @Test
    @DisplayName("CLI: CALCULATE_IVV por stdin retorna JSON con ivv_score y risk_level por stdout")
    void testCliCalculateIvv() throws Exception {
        String inputJson = """
            {
              "command": "CALCULATE_IVV",
              "precip_24h_mm": "40.00",
              "precip_72h_mm": "90.00",
              "max_slope_pct": "22.5",
              "distance_to_ravine_m": 250,
              "soil_susceptibility_idx": "0.50",
              "is_historical_hotspot": true
            }
            """;

        ByteArrayInputStream in = new ByteArrayInputStream(inputJson.getBytes(StandardCharsets.UTF_8));
        ByteArrayOutputStream out = new ByteArrayOutputStream();

        TrackCCli cli = new TrackCCli();
        cli.process(in, new PrintStream(out));

        JsonNode res = mapper.readTree(out.toByteArray());
        assertThat(res.path("status").asText()).isEqualTo("SUCCESS");
        assertThat(res.path("ivv_score").asDouble()).isEqualTo(55.00);
        assertThat(res.path("risk_level").asText()).isEqualTo("AMBER");
    }

    @Test
    @DisplayName("CLI: RUN_WFEO_DEMO ejecuta la simulación completa y reporta métricas de impacto")
    void testCliRunDemo() throws Exception {
        String inputJson = "{\"command\": \"RUN_WFEO_DEMO\"}";

        ByteArrayInputStream in = new ByteArrayInputStream(inputJson.getBytes(StandardCharsets.UTF_8));
        ByteArrayOutputStream out = new ByteArrayOutputStream();

        TrackCCli cli = new TrackCCli();
        cli.process(in, new PrintStream(out));

        JsonNode res = mapper.readTree(out.toByteArray());
        assertThat(res.path("status").asText()).isEqualTo("SUCCESS");
        assertThat(res.path("risk_level").asText()).isEqualTo("RED");
        assertThat(res.path("time_saved_min").asDouble()).isGreaterThan(0.0);
    }
}
