package pe.qullaruta.ivv;

import java.math.BigDecimal;
import java.util.Objects;

public record WeatherInput(
        BigDecimal precip24hMm,
        BigDecimal precip72hMm
) {
    public WeatherInput {
        Objects.requireNonNull(precip24hMm, "precip24hMm cannot be null");
        Objects.requireNonNull(precip72hMm, "precip72hMm cannot be null");

        if (precip24hMm.compareTo(BigDecimal.ZERO) < 0) {
            throw new IllegalArgumentException("precip24hMm cannot be negative");
        }
        if (precip72hMm.compareTo(precip24hMm) < 0) {
            throw new IllegalArgumentException("precip_72h_mm cannot be less than precip_24h_mm");
        }
    }
}
