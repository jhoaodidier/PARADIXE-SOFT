package pe.qullaruta.gui;

import pe.qullaruta.audio.EmergencyAudioAlert;
import pe.qullaruta.ivv.*;
import pe.qullaruta.map.OsmTileProvider;
import pe.qullaruta.simulator.*;

import javax.swing.*;
import javax.swing.border.EmptyBorder;
import javax.swing.border.LineBorder;
import java.awt.*;
import java.awt.geom.Path2D;
import java.awt.image.BufferedImage;
import java.math.BigDecimal;
import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.List;

/**
 * 100% Java Swing GUI: Panel de Control de Resiliencia Territorial y Simulación WFEO.
 * Incluye:
 * - Alerta acústica sintetizada en memoria con javax.sound.sampled.
 * - Integración de mapas OpenStreetMap con fallback vectorial Offline-First.
 * - Cero dependencias externas (solo JDK estándar).
 */
public final class WfeoDemoGui extends JFrame {

    private final IvvModelVersion model = new IvvModelVersion(
            "ivv-v1.0-rainy-andes",
            new BigDecimal("0.400"), new BigDecimal("0.300"), new BigDecimal("0.200"), new BigDecimal("0.100"),
            new BigDecimal("80.00"), new BigDecimal("180.00"), new BigDecimal("45.0"), 500,
            new BigDecimal("40.00"), new BigDecimal("70.00"), 3
    );

    private final ImpactParameters impactParams = new ImpactParameters(
            "impact-v1.0-huanuco-2026", "HEAVY_TRAILER",
            new BigDecimal("2.500"), new BigDecimal("0.850"),
            new BigDecimal("18000.00"), new BigDecimal("0.0500"), new BigDecimal("0.080")
    );

    private final List<String> firewallLogs = new ArrayList<>();
    private final NotificationFirewall firewall = new NotificationFirewall(
            (phone, msg) -> {},
            log -> firewallLogs.add(log)
    );

    private final OsmTileProvider tileProvider = new OsmTileProvider();

    // Componentes interactivos
    private final JLabel lblIvvScore = new JLabel("2.00");
    private final JLabel lblIvvBadge = new JLabel("GREEN");
    private final JLabel lblTimeSaved = new JLabel("-- min");
    private final JLabel lblCo2Saved = new JLabel("-- kg");
    private final JLabel lblAgriSaved = new JLabel("-- USD");
    private final JTextArea txtFirewall = new JTextArea();
    private final CorridorCanvas canvas;

    public WfeoDemoGui() {
        super("QULLA-RUTA — Red Territorial de Resiliencia Vial (100% Java)");
        this.canvas = new CorridorCanvas(tileProvider);

        setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        setSize(1120, 740);
        setLocationRelativeTo(null);

        initUi();

        // Precargar mapa de Carpish de fondo en hilo virtual
        tileProvider.prefetchCorridorAsync(-9.705, -76.095, 11, canvas::repaint);
    }

