package pe.qullaruta.ivv;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Nested;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;

import java.math.BigDecimal;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

/**
 * Suite de Pruebas de Alta Integridad (Inspirada en DO-178C / Sistemas Vitales).
 * Cobertura de: IVV-01, Análisis de Valores Límite (BVA), Invariantes y No-Deriva Aritmética.
 */
@DisplayName("Motor Geotécnico IVV (DO-178C Critical Integrity Tests)")
class IvvCalculatorTest {

    // Modelo de referencia v1 estándar (según spec.md sección 2.3)
    private final IvvModelVersion modelV1 = new IvvModelVersion(
            "ivv-v1.0-rainy-andes",
            new BigDecimal("0.400"), // w_precip
            new BigDecimal("0.300"), // w_slope_ravine
            new BigDecimal("0.200"), // w_soil
            new BigDecimal("0.100"), // w_hotspot (Suma = 1.000)
            new BigDecimal("80.00"), // precip_24h_cap_mm
            new BigDecimal("180.00"),// precip_72h_cap_mm
            new BigDecimal("45.0"),  // slope_cap_pct
            500,                     // ravine_max_dist_m
            new BigDecimal("40.00"), // amber_threshold
            new BigDecimal("70.00"), // red_threshold
            3                        // validity_hours
    );

    @Nested
    @DisplayName("1. Análisis de Valores Límite Exactos (BVA) y Clasificación de Riesgo")
    class BoundaryValueAnalysisTests {

        @ParameterizedTest(name = "IVV {0} debe clasificarse como {1}")
        @CsvSource({
                "0.00,  GREEN",
                "39.99, GREEN",
                "40.00, AMBER",
                "69.99, AMBER",
                "70.00, RED",
                "100.00, RED"
        })
        @DisplayName("Transiciones de estado en bordes críticos exactos (39.99/40.00 y 69.99/70.00)")
        void shouldClassifyRiskLevelAtExactBoundaries(String scoreStr, RiskLevel expectedLevel) {
            BigDecimal score = new BigDecimal(scoreStr);
            RiskLevel level = IvvCalculator.classifyRisk(score, modelV1);
            assertThat(level).isEqualTo(expectedLevel);
        }

        @Test
        @DisplayName("Saturación superior: Lluvias y pendientes extremas deben truncar a 1.0 sin desbordamiento")
        void shouldClampAtSaturationCapsWithoutOverflow() {
            // Lluvias de 500 mm (cap 80 y 180), pendiente 90% (cap 45%), distancia 0 m (máxima cercanía a quebrada)
            // suelo 1.0, hotspot histórico true (1.0)
            WeatherInput extremeWeather = new WeatherInput(new BigDecimal("500.00"), new BigDecimal("1000.00"));
            TerrainInput extremeTerrain = new TerrainInput(new BigDecimal("90.0"), 0, new BigDecimal("1.00"), true);

            BigDecimal score = IvvCalculator.calculate(modelV1, extremeWeather, extremeTerrain);

            // Todos los factores al 100% -> IVV debe ser exactamente 100.00
            assertThat(score).isEqualByComparingTo(new BigDecimal("100.00"));
            assertThat(IvvCalculator.classifyRisk(score, modelV1)).isEqualTo(RiskLevel.RED);
        }

        @Test
        @DisplayName("Cero absoluto: Lluvia cero, pendiente cero, lejos de quebrada, suelo nulo, sin histórico")
        void shouldHandleAbsoluteZeroInputs() {
            WeatherInput zeroWeather = new WeatherInput(BigDecimal.ZERO, BigDecimal.ZERO);
            TerrainInput zeroTerrain = new TerrainInput(BigDecimal.ZERO, 1000, BigDecimal.ZERO, false);

            // P = 0, S = 0, G = 0, H = 0.20 -> 100 * (0.100 * 0.20) = 2.00
            BigDecimal score = IvvCalculator.calculate(modelV1, zeroWeather, zeroTerrain);

            assertThat(score).isEqualByComparingTo(new BigDecimal("2.00"));
            assertThat(IvvCalculator.classifyRisk(score, modelV1)).isEqualTo(RiskLevel.GREEN);
        }
    }

