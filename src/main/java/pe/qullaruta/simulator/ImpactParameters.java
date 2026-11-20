package pe.qullaruta.simulator;

import java.math.BigDecimal;

public record ImpactParameters(
        String versionTag,
        String vehicleCategoryCode,
        BigDecimal co2IdleKgPerHour,
        BigDecimal co2DriveKgPerKm,
        BigDecimal agriCargoValueUsd,
        BigDecimal perishabilityLossRatePerHour,
        BigDecimal transferHandlingLossPct
) {}