    private void initUi() {
        JPanel root = new JPanel(new BorderLayout());
        root.setBackground(new Color(15, 23, 42)); // Slate 900

        // Panel Izquierdo: Controles y Métricas
        JPanel leftPanel = new JPanel();
        leftPanel.setLayout(new BoxLayout(leftPanel, BoxLayout.Y_AXIS));
        leftPanel.setBackground(new Color(30, 41, 59)); // Slate 800
        leftPanel.setBorder(new EmptyBorder(20, 20, 20, 20));
        leftPanel.setPreferredSize(new Dimension(380, 740));

        // Título
        JLabel title = new JLabel("QULLA-RUTA");
        title.setFont(new Font("SansSerif", Font.BOLD, 22));
        title.setForeground(new Color(56, 189, 248)); // Sky 400
        JLabel subtitle = new JLabel("Simulador de Resiliencia Vial (Huánuco – Carpish)");
        subtitle.setFont(new Font("SansSerif", Font.PLAIN, 12));
        subtitle.setForeground(new Color(148, 163, 184)); // Slate 400

        leftPanel.add(title);
        leftPanel.add(subtitle);
        leftPanel.add(Box.createVerticalStrut(20));

        // Tarjeta IVV
        JPanel ivvCard = createCard("Vulnerabilidad Geotécnica (IVV)");
        lblIvvScore.setFont(new Font("SansSerif", Font.BOLD, 32));
        lblIvvScore.setForeground(Color.WHITE);
        lblIvvBadge.setFont(new Font("SansSerif", Font.BOLD, 12));
        lblIvvBadge.setOpaque(true);
        lblIvvBadge.setBackground(new Color(34, 197, 94, 50));
        lblIvvBadge.setForeground(new Color(34, 197, 94));
        lblIvvBadge.setBorder(new LineBorder(new Color(34, 197, 94), 1, true));

        JPanel ivvRow = new JPanel(new FlowLayout(FlowLayout.LEFT, 10, 0));
        ivvRow.setBackground(new Color(51, 65, 85));
        ivvRow.add(lblIvvScore);
        ivvRow.add(lblIvvBadge);
        ivvCard.add(ivvRow);
        leftPanel.add(ivvCard);
        leftPanel.add(Box.createVerticalStrut(15));

        // Botón Simulación con Sirena Acústica
        JButton btnSimulate = new JButton("⚡ Ejecutar Simulación de Emergencia");
        btnSimulate.setFont(new Font("SansSerif", Font.BOLD, 13));
        btnSimulate.setBackground(new Color(239, 68, 68)); // Red 500
        btnSimulate.setForeground(Color.WHITE);
        btnSimulate.setFocusPainted(false);
        btnSimulate.setBorder(new EmptyBorder(12, 14, 12, 14));
        btnSimulate.setCursor(new Cursor(Cursor.HAND_CURSOR));
        btnSimulate.setMaximumSize(new Dimension(Short.MAX_VALUE, 45));
        btnSimulate.addActionListener(e -> runSimulation());
        leftPanel.add(btnSimulate);
        leftPanel.add(Box.createVerticalStrut(15));

        // Tarjeta Métricas de Impacto
        JPanel impactCard = createCard("Impacto Socioeconómico (Δ ∈ ℝ)");
        impactCard.add(createMetricRow("Tiempo Ahorrado:", lblTimeSaved));
        impactCard.add(createMetricRow("CO2 Evitado:", lblCo2Saved));
        impactCard.add(createMetricRow("Carga Agrícola:", lblAgriSaved));
        leftPanel.add(impactCard);
        leftPanel.add(Box.createVerticalStrut(15));

        // Tarjeta Notification Firewall Audit
        JPanel firewallCard = createCard("Notification Firewall (Aislamiento)");
        txtFirewall.setEditable(false);
        txtFirewall.setFont(new Font("Monospaced", Font.PLAIN, 11));
        txtFirewall.setBackground(new Color(9, 13, 22));
        txtFirewall.setForeground(new Color(165, 243, 252));
        txtFirewall.setText("[FIREWALL ACTIVO]\nContención estricta.\nCero fugas a producción.");
        JScrollPane scroll = new JScrollPane(txtFirewall);
        scroll.setPreferredSize(new Dimension(340, 90));
        scroll.setBorder(new LineBorder(new Color(71, 85, 105), 1));
        firewallCard.add(scroll);
        leftPanel.add(firewallCard);

        root.add(leftPanel, BorderLayout.WEST);
        root.add(canvas, BorderLayout.CENTER);

        setContentPane(root);
    }

    private JPanel createCard(String titleText) {
        JPanel card = new JPanel();
        card.setLayout(new BoxLayout(card, BoxLayout.Y_AXIS));
        card.setBackground(new Color(51, 65, 85));
        card.setBorder(new EmptyBorder(10, 12, 10, 12));
        card.setMaximumSize(new Dimension(Short.MAX_VALUE, 160));

        JLabel title = new JLabel(titleText.toUpperCase());
        title.setFont(new Font("SansSerif", Font.BOLD, 10));
        title.setForeground(new Color(148, 163, 184));
        card.add(title);
        card.add(Box.createVerticalStrut(8));
        return card;
    }

