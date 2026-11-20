package pe.qullaruta.simulator;

import java.util.Objects;
import java.util.function.Consumer;

/**
 * Barrera Arquitectónica de Aislamiento (Notification Firewall).
 * Garantiza contención estricta: en modo SIMULATION se aborta cualquier despacho
 * hacia el mundo exterior y solo se registra internamente con prefijo [SIMULATION].
 */
public final class NotificationFirewall {

    private final ExternalDispatcher productionDispatcher;
    private final Consumer<String> simulationAuditLogger;

    public NotificationFirewall(ExternalDispatcher productionDispatcher, Consumer<String> simulationAuditLogger) {
        this.productionDispatcher = Objects.requireNonNull(productionDispatcher, "productionDispatcher cannot be null");
        this.simulationAuditLogger = Objects.requireNonNull(simulationAuditLogger, "simulationAuditLogger cannot be null");
    }

    public void sendNotification(ExecutionContext context, String recipientPhone, String message) {
        if (context == ExecutionContext.SIMULATION) {
            String logEntry = "[SIMULATION] Intercepted dispatch to " + recipientPhone + ": " + message;
            simulationAuditLogger.accept(logEntry);
            throw new SafetyBarrierViolationException("Blocked dispatch in SIMULATION context: " + recipientPhone);
        }

        productionDispatcher.dispatch(recipientPhone, message);
    }
}
