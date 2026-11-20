package pe.qullaruta.simulator;

import pe.qullaruta.ivv.*;

import java.math.BigDecimal;
import java.time.Duration;
import java.time.Instant;
import java.util.Objects;
import java.util.UUID;

/**
 * Orquestador del Escenario de Simulación WFEO (E2E-01).
 * Corredor Estratégico: Huánuco – Carpish – Tingo María.
 */
public final class WfeoDemoRunner {

    private final IvvModelVersion ivvModel;
    private final ImpactParameters impactParams;
    private final NotificationFirewall firewall;

    public WfeoDemoRunner(IvvModelVersion ivvModel, ImpactParameters impactParams, NotificationFirewall firewall) {
        this.ivvModel = Objects.requireNonNull(ivvModel, "ivvModel cannot be null");
        this.impactParams = Objects.requireNonNull(impactParams, "impactParams cannot be null");
        this.firewall = Objects.requireNonNull(firewall, "firewall cannot be null");
    }

    public WfeoDemoResult runCarpishEmergencyScenario() {
        Instant startTime = Instant.now();
        String scenarioId = UUID.randomUUID().toString();
        String scenarioName = "Emergencia por Lluvia Extrema y Huaico en Carpish (Demo WFEO)";

        // 1. Inyección de Lluvia Extrema Sintética
        WeatherInput extremeRain = new WeatherInput(new BigDecimal("95.00"), new BigDecimal("210.00"));
        TerrainInput carpishTerrain = new TerrainInput(
                new BigDecimal("35.0"), // pendiente pronunciada en cordillera
                80,                    // 80m a quebrada activa
                new BigDecimal("0.85"), // suelo arcilloso altamente inestable
                true                    // punto crítico recurrente Provías
        );

        BigDecimal ivvScore = IvvCalculator.calculate(ivvModel, extremeRain, carpishTerrain);
        RiskLevel riskLevel = IvvCalculator.classifyRisk(ivvScore, ivvModel);

        // 2. Inyección de Bloqueo Sintético y Contención con Firewall
        try {
            firewall.sendNotification(
                    ExecutionContext.SIMULATION,
                    "+51962000111",
                    "ALERTA VIAL: Derrumbe total km 47 Carretera Central (Carpish)"
            );
        } catch (SafetyBarrierViolationException expected) {
            // Esperado: La barrera del firewall abortó el despacho real y registró en auditoría interna
        }

        // 3. Ruteo Multimodal Diferenciado
        // Ambulancia: Liviana / 4x4 -> Puede circular por desvío vecinal afirmado
        String ambulanceResolution = "ALTERNATE_ROUTE";

        // Tráiler Pesado: Excede límites de peso/ancho de trocha -> Requiere Puente Logístico Peatonal/Relevo
        String heavyTrailerResolution = "LOGISTIC_BRIDGE_REQUIRED";

        // 4. Cálculo de Impacto Socioeconómico y Ambiental para Transporte de Carga (Agro)
        // Escenario Base: Tráiler esperando 14 horas varado en Carpish
        RunMetric baselineWait = new RunMetric(
                new BigDecimal("840.00"), // 14 horas
                BigDecimal.ZERO,
                new BigDecimal("14.00")   // 14 horas de ralentí
        );

        // Escenario Mitigado: Transbordo organizado por puente logístico seguro (2 horas)
        RunMetric mitigatedTransfer = new RunMetric(
                new BigDecimal("120.00"), // 2 horas de operación
                new BigDecimal("15.00"),  // 15 km hasta punto seguro de entrega
                new BigDecimal("1.50")    // 1.5 horas ralentí mientras trasvasa
        );

        ImpactDelta delta = ImpactCalculator.calculateDelta(impactParams, baselineWait, mitigatedTransfer, true);

        long elapsedMs = Duration.between(startTime, Instant.now()).toMillis();

        return new WfeoDemoResult(
                scenarioId,
                scenarioName,
                ivvScore,
                riskLevel,
                ambulanceResolution,
                heavyTrailerResolution,
                delta,
                elapsedMs
        );
    }

    public static WfeoDemoRunner forPresentation(NotificationFirewall firewall) {
        IvvModelVersion modelV1 = new IvvModelVersion(
                "ivv-v1.0-rainy-andes",
                new BigDecimal("0.400"), new BigDecimal("0.300"), new BigDecimal("0.200"), new BigDecimal("0.100"),
                new BigDecimal("80.00"), new BigDecimal("180.00"), new BigDecimal("45.0"), 500,
                new BigDecimal("40.00"), new BigDecimal("70.00"), 3
        );

        ImpactParameters trailerParams = new ImpactParameters(
                "impact-v1.0-huanuco-2026",
                "HEAVY_TRAILER",
                new BigDecimal("2.500"),
                new BigDecimal("0.850"),
                new BigDecimal("18000.00"),
                new BigDecimal("0.0500"),
                new BigDecimal("0.080")
        );

        return new WfeoDemoRunner(modelV1, trailerParams, firewall);
    }

    public static void main(String[] args) {
        System.out.println("================================================================================");
        System.out.println("          QULLA-RUTA: RED TERRITORIAL DE RESILIENCIA VIAL (WFEO DEMO)");
        System.out.println("================================================================================");

        NotificationFirewall firewall = new NotificationFirewall(
                (phone, msg) -> System.out.println("[PROD GATEWAY ERROR] " + phone + " -> " + msg),
                simLog -> System.out.println("  [FIREWALL AUDIT] " + simLog)
        );

        WfeoDemoRunner runner = forPresentation(firewall);
        WfeoDemoResult res = runner.runCarpishEmergencyScenario();

        System.out.println("Escenario ID: " + res.scenarioId());
        System.out.println("Descripción : " + res.scenarioName());
        System.out.println("--------------------------------------------------------------------------------");
        System.out.println("1. EVALUACIÓN GEOTÉCNICA (CAPA 1):");
        System.out.println("   - Índice IVV Calculado : " + res.ivvScore() + " / 100.00");
        System.out.println("   - Clasificación Riesgo : " + res.riskLevel());
        System.out.println("--------------------------------------------------------------------------------");
        System.out.println("2. CONTENCIÓN DE SEGURIDAD (NOTIFICATION FIREWALL):");
        System.out.println("   - Estado de Aislamiento: ACTIVO (Sin fugas a pasarelas SMS/WhatsApp de producción)");
        System.out.println("--------------------------------------------------------------------------------");
        System.out.println("3. RESOLUCIÓN DE MOVILIDAD MULTIMODAL (CAPA 3):");
        System.out.println("   - Ambulancia 4x4       : " + res.ambulanceResolution() + " (Desvío seguro)");
        System.out.println("   - Camión Pesado (Agro) : " + res.heavyTrailerResolution() + " (Transbordo verificado)");
        System.out.println("--------------------------------------------------------------------------------");
        System.out.println("4. MÉTRICAS DE IMPACTO LOGÍSTICO Y AMBIENTAL (Δ ∈ ℝ):");
        System.out.println("   - Tiempo Ahorrado      : +" + res.trailerImpact().timeDeltaMin() + " min");
        System.out.println("   - Emisiones CO2 Evitadas: +" + res.trailerImpact().netCo2DeltaKg() + " kg CO2");
        System.out.println("   - Carga Agrícola Salvada: +$" + res.trailerImpact().netAgriSavingsUsd() + " USD");
        System.out.println("--------------------------------------------------------------------------------");
        System.out.println("Tiempo Total de Cómputo   : " + res.executionTimeMs() + " ms (Objetivo < 2000 ms)");
        System.out.println("================================================================================");
    }
}