    private JPanel createMetricRow(String labelText, JLabel valLabel) {
        JPanel row = new JPanel(new BorderLayout());
        row.setBackground(new Color(51, 65, 85));
        JLabel lbl = new JLabel(labelText);
        lbl.setFont(new Font("SansSerif", Font.PLAIN, 12));
        lbl.setForeground(new Color(226, 232, 240));

        valLabel.setFont(new Font("SansSerif", Font.BOLD, 13));
        valLabel.setForeground(new Color(34, 197, 94));

        row.add(lbl, BorderLayout.WEST);
        row.add(valLabel, BorderLayout.EAST);
        return row;
    }

    private void runSimulation() {
        // Disparar sirena acústica de emergencia (asíncrona)
        EmergencyAudioAlert.playEmergencySirenAsync();

        WfeoDemoRunner runner = new WfeoDemoRunner(model, impactParams, firewall);
        WfeoDemoResult res = runner.runCarpishEmergencyScenario();

        // Actualizar UI
        lblIvvScore.setText(res.ivvScore().toString());
        lblIvvBadge.setText(res.riskLevel().name());
        lblIvvBadge.setForeground(new Color(239, 68, 68));
        lblIvvBadge.setBackground(new Color(239, 68, 68, 50));
        lblIvvBadge.setBorder(new LineBorder(new Color(239, 68, 68), 1, true));

        lblTimeSaved.setText("+" + res.trailerImpact().timeDeltaMin() + " min");
        lblCo2Saved.setText("+" + res.trailerImpact().netCo2DeltaKg() + " kg");
        lblAgriSaved.setText("+$" + res.trailerImpact().netAgriSavingsUsd() + " USD");

        String time = LocalTime.now().format(DateTimeFormatter.ofPattern("HH:mm:ss"));
        txtFirewall.setText("[" + time + "] ESCENARIO: Carpish Emergency\n" +
                "[FIREWALL AUDIT] Interceptado despacho a SMS/WhatsApp\n" +
                "[RUTEO] Ambulancia: ALTERNATE_ROUTE\n" +
                "[RUTEO] Tráiler: LOGISTIC_BRIDGE_REQUIRED\n" +
                "[BARRERA] 0 mensajes a producción.");

        canvas.setSimulationActive(true);
        canvas.repaint();

        // Tono de confirmación de resolución de ruta
        EmergencyAudioAlert.playRouteResolvedChimeAsync();
    }

    /**
     * Lienzo gráfico que combina teselas OpenStreetMap (cuando hay red)
     * con fallback vectorial topográfico Offline-First.
     */
    static class CorridorCanvas extends JPanel {
        private boolean isSimulationActive = false;
        private final OsmTileProvider tileProvider;

        public CorridorCanvas(OsmTileProvider tileProvider) {
            this.tileProvider = tileProvider;
            setBackground(new Color(15, 23, 42));
        }

        public void setSimulationActive(boolean active) {
            this.isSimulationActive = active;
        }

        @Override
        protected void paintComponent(Graphics g) {
            super.paintComponent(g);
            Graphics2D g2 = (Graphics2D) g.create();
            g2.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);

            int w = getWidth();
            int h = getHeight();

            // 1. Capa de Fondo: OpenStreetMap Tile (si está en caché o disponible)
            int centerX = OsmTileProvider.lonToTileX(-76.095, 11);
            int centerY = OsmTileProvider.latToTileY(-9.705, 11);
            BufferedImage tile = tileProvider.getTile(11, centerX, centerY);

