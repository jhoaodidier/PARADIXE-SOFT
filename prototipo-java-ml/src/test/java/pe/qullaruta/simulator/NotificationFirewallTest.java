package pe.qullaruta.simulator;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

/**
 * Suite de Prueba para SIM-01: Aislamiento Fail-Safe y Firewall de Notificaciones.
 * Filosofía aviónica: El simulador de vuelo nunca debe accionar los alerones reales del avión.
 */
@DisplayName("SIM-01: Notification Firewall y Aislamiento de Simulación")
class NotificationFirewallTest {

    static class MockExternalDispatcher implements ExternalDispatcher {
        int productionDispatches = 0;

        @Override
        public void dispatch(String recipientPhone, String message) {
            productionDispatches++;
        }
    }

    @Test
    @DisplayName("SIM-01: En contexto SIMULATION, cualquier despacho externo debe ser abortado y registrado solo internamente")
    void shouldBlockExternalDispatchInSimulationContext() {
        MockExternalDispatcher mockGateway = new MockExternalDispatcher();
        List<String> internalSimLogs = new ArrayList<>();
        NotificationFirewall firewall = new NotificationFirewall(mockGateway, internalSimLogs::add);

        // Intento de envío en SIMULATION -> Debe interceptar y registrar con prefijo [SIMULATION]
        assertThatThrownBy(() -> firewall.sendNotification(
                ExecutionContext.SIMULATION,
                "+51999888777",
                "ALERTA: Derrumbe inminente en Carpish"
        )).isInstanceOf(SafetyBarrierViolationException.class)
          .hasMessageContaining("Blocked dispatch in SIMULATION context");

        // Verificaciones críticas de aislamiento
        assertThat(mockGateway.productionDispatches)
                .as("Cero llamadas a la pasarela real de producción")
                .isZero();

        assertThat(internalSimLogs)
                .hasSize(1)
                .first()
                .asString()
                .startsWith("[SIMULATION]");
    }

    @Test
    @DisplayName("En contexto PRODUCTION, el despacho hacia el gateway externo debe proceder")
    void shouldAllowExternalDispatchInProductionContext() {
        MockExternalDispatcher mockGateway = new MockExternalDispatcher();
        List<String> internalSimLogs = new ArrayList<>();
        NotificationFirewall firewall = new NotificationFirewall(mockGateway, internalSimLogs::add);

        firewall.sendNotification(
                ExecutionContext.PRODUCTION,
                "+51999888777",
                "Alerta operativa oficial"
        );

        assertThat(mockGateway.productionDispatches).isEqualTo(1);
        assertThat(internalSimLogs).isEmpty();
    }
}
