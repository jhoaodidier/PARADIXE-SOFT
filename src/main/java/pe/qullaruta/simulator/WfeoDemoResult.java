package pe.qullaruta.simulator;

import pe.qullaruta.ivv.RiskLevel;

import java.math.BigDecimal;

public record WfeoDemoResult(
        String scenarioId,
        String scenarioName,
        BigDecimal ivvScore,
        RiskLevel riskLevel,
        String ambulanceResolution,
        String heavyTrailerResolution,
        ImpactDelta trailerImpact,
        long executionTimeMs
) {}