            if (tile != null) {
                // Dibujar tesela OSM con oscurecimiento para preservar contraste del dashboard
                g2.drawImage(tile, 0, 0, w, h, null);
                g2.setColor(new Color(15, 23, 42, 190)); // Capa semitransparente oscura
                g2.fillRect(0, 0, w, h);
            } else {
                // Fallback Offline-First: Renderizado vectorial de montañas de los Andes
                g2.setColor(new Color(30, 41, 59));
                Path2D mountains = new Path2D.Double();
                mountains.moveTo(0, h * 0.7);
                mountains.curveTo(w * 0.2, h * 0.4, w * 0.4, h * 0.8, w * 0.6, h * 0.3);
                mountains.curveTo(w * 0.8, h * 0.5, w * 0.9, h * 0.2, w, h * 0.6);
                mountains.lineTo(w, h);
                mountains.lineTo(0, h);
                mountains.closePath();
                g2.fill(mountains);
            }

            // 2. Trazado Vial Carretera Central (Huánuco -> Carpish -> Tingo María)
            g2.setStroke(new BasicStroke(6f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
            if (isSimulationActive) {
                // Tramo Huánuco -> Carpish Sur (Verde)
                g2.setColor(new Color(34, 197, 94));
                g2.drawLine(w / 8, h / 2, w * 3 / 8, h / 2);

                // Tramo Túnel Carpish bloqueado (Rojo punteado)
                g2.setColor(new Color(239, 68, 68));
                float[] dash = {10f, 10f};
                g2.setStroke(new BasicStroke(6f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND, 10f, dash, 0f));
                g2.drawLine(w * 3 / 8, h / 2, w * 5 / 8, h / 2);

                // Tramo Carpish Norte -> Tingo María
                g2.setStroke(new BasicStroke(6f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
                g2.setColor(new Color(34, 197, 94));
                g2.drawLine(w * 5 / 8, h / 2, w * 7 / 8, h / 2);

                // Ruta alterna desvío seguro (Ambulancia)
                g2.setColor(new Color(56, 189, 248));
                g2.setStroke(new BasicStroke(3f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
                Path2D bypass = new Path2D.Double();
                bypass.moveTo(w * 3 / 8, h / 2);
                bypass.curveTo(w * 4 / 8, h / 2 - 120, w * 4 / 8, h / 2 - 120, w * 5 / 8, h / 2);
                g2.draw(bypass);

                g2.setFont(new Font("SansSerif", Font.BOLD, 12));
                g2.drawString("Desvío Ambulancia 4x4 (Trocha segura)", w * 4 / 8 - 110, h / 2 - 130);
            } else {
                g2.setColor(new Color(34, 197, 94));
                g2.drawLine(w / 8, h / 2, w * 7 / 8, h / 2);
            }

            // Nodos territoriales
            drawNode(g2, w / 8, h / 2, "Huánuco (Km 0)", new Color(56, 189, 248));
            drawNode(g2, w * 4 / 8, h / 2, isSimulationActive ? "Túnel Carpish (Huaico RED)" : "Túnel Carpish (Normal)",
                    isSimulationActive ? new Color(239, 68, 68) : new Color(34, 197, 94));
            drawNode(g2, w * 7 / 8, h / 2, "Tingo María (Km 135)", new Color(56, 189, 248));

            g2.dispose();
        }

        private void drawNode(Graphics2D g2, int x, int y, String label, Color color) {
            g2.setColor(color);
            g2.fillOval(x - 10, y - 10, 20, 20);
            g2.setColor(Color.WHITE);
            g2.setStroke(new BasicStroke(2f));
            g2.drawOval(x - 10, y - 10, 20, 20);

            g2.setFont(new Font("SansSerif", Font.BOLD, 12));
            g2.setColor(new Color(241, 245, 249));
            g2.drawString(label, x - 50, y + 28);
        }
    }

    public static void main(String[] args) {
        SwingUtilities.invokeLater(() -> {
            WfeoDemoGui gui = new WfeoDemoGui();
            gui.setVisible(true);
        });
    }
}

