package pe.qullaruta.ui;

import pe.qullaruta.simulator.NotificationFirewall;
import pe.qullaruta.simulator.WfeoDemoResult;
import pe.qullaruta.simulator.WfeoDemoRunner;

import javax.swing.*;
import java.awt.*;
import java.text.NumberFormat;
import java.util.ArrayList;
import java.util.Locale;

public final class CarpishDemo extends JPanel {
    private static final Color PAPER = new Color(0xF5F4ED);
    private static final Color INK = new Color(0x173A35);
    private static final Color MUTED = new Color(0x52665F);
    private static final Color MINT = new Color(0xB4EACB);
    private static final Color ALERT = new Color(0xA43826);
    private final JPanel results = column(PAPER, 0);
    private final JButton repeat = new JButton("Repetir simulación");

    CarpishDemo() {
        super(new BorderLayout(0, 16));
        setBackground(PAPER);
        setBorder(BorderFactory.createEmptyBorder(20, 28, 20, 28));

        JPanel title = column(PAPER, 0);
        title.add(label("QULLA-RUTA   /   DEMO WFEO", 13, true, MUTED));
        title.add(label("Huaico en Carpish", 34, true, INK));
        JPanel header = new JPanel(new BorderLayout(16, 0));
        header.setOpaque(false);
        header.add(title);
        JPanel actions = new JPanel(new GridBagLayout());
        actions.setOpaque(false);
        repeat.setFont(new Font("Inter", Font.BOLD, 14));
        repeat.setBackground(INK);
        repeat.setForeground(Color.WHITE);
        repeat.setMargin(new Insets(9, 16, 9, 16));
        repeat.addActionListener(event -> simulate());
        actions.add(repeat);
        header.add(actions, BorderLayout.EAST);
        add(header, BorderLayout.NORTH);
        add(results);
        simulate();
    }

    private void simulate() {
        var audit = new ArrayList<String>();
        var firewall = new NotificationFirewall((phone, message) -> {
            throw new IllegalStateException("Se intentó un despacho externo");
        }, audit::add);
        results.removeAll();
        try {
            var result = WfeoDemoRunner.forPresentation(firewall).runCarpishEmergencyScenario();
            results.add(new Corridor());
            results.add(Box.createVerticalStrut(12));
            JPanel assessment = row(2);
            assessment.add(vulnerability(result));
            JPanel mobility = column(Color.WHITE, 16);
            mobility.add(label("Respuesta por vehículo", 18, true, INK));
            mobility.add(Box.createVerticalStrut(8));
            mobility.add(label("Ambulancia 4×4", 13, false, MUTED));
            mobility.add(label(route(result.ambulanceResolution()), 17, true, INK));
            mobility.add(Box.createVerticalStrut(6));
            mobility.add(label("Camión pesado", 13, false, MUTED));
            mobility.add(label(route(result.heavyTrailerResolution()), 17, true, INK));
            assessment.add(mobility);
            results.add(assessment);

            results.add(Box.createVerticalStrut(16));
            results.add(label("Impacto simulado en la carga agrícola", 18, true, INK));
            results.add(Box.createVerticalStrut(8));
            var impact = result.trailerImpact();
            JPanel metrics = row(3);
            metrics.add(metric("Tiempo ahorrado", number(impact.timeDeltaMin()) + " min"));
            metrics.add(metric("Emisiones evitadas", number(impact.netCo2DeltaKg()) + " kg CO₂"));
            metrics.add(metric("Pérdida evitada", "USD " + number(impact.netAgriSavingsUsd())));
            results.add(metrics);
            results.add(Box.createVerticalStrut(12));
            results.add(label("Alertas interceptadas: " + audit.size() + "    ·    Tiempo de cálculo: "
                    + result.executionTimeMs() + " ms", 14, true, INK));
            results.add(label("ID del escenario: " + result.scenarioId(), 12, false, MUTED));
        } catch (RuntimeException ex) {
            results.removeAll();
            results.add(label("No se pudo completar la simulación. " + ex.getMessage(), 16, true, ALERT));
        }
        results.revalidate();
        results.repaint();
    }

    private static JPanel vulnerability(WfeoDemoResult result) {
        JPanel panel = column(Color.WHITE, 16);
        panel.add(label("Índice de vulnerabilidad vial", 18, true, INK));
        panel.add(label(number(result.ivvScore()) + " / 100", 34, true, INK));
        String risk = switch (result.riskLevel()) {
            case RED -> "Rojo";
            case AMBER -> "Ámbar";
            case GREEN -> "Verde";
        };
        panel.add(label("Nivel de riesgo: " + risk, 15, true, INK));
        panel.add(Box.createVerticalStrut(8));
        JProgressBar gauge = new JProgressBar(0, 100);
        gauge.setValue(result.ivvScore().intValue());
        gauge.setBorderPainted(false);
        gauge.setForeground(switch (result.riskLevel()) {
            case RED -> ALERT;
            case AMBER -> new Color(0x9A6700);
            case GREEN -> new Color(0x26724F);
        });
        gauge.setAlignmentX(LEFT_ALIGNMENT);
        gauge.setMaximumSize(new Dimension(Integer.MAX_VALUE, 9));
        gauge.getAccessibleContext().setAccessibleName("Índice de vulnerabilidad vial");
        panel.add(gauge);
        return panel;
    }

