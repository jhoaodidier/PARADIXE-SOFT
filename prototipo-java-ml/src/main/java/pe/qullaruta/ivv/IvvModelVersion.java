package pe.qullaruta.ivv;

import java.math.BigDecimal;
import java.util.Objects;

public record IvvModelVersion(
        String versionTag,
        BigDecimal wPrecip,
        BigDecimal wSlopeRavine,
        BigDecimal wSoil,
        BigDecimal wHotspot,
        BigDecimal precip24hCapMm,
        BigDecimal precip72hCapMm,
        BigDecimal slopeCapPct,
        int ravineMaxDistM,
        BigDecimal amberThreshold,
        BigDecimal redThreshold,
        int validityHours
) {
    public IvvModelVersion {
        Objects.requireNonNull(versionTag, "versionTag cannot be null");
        Objects.requireNonNull(wPrecip, "wPrecip cannot be null");
        Objects.requireNonNull(wSlopeRavine, "wSlopeRavine cannot be null");
        Objects.requireNonNull(wSoil, "wSoil cannot be null");
        Objects.requireNonNull(wHotspot, "wHotspot cannot be null");

        BigDecimal sum = wPrecip.add(wSlopeRavine).add(wSoil).add(wHotspot);
        if (sum.compareTo(BigDecimal.ONE) != 0) {
            throw new IllegalArgumentException("Weights must sum exactly to 1.000, current sum: " + sum);
        }

        if (redThreshold.compareTo(amberThreshold) <= 0) {
            throw new IllegalArgumentException("red_threshold must be strictly greater than amber_threshold");
        }
    }
}
