package pe.qullaruta.simulator;

import java.math.BigDecimal;

public record ImpactDelta(
        BigDecimal timeDeltaMin,
        BigDecimal netCo2DeltaKg,
        BigDecimal netAgriSavingsUsd,
        boolean isBeneficialInTime,
        boolean isBeneficialInEmissions
) {}
