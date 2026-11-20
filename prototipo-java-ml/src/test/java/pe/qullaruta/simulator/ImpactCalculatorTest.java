package pe.qullaruta.simulator;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.math.BigDecimal;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * Suite de Prueba para SIM-02: Métricas de Impacto con Signo Libre (Δ ∈ ℝ).
 * Garantiza que aumentos de tiempo o emisiones en desvíos largos no sean truncados a cero.
 */
@DisplayName("SIM-02: Métricas de Impacto con Signo Libre")
class ImpactCalculatorTest {

    private final ImpactParameters params = new ImpactParameters(
            "impact-v1.0-huanuco-2026",
            "HEAVY_TRAILER",
            new BigDecimal("2.500"), // co2_idle_kg_per_hour
            new BigDecimal("0.850"), // co2_drive_kg_per_km
            new BigDecimal("15000.00"), // agri_cargo_value_usd
            new BigDecimal("0.0500"), // perishability_loss_rate_per_hour (5% por hora tras espera)
            new BigDecimal("0.080")   // transfer_handling_loss_pct (8% merma por transbordo)
    );

    @Test
    @DisplayName("SIM-02: Desvío de 120 km frente a espera de 20 min genera delta de CO2 y tiempo negativos")
    void shouldProduceNegativeDeltasWhenDetourIsMoreCostlyThanWait() {
        // Escenario base: Espera de 20 min (0.333 h) en ralentí, 0 km de tránsito adicional
        RunMetric baseline = new RunMetric(
                new BigDecimal("20.00"), // 20 min
                new BigDecimal("0.00"),  // 0 km adicionales
                new BigDecimal("0.333")  // 0.333 h ralentí -> CO2 = 0.333 * 2.500 = 0.8325 kg
        );

        // Escenario mitigado: Desvío de 120 km que toma 195 min (3.25 h), 0 h ralentí
        RunMetric mitigated = new RunMetric(
                new BigDecimal("195.00"), // 195 min
                new BigDecimal("120.00"), // 120 km -> CO2 = 120 * 0.850 = 102.00 kg
                BigDecimal.ZERO
        );

        ImpactDelta delta = ImpactCalculator.calculateDelta(params, baseline, mitigated, false);

        // Delta Tiempo = 20 - 195 = -175 min (costo de tiempo adicional real)
        assertThat(delta.timeDeltaMin())
                .isEqualByComparingTo(new BigDecimal("-175.00"));

        // Delta CO2 = 0.83 - 102.00 < 0 (emisión adicional explícita, nunca truncada a cero)
        assertThat(delta.netCo2DeltaKg())
                .isLessThan(BigDecimal.ZERO);

        assertThat(delta.isBeneficialInTime()).isFalse();
        assertThat(delta.isBeneficialInEmissions()).isFalse();
    }

    @Test
    @DisplayName("Escenario favorable: Desvío que evita bloqueo de 12 horas genera deltas positivos de ahorro")
    void shouldProducePositiveDeltasWhenDetourAvoidsProlongedWait() {
        // Escenario base: Bloqueo de 12 horas (720 min) en ralentí
        RunMetric baseline = new RunMetric(
                new BigDecimal("720.00"),
                BigDecimal.ZERO,
                new BigDecimal("12.00")
        );

        // Escenario mitigado: Desvío que toma 180 min y 100 km
        RunMetric mitigated = new RunMetric(
                new BigDecimal("180.00"),
                new BigDecimal("100.00"),
                BigDecimal.ZERO
        );

        ImpactDelta delta = ImpactCalculator.calculateDelta(params, baseline, mitigated, false);

        // Delta Tiempo = 720 - 180 = +540 min de ahorro
        assertThat(delta.timeDeltaMin()).isGreaterThan(BigDecimal.ZERO);
        assertThat(delta.isBeneficialInTime()).isTrue();
    }
}
