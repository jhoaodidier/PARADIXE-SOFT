package pe.qullaruta.ivv;

import java.math.BigDecimal;
import java.util.Objects;

public record TerrainInput(
        BigDecimal maxSlopePct,
        int distanceToRavineM,
        BigDecimal soilSusceptibilityIdx,
        boolean isHistoricalHotspot
) {
    public TerrainInput {
        Objects.requireNonNull(maxSlopePct, "maxSlopePct cannot be null");
        Objects.requireNonNull(soilSusceptibilityIdx, "soilSusceptibilityIdx cannot be null");

        if (maxSlopePct.compareTo(BigDecimal.ZERO) < 0) {
            throw new IllegalArgumentException("maxSlopePct cannot be negative");
        }
        if (distanceToRavineM < 0) {
            throw new IllegalArgumentException("distanceToRavineM cannot be negative");
        }
    }
}
