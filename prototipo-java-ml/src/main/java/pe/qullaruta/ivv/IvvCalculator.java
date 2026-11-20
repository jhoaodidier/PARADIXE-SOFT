package pe.qullaruta.ivv;

import java.math.BigDecimal;
import java.math.RoundingMode;

/**
 * Motor Geotécnico IVV: Cálculo reproducible y determinista sin deriva de punto flotante.
 */
public final class IvvCalculator {

    private IvvCalculator() {}

    private static double clamp(double value, double min, double max) {
        return Math.min(max, Math.max(min, value));
    }

    public static BigDecimal calculate(IvvModelVersion model, WeatherInput weather, TerrainInput terrain) {
        // 1. Precipitación (P)
        double p24 = clamp(weather.precip24hMm().doubleValue() / model.precip24hCapMm().doubleValue(), 0.0, 1.0);
        double p72 = clamp(weather.precip72hMm().doubleValue() / model.precip72hCapMm().doubleValue(), 0.0, 1.0);
        double p = 0.6 * p24 + 0.4 * p72;

        // 2. Topografía y Quebrada (S)
        double sSlope = clamp(terrain.maxSlopePct().doubleValue() / model.slopeCapPct().doubleValue(), 0.0, 1.0);
        double sRavine = 1.0 - clamp((double) terrain.distanceToRavineM() / model.ravineMaxDistM(), 0.0, 1.0);
        double s = 0.5 * sSlope + 0.5 * sRavine;

        // 3. Suelo (G) e Histórico (H)
        double g = clamp(terrain.soilSusceptibilityIdx().doubleValue(), 0.0, 1.0);
        double h = terrain.isHistoricalHotspot() ? 1.0 : 0.2;

        // 4. Ponderación
        double w1 = model.wPrecip().doubleValue();
        double w2 = model.wSlopeRavine().doubleValue();
        double w3 = model.wSoil().doubleValue();
        double w4 = model.wHotspot().doubleValue();

        double rawScore = 100.0 * (w1 * p + w2 * s + w3 * g + w4 * h);

        return BigDecimal.valueOf(rawScore).setScale(2, RoundingMode.HALF_UP);
    }

    public static RiskLevel classifyRisk(BigDecimal score, IvvModelVersion model) {
        if (score.compareTo(model.amberThreshold()) < 0) {
            return RiskLevel.GREEN;
        } else if (score.compareTo(model.redThreshold()) < 0) {
            return RiskLevel.AMBER;
        } else {
            return RiskLevel.RED;
        }
    }
}