    @Nested
    @DisplayName("2. Reproducibilidad Bit a Bit (IVV-01) y No-Deriva Aritmética")
    class DeterminismAndReproducibilityTests {

        @Test
        @DisplayName("IVV-01: Cálculo exacto al centésimo con modelo v1 reproducible")
        void shouldReproduceHistoricalScoreDeterministically() {
            // Insumos típicos de Carpish en época de lluvias
            WeatherInput weather = new WeatherInput(new BigDecimal("40.00"), new BigDecimal("90.00"));
            TerrainInput terrain = new TerrainInput(new BigDecimal("22.5"), 250, new BigDecimal("0.50"), true);

            // P24 = 40/80 = 0.5; P72 = 90/180 = 0.5 -> P = 0.5
            // S_slope = 22.5/45 = 0.5; S_ravine = 1 - 250/500 = 0.5 -> S = 0.5
            // G = 0.5; H = 1.0 (hotspot)
            // IVV = 100 * (0.400*0.5 + 0.300*0.5 + 0.200*0.5 + 0.100*1.0)
            //     = 100 * (0.20 + 0.15 + 0.10 + 0.10) = 100 * 0.55 = 55.00
            BigDecimal score = IvvCalculator.calculate(modelV1, weather, terrain);

            assertThat(score).isEqualByComparingTo(new BigDecimal("55.00"));
            assertThat(IvvCalculator.classifyRisk(score, modelV1)).isEqualTo(RiskLevel.AMBER);
        }
    }

    @Nested
    @DisplayName("3. Invariantes de Seguridad y Protección de Entrada (Fail-Fast)")
    class InvariantSafetyTests {

        @Test
        @DisplayName("Invariante: La suma de pesos del modelo debe ser estrictamente 1.000")
        void shouldRejectModelWithInvalidWeightsSum() {
            assertThatThrownBy(() -> new IvvModelVersion(
                    "invalid-weights",
                    new BigDecimal("0.500"),
                    new BigDecimal("0.500"),
                    new BigDecimal("0.200"), // Suma = 1.300 != 1.000
                    new BigDecimal("0.100"),
                    new BigDecimal("80.00"),
                    new BigDecimal("180.00"),
                    new BigDecimal("45.0"),
                    500,
                    new BigDecimal("40.00"),
                    new BigDecimal("70.00"),
                    3
            )).isInstanceOf(IllegalArgumentException.class)
              .hasMessageContaining("Weights must sum exactly to 1.000");
        }

        @Test
        @DisplayName("Invariante: Umbral RED debe ser estrictamente mayor a AMBER")
        void shouldRejectModelWithInvertedThresholds() {
            assertThatThrownBy(() -> new IvvModelVersion(
                    "inverted-thresholds",
                    new BigDecimal("0.400"), new BigDecimal("0.300"), new BigDecimal("0.200"), new BigDecimal("0.100"),
                    new BigDecimal("80.00"), new BigDecimal("180.00"), new BigDecimal("45.0"), 500,
                    new BigDecimal("70.00"), // amber
                    new BigDecimal("40.00"), // red < amber (violación)
                    3
            )).isInstanceOf(IllegalArgumentException.class)
              .hasMessageContaining("red_threshold must be strictly greater than amber_threshold");
        }

        @Test
        @DisplayName("Invariante física: Lluvia 72h no puede ser inferior a lluvia 24h")
        void shouldRejectPhysicalImpossibilityOfPrecipitation() {
            assertThatThrownBy(() -> new WeatherInput(new BigDecimal("60.00"), new BigDecimal("40.00")))
                    .isInstanceOf(IllegalArgumentException.class)
                    .hasMessageContaining("precip_72h_mm cannot be less than precip_24h_mm");
        }
    }
}
