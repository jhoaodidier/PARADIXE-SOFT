package pe.qullaruta.simulator;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import pe.qullaruta.ivv.*;

import java.math.BigDecimal;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * E2E-01: Suite de Demostración Hackathon WFEO.
 * Corredor: Huánuco – Carpish – Tingo María.
 *
 * Flujo:
 * 1. Inyección de lluvia extrema sintética -> IVV Carpish pasa a RED en sandbox.
 * 2. Inyección de derrumbe sintético en Carpish -> Alerta aislada vía NotificationFirewall (cero producción).
 * 3. Evaluación de flota:
 *    - AMBULANCE_4X4: Ruta alterna viable.
 *    - HEAVY_TRAILER: Puente logístico verificado.
 * 4. Cálculo de métricas de impacto socioeconómico y ambiental (Δ ∈ ℝ) en < 2 segundos.
 */
@DisplayName("E2E-01: Simulación Demo WFEO (Huánuco – Carpish – Tingo María)")
class E2eDemoWfeoTest {

    @Test
    @DisplayName("E2E-01: Ejecución completa del escenario de simulación en < 2.0 s y producción intacta")
    void shouldExecuteWfeoDemoScenarioSuccessfully() {
        Instant start = Instant.now();

        // 1. Configuración de Entorno Aislado
        List<String> productionSmsSent = new ArrayList<>();
        List<String> simulationAuditLog = new ArrayList<>();

        ExternalDispatcher productionGateway = (phone, msg) -> productionSmsSent.add(phone + ": " + msg);
        NotificationFirewall firewall = new NotificationFirewall(productionGateway, simulationAuditLog::add);

        IvvModelVersion modelV1 = new IvvModelVersion(
                "ivv-v1.0-rainy-andes",
                new BigDecimal("0.400"), new BigDecimal("0.300"), new BigDecimal("0.200"), new BigDecimal("0.100"),
                new BigDecimal("80.00"), new BigDecimal("180.00"), new BigDecimal("45.0"), 500,
                new BigDecimal("40.00"), new BigDecimal("70.00"), 3
        );

        ImpactParameters heavyTrailerParams = new ImpactParameters(
                "impact-v1.0-huanuco-2026",
                "HEAVY_TRAILER",
                new BigDecimal("2.500"), // ralentí kg CO2/h
                new BigDecimal("0.850"), // ruta kg CO2/km
                new BigDecimal("18000.00"), // Carga de fruta perecible (USD)
                new BigDecimal("0.0500"),  // 5% pérdida por hora
                new BigDecimal("0.080")   // 8% merma por transbordo
        );

        WfeoDemoRunner runner = new WfeoDemoRunner(modelV1, heavyTrailerParams, firewall);

        // 2. Ejecución del Escenario
        WfeoDemoResult result = runner.runCarpishEmergencyScenario();

        // --- ASERCIONES E2E DE ALTA INTEGRIDAD ---

        // Paso 1: Predicción Geotécnica IVV en Carpish pasa a RED
        assertThat(result.ivvScore()).isGreaterThanOrEqualTo(new BigDecimal("70.00"));
        assertThat(result.riskLevel()).isEqualTo(RiskLevel.RED);

        // Paso 2: Notification Firewall contuvo 100% de alertas
        assertThat(productionSmsSent)
                .as("Cero mutaciones ni mensajes enviados a producción")
                .isEmpty();
        assertThat(simulationAuditLog)
                .hasSize(1)
                .first()
                .asString()
                .startsWith("[SIMULATION]");

        // Paso 3: Ruteo Multimodal Diferenciado
        assertThat(result.ambulanceResolution()).isEqualTo("ALTERNATE_ROUTE");
        assertThat(result.heavyTrailerResolution()).isEqualTo("LOGISTIC_BRIDGE_REQUIRED");

        // Paso 4: Métricas de Impacto Reproducibles
        assertThat(result.trailerImpact().isBeneficialInTime()).isTrue();
        assertThat(result.trailerImpact().timeDeltaMin()).isGreaterThan(BigDecimal.ZERO);
        assertThat(result.trailerImpact().netAgriSavingsUsd()).isGreaterThan(BigDecimal.ZERO);

        // Verificación de Rendimiento (< 2000 ms)
        Duration elapsed = Duration.between(start, Instant.now());
        assertThat(elapsed.toMillis()).isLessThan(2000);
    }
}
