package pe.qullaruta.simulator;

public class SafetyBarrierViolationException extends RuntimeException {
    public SafetyBarrierViolationException(String message) {
        super(message);
    }
}