    private static String route(String code) {
        return switch (code) {
            case "ALTERNATE_ROUTE" -> "Desvío por ruta alterna";
            case "LOGISTIC_BRIDGE_REQUIRED" -> "Transbordo requerido";
            default -> code;
        };
    }

    private static String number(Number value) {
        return NumberFormat.getNumberInstance(Locale.forLanguageTag("es-PE")).format(value);
    }

    private static JPanel metric(String title, String value) {
        JPanel card = column(new Color(0xE4EBE3), 14);
        card.add(label(title, 14, false, MUTED));
        card.add(label(value, 24, true, INK));
        return card;
    }

    private static JPanel column(Color background, int padding) {
        JPanel panel = new JPanel();
        panel.setLayout(new BoxLayout(panel, BoxLayout.Y_AXIS));
        panel.setBackground(background);
        panel.setBorder(BorderFactory.createEmptyBorder(padding, padding, padding, padding));
        panel.setAlignmentX(LEFT_ALIGNMENT);
        return panel;
    }

    private static JPanel row(int columns) {
        JPanel panel = new JPanel(new GridLayout(1, columns, 12, 0));
        panel.setOpaque(false);
        panel.setAlignmentX(LEFT_ALIGNMENT);
        return panel;
    }

    private static JLabel label(String text, int size, boolean bold, Color color) {
        JLabel label = new JLabel(text);
        label.setFont(new Font("Inter", bold ? Font.BOLD : Font.PLAIN, size));
        label.setForeground(color);
        label.setAlignmentX(LEFT_ALIGNMENT);
        return label;
    }

    private static final class Corridor extends JPanel {
        Corridor() {
            super(new GridLayout(1, 3));
            setBackground(INK);
            setAlignmentX(LEFT_ALIGNMENT);
            setBorder(BorderFactory.createEmptyBorder(50, 8, 12, 8));
            for (String stop : new String[]{"Huánuco", "Carpish", "Tingo María"}) {
                JLabel name = label(stop, 16, true, Color.WHITE);
                name.setHorizontalAlignment(SwingConstants.CENTER);
                add(name);
            }
            getAccessibleContext().setAccessibleName("Corredor simulado Huánuco, Carpish y Tingo María. Huaico en Carpish.");
        }

        @Override protected void paintComponent(Graphics graphics) {
            super.paintComponent(graphics);
            Graphics2D g = (Graphics2D) graphics.create();
            g.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
            int width = getWidth() - 16;
            int start = 8 + width / 6;
            int center = getWidth() / 2;
            int end = getWidth() - start;
            int y = getHeight() / 2;
            g.setFont(new Font("Inter", Font.BOLD, 12));
            g.setColor(MINT);
            g.drawString("CORREDOR VIAL   /   DATOS SINTÉTICOS", 16, 20);
            g.setStroke(new BasicStroke(3));
            g.drawLine(start, y, end, y);
            for (int x : new int[]{start, end}) {
                g.fillOval(x - 5, y - 5, 10, 10);
            }
            g.setColor(new Color(0xFFAB83));
            g.fillPolygon(new int[]{center, center + 9, center, center - 9}, new int[]{y - 9, y, y + 9, y}, 4);
            g.dispose();
        }

        @Override public Dimension getMaximumSize() {
            return new Dimension(Integer.MAX_VALUE, getPreferredSize().height);
        }
    }

    public static void main(String[] args) {
        SwingUtilities.invokeLater(() -> {
            CarpishDemo demo = new CarpishDemo();
            JFrame window = new JFrame("Qulla-Ruta · Carpish");
            window.setDefaultCloseOperation(WindowConstants.EXIT_ON_CLOSE);
            JScrollPane view = new JScrollPane(demo);
            view.setBorder(null);
            view.getVerticalScrollBar().setUnitIncrement(20);
            window.setContentPane(view);
            window.getRootPane().setDefaultButton(demo.repeat);
            Rectangle screen = GraphicsEnvironment.getLocalGraphicsEnvironment().getMaximumWindowBounds();
            window.setSize(Math.min(880, screen.width), Math.min(demo.getPreferredSize().height + 52, screen.height));
            window.setLocationRelativeTo(null);
            window.setVisible(true);
        });
    }
}
