package pe.qullaruta.ui;

import org.junit.jupiter.api.Test;
import javax.swing.*;
import java.awt.*;
import java.util.Arrays;
import java.util.List;
import java.util.stream.Stream;
import static org.assertj.core.api.Assertions.assertThat;

class CarpishDemoTest {
    @Test
    void repeatRunsANewScenarioWithTheSameResults() throws Exception {
        SwingUtilities.invokeAndWait(() -> {
            var panel = new CarpishDemo();
            String firstId = checkResults(panel);
            children(panel).filter(JButton.class::isInstance).map(JButton.class::cast)
                    .filter(button -> button.getText().equals("Repetir simulación"))
                    .findFirst().orElseThrow().doClick();
            assertThat(checkResults(panel)).isNotEqualTo(firstId);
        });
    }

    private static String checkResults(CarpishDemo panel) {
        List<String> labels = children(panel).filter(JLabel.class::isInstance).map(JLabel.class::cast)
                .map(JLabel::getText).toList();
        assertThat(labels).contains("91.27 / 100", "Nivel de riesgo: Rojo", "Desvío por ruta alterna",
                "Transbordo requerido", "720 min", "18.5 kg CO₂", "USD 9,810");
        assertThat(labels).anyMatch(text -> text.startsWith("Alertas interceptadas: 1 "));
        return labels.stream().filter(text -> text.startsWith("ID del escenario: ")).findFirst().orElseThrow();
    }

    private static Stream<Component> children(Component component) {
        if (component instanceof Container parent) {
            return Stream.concat(Stream.of(component), Arrays.stream(parent.getComponents()).flatMap(CarpishDemoTest::children));
        }
        return Stream.of(component);
    }
}
