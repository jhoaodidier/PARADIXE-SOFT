package pe.qullaruta.simulator;

import java.math.BigDecimal;
import java.math.RoundingMode;

/**
 * Motor de Cálculo de Impacto con Signo Libre (Δ ∈ ℝ).
 * Garantiza que deltas negativos (sobrecostos de tiempo o emisiones) se preserven fielmente.
 */
public final class ImpactCalculator {

    private ImpactCalculator() {}

    public static ImpactDelta calculateDelta(
            ImpactParameters params,
            RunMetric baseline,
            RunMetric mitigated,
            boolean isLogisticBridge
    ) {
        // ΔT = T_base - T_mit
        BigDecimal timeDelta = baseline.travelTimeMin().subtract(mitigated.travelTimeMin())
                .setScale(2, RoundingMode.HALF_UP);

        // CO2_base = (idle_hours * factor_idle) + (km * factor_km)
        BigDecimal co2Base = baseline.idleTimeHours().multiply(params.co2IdleKgPerHour())
                .add(baseline.travelDistanceKm().multiply(params.co2DriveKgPerKm()));

        // CO2_mit = (idle_hours * factor_idle) + (km * factor_km)
        BigDecimal co2Mit = mitigated.idleTimeHours().multiply(params.co2IdleKgPerHour())
                .add(mitigated.travelDistanceKm().multiply(params.co2DriveKgPerKm()));

        // ΔCO2 = CO2_base - CO2_mit (puede ser negativo si el desvío añade emisiones)
        BigDecimal co2Delta = co2Base.subtract(co2Mit).setScale(2, RoundingMode.HALF_UP);

        // Pérdidas Agrícolas
        BigDecimal baseLossRate = baseline.idleTimeHours().multiply(params.perishabilityLossRatePerHour());
        BigDecimal mitLossRate = mitigated.idleTimeHours().multiply(params.perishabilityLossRatePerHour());
        if (isLogisticBridge) {
            mitLossRate = mitLossRate.add(params.transferHandlingLossPct());
        }

        BigDecimal lossBase = params.agriCargoValueUsd().multiply(baseLossRate);
        BigDecimal lossMit = params.agriCargoValueUsd().multiply(mitLossRate);
        BigDecimal agriSavings = lossBase.subtract(lossMit).setScale(2, RoundingMode.HALF_UP);

        boolean beneficialInTime = timeDelta.compareTo(BigDecimal.ZERO) > 0;
        boolean beneficialInEmissions = co2Delta.compareTo(BigDecimal.ZERO) > 0;

        return new ImpactDelta(timeDelta, co2Delta, agriSavings, beneficialInTime, beneficialInEmissions);
    }
}
