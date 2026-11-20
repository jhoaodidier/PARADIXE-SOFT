package pe.qullaruta.simulator;

import java.math.BigDecimal;

public record RunMetric(
        BigDecimal travelTimeMin,
        BigDecimal travelDistanceKm,
        BigDecimal idleTimeHours
) {}
