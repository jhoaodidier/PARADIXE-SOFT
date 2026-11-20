package pe.qullaruta.gui;

import pe.qullaruta.audio.EmergencyAudioAlert;
import pe.qullaruta.ivv.*;
import pe.qullaruta.map.OsmTileProvider;
import pe.qullaruta.simulator.*;

import javax.swing.*;
import javax.swing.border.EmptyBorder;
import java.awt.*;
import java.awt.event.ComponentAdapter;
import java.awt.event.ComponentEvent;
import java.awt.geom.Path2D;
import java.awt.geom.RoundRectangle2D;
import java.awt.image.BufferedImage;
import java.math.BigDecimal;
import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.List;

/**
 * QULLA-RUTA Navigation HUD (Estilo Waze Moderno — 100% Java Swing / Java2D).
 * 
 * Enfoque: Resiliencia Territorial, Alertas Geotécnicas (IVV), Puentes Logísticos y Simulación WFEO.
 * Características de diseño moderno:
 * - JLayeredPane con mapa base a pantalla completa (Slippy Map OSM + Offline Vectorial).
 * - Componentes flotantes Glassmorphic (cabecera turn-by-turn, alerta de peligro, HUD inferior).
 * - Trazado de ruta con brillo de neón, semáforo de riesgo y cursor vehicular.
 * - Cero dependencias externas (100% JDK estándar).
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

    // Estado interactivo
    private boolean isSimulationActive = false;
    private String selectedVehicle = "AMBULANCE_4X4"; // "AMBULANCE_4X4" o "HEAVY_TRAILER"

    // Componentes del HUD
    private FullMapCanvas mapCanvas;
    private JPanel overlayPanel;
    private JLabel lblTurnHeader;
    private JLabel lblSubTurn;
    private JLabel lblIvvBadge;
    private JPanel hazardAlertCard;
    private JLabel lblHazardTitle;
    private JLabel lblHazardDesc;
    private JLabel lblEtaTime;
    private JLabel lblEtaDistance;
    private JLabel lblTimeSavedBadge;
    private JLabel lblCo2SavedBadge;
    private JLabel lblAgriSavedBadge;
    private JButton btnSimulate;

    public WfeoDemoGui() {
        super("QULLA-RUTA — Navegador de Resiliencia Vial (Huánuco – Carpish)");
        setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        setSize(1180, 780);
        setMinimumSize(new Dimension(960, 640));
        setLocationRelativeTo(null);

        initLayeredUi();

        // Precargar mapa de Carpish de fondo
        tileProvider.prefetchCorridorAsync(-9.705, -76.095, 11, () -> mapCanvas.repaint());
    }

    private void initLayeredUi() {
        JLayeredPane layeredPane = new JLayeredPane();
        layeredPane.setLayout(null);

        // CAPA 0: Mapa a pantalla completa
        mapCanvas = new FullMapCanvas(tileProvider);
        layeredPane.add(mapCanvas, JLayeredPane.DEFAULT_LAYER);

        // CAPA 1: Contenedor flotante transparente estilo Waze
        overlayPanel = new JPanel(null);
        overlayPanel.setOpaque(false);
        layeredPane.add(overlayPanel, JLayeredPane.PALETTE_LAYER);

        buildFloatingWazeComponents();

        // Ajuste dinámico de dimensiones al redimensionar la ventana
        addComponentListener(new ComponentAdapter() {
            @Override
            public void componentResized(ComponentEvent e) {
                int w = getContentPane().getWidth();
                int h = getContentPane().getHeight();
                mapCanvas.setBounds(0, 0, w, h);
                overlayPanel.setBounds(0, 0, w, h);
                relayoutOverlays(w, h);
            }
        });

        setContentPane(layeredPane);
    }

    private void buildFloatingWazeComponents() {
        // 1. Barra Superior Flotante: Indicador de Navegación y Próximo Hito
        JPanel topNavCard = new GlassPanel(16, new Color(18, 24, 38, 235));
        topNavCard.setLayout(new BorderLayout(15, 0));
        topNavCard.setBorder(new EmptyBorder(12, 18, 12, 18));

        JLabel iconArrow = new JLabel("⬆️");
        iconArrow.setFont(new Font("SansSerif", Font.PLAIN, 28));

        JPanel navTextCol = new JPanel(new GridLayout(2, 1, 0, 2));
        navTextCol.setOpaque(false);
        lblTurnHeader = new JLabel("Carretera Central (PE-18A) — Hacia Tingo María");
        lblTurnHeader.setFont(new Font("SansSerif", Font.BOLD, 15));
        lblTurnHeader.setForeground(Color.WHITE);

        lblSubTurn = new JLabel("A 12 km: Paso por Túnel Carpish • Condiciones Normales");
        lblSubTurn.setFont(new Font("SansSerif", Font.PLAIN, 12));
        lblSubTurn.setForeground(new Color(148, 163, 184)); // Slate 400

        navTextCol.add(lblTurnHeader);
        navTextCol.add(lblSubTurn);

        // Badge de Riesgo Geotécnico IVV (Semáforo Waze)
        lblIvvBadge = new JLabel("IVV: 2.00 GREEN", SwingConstants.CENTER);
        lblIvvBadge.setFont(new Font("SansSerif", Font.BOLD, 11));
        lblIvvBadge.setForeground(new Color(46, 213, 115));
        lblIvvBadge.setOpaque(true);
        lblIvvBadge.setBackground(new Color(46, 213, 115, 40));
        lblIvvBadge.setBorder(BorderFactory.createCompoundBorder(
                BorderFactory.createLineBorder(new Color(46, 213, 115), 1, true),
                new EmptyBorder(4, 10, 4, 10)
        ));

        topNavCard.add(iconArrow, BorderLayout.WEST);
        topNavCard.add(navTextCol, BorderLayout.CENTER);
        topNavCard.add(lblIvvBadge, BorderLayout.EAST);
        topNavCard.setName("topNavCard");
        overlayPanel.add(topNavCard);

        // 2. Tarjeta Flotante Lateral de Peligro / Alerta Waze (Aparece al detectar huaico)
        hazardAlertCard = new GlassPanel(16, new Color(30, 20, 28, 240));
        hazardAlertCard.setLayout(new BorderLayout(10, 8));
        hazardAlertCard.setBorder(BorderFactory.createCompoundBorder(
                BorderFactory.createLineBorder(new Color(239, 68, 68), 1, true),
                new EmptyBorder(14, 16, 14, 16)
        ));

        JLabel hazardIcon = new JLabel("⚠️");
        hazardIcon.setFont(new Font("SansSerif", Font.PLAIN, 26));

        JPanel hazardTextCol = new JPanel(new GridLayout(2, 1, 0, 3));
        hazardTextCol.setOpaque(false);
        lblHazardTitle = new JLabel("HUAICO Y DERRUMBE CONFIRMADO");
        lblHazardTitle.setFont(new Font("SansSerif", Font.BOLD, 13));
        lblHazardTitle.setForeground(new Color(255, 71, 87));

        lblHazardDesc = new JLabel("Km 47 Túnel Carpish • Calzada Bloqueada");
        lblHazardDesc.setFont(new Font("SansSerif", Font.PLAIN, 12));
        lblHazardDesc.setForeground(new Color(226, 232, 240));

        hazardTextCol.add(lblHazardTitle);
        hazardTextCol.add(lblHazardDesc);

        hazardAlertCard.add(hazardIcon, BorderLayout.WEST);
        hazardAlertCard.add(hazardTextCol, BorderLayout.CENTER);
        hazardAlertCard.setName("hazardAlertCard");
        hazardAlertCard.setVisible(false); // Oculto hasta simular
        overlayPanel.add(hazardAlertCard);

        // 3. HUD Inferior Flotante (ETA, Distancia, Tiempo Ahorrado, Botón Waze)
        JPanel bottomHud = new GlassPanel(20, new Color(18, 24, 38, 240));
        bottomHud.setLayout(new BorderLayout(20, 0));
        bottomHud.setBorder(new EmptyBorder(14, 22, 14, 22));

        // Columna Izquierda: ETA y Distancia
        JPanel etaCol = new JPanel(new GridLayout(2, 1, 0, 2));
        etaCol.setOpaque(false);
        lblEtaTime = new JLabel("14:25 • Llegada");
        lblEtaTime.setFont(new Font("SansSerif", Font.BOLD, 18));
        lblEtaTime.setForeground(Color.WHITE);

        lblEtaDistance = new JLabel("135 km restantes • Ruta Troncal");
        lblEtaDistance.setFont(new Font("SansSerif", Font.PLAIN, 12));
        lblEtaDistance.setForeground(new Color(148, 163, 184));
        etaCol.add(lblEtaTime);
        etaCol.add(lblEtaDistance);

        // Columna Central: Métricas de Resiliencia Territorial (Ahorro)
        JPanel savingsCol = new JPanel(new FlowLayout(FlowLayout.CENTER, 14, 4));
        savingsCol.setOpaque(false);

        lblTimeSavedBadge = createPillLabel("⏱️ Sin Demora", new Color(56, 189, 248));
        lblCo2SavedBadge = createPillLabel("🌱 CO₂ Normal", new Color(46, 213, 115));
        lblAgriSavedBadge = createPillLabel("📦 Carga Protegida", new Color(245, 158, 11));

        savingsCol.add(lblTimeSavedBadge);
        savingsCol.add(lblCo2SavedBadge);
        savingsCol.add(lblAgriSavedBadge);

        // Columna Derecha: Botón de Simulación de Emergencia Estilo Waze
        JPanel actionCol = new JPanel(new FlowLayout(FlowLayout.RIGHT, 10, 0));
        actionCol.setOpaque(false);

        // Selector de vehículo (Ambulancia vs Camión Agrícola)
        JComboBox<String> vehicleSelector = new JComboBox<>(new String[]{"🚑 Ambulancia 4x4", "🚛 Tráiler Agrícola"});
        vehicleSelector.setFont(new Font("SansSerif", Font.BOLD, 12));
        vehicleSelector.setBackground(new Color(51, 65, 85));
        vehicleSelector.setForeground(Color.WHITE);
        vehicleSelector.addActionListener(e -> {
            selectedVehicle = vehicleSelector.getSelectedIndex() == 0 ? "AMBULANCE_4X4" : "HEAVY_TRAILER";
            if (isSimulationActive) {
                triggerSimulationWorkflow();
            }
        });

        btnSimulate = new JButton("⚡ SIMULAR CRISIS WFEO");
        btnSimulate.setFont(new Font("SansSerif", Font.BOLD, 13));
        btnSimulate.setBackground(new Color(239, 68, 68)); // Rojo Waze Hazard
        btnSimulate.setForeground(Color.WHITE);
        btnSimulate.setFocusPainted(false);
        btnSimulate.setCursor(new Cursor(Cursor.HAND_CURSOR));
        btnSimulate.setBorder(new EmptyBorder(10, 18, 10, 18));
        btnSimulate.addActionListener(e -> triggerSimulationWorkflow());

        actionCol.add(vehicleSelector);
        actionCol.add(btnSimulate);

        bottomHud.add(etaCol, BorderLayout.WEST);
        bottomHud.add(savingsCol, BorderLayout.CENTER);
        bottomHud.add(actionCol, BorderLayout.EAST);
        bottomHud.setName("bottomHud");
        overlayPanel.add(bottomHud);
    }

    private JLabel createPillLabel(String text, Color accent) {
        JLabel lbl = new JLabel(text);
        lbl.setFont(new Font("SansSerif", Font.BOLD, 11));
        lbl.setForeground(accent);
        lbl.setOpaque(true);
        lbl.setBackground(new Color(accent.getRed(), accent.getGreen(), accent.getBlue(), 35));
        lbl.setBorder(BorderFactory.createCompoundBorder(
                BorderFactory.createLineBorder(accent, 1, true),
                new EmptyBorder(4, 10, 4, 10)
        ));
        return lbl;
    }

    private void relayoutOverlays(int w, int h) {
        for (Component c : overlayPanel.getComponents()) {
            if ("topNavCard".equals(c.getName())) {
                int cardWidth = Math.min(w - 60, 780);
                c.setBounds((w - cardWidth) / 2, 20, cardWidth, 65);
            } else if ("hazardAlertCard".equals(c.getName())) {
                c.setBounds(30, 95, 420, 75);
            } else if ("bottomHud".equals(c.getName())) {
                int hudWidth = w - 60;
                c.setBounds(30, h - 95, hudWidth, 75);
            }
        }
    }

    private void triggerSimulationWorkflow() {
        isSimulationActive = true;

        // 1. Alerta Acústica de Emergencia (Sirena de 2 tonos en hilo virtual)
        EmergencyAudioAlert.playEmergencySirenAsync();

        // 2. Ejecutar Simulación Geotécnica y de Ruteo
        WfeoDemoRunner runner = new WfeoDemoRunner(model, impactParams, firewall);
        WfeoDemoResult res = runner.runCarpishEmergencyScenario();

        // 3. Actualizar Visualización Estilo Waze
        lblIvvBadge.setText("IVV: " + res.ivvScore() + " RED");
        lblIvvBadge.setForeground(new Color(255, 71, 87));
        lblIvvBadge.setBackground(new Color(255, 71, 87, 45));
        lblIvvBadge.setBorder(BorderFactory.createCompoundBorder(
                BorderFactory.createLineBorder(new Color(255, 71, 87), 1, true),
                new EmptyBorder(4, 10, 4, 10)
        ));

        // Activar tarjeta flotante de peligro
        hazardAlertCard.setVisible(true);
        if ("AMBULANCE_4X4".equals(selectedVehicle)) {
            lblHazardTitle.setText("⚠️ HUAICO EN CARPISH — DESVÍO ACTIVADO");
            lblHazardDesc.setText("Ambulancia 4x4 redirigida por Trocha Chinchao (+15 km)");
            lblEtaTime.setText("14:48 • Desvío Asignado");
            lblEtaDistance.setText("150 km restantes • Ruta Alterna Verificada");
            lblTurnHeader.setText("Desvío Autorizado: Trocha Chinchao (Corredor Seguro)");
            lblSubTurn.setText("Paso prioritario humanitario • Evita zona de peligro");
        } else {
            lblHazardTitle.setText("⛔ BLOQUEO TOTAL — PUENTE LOGÍSTICO");
            lblHazardDesc.setText("Camión pesado no apto para trocha. Transbordo seguro.");
            lblEtaTime.setText("16:30 • En Espera de Transbordo");
            lblEtaDistance.setText("Km 47 • Zona de Espera Geotécnicamente Estable");
            lblTurnHeader.setText("Detención Segura: Zona de Relevo y Transbordo");
            lblSubTurn.setText("Sendero peatonal certificado por Defensa Civil fuera de huaico");
        }

        // Actualizar badges de impacto de resiliencia
        lblTimeSavedBadge.setText("⏱️ Ahorro: +" + res.trailerImpact().timeDeltaMin().intValue() + " min");
        lblCo2SavedBadge.setText("🌱 Evitado: +" + res.trailerImpact().netCo2DeltaKg() + " kg CO₂");
        lblAgriSavedBadge.setText("📦 Salvado: +$" + res.trailerImpact().netAgriSavingsUsd() + " USD");

        btnSimulate.setText("⚡ CRISIS ACTIVA (WFEO)");
        btnSimulate.setBackground(new Color(255, 71, 87));

        // Actualizar lienzo de navegación
        mapCanvas.setSimulationState(true, selectedVehicle);
        mapCanvas.repaint();

        // 4. Tono de Confirmación Armónico
        EmergencyAudioAlert.playRouteResolvedChimeAsync();
    }

    /**
     * Panel con fondo semitransparente Glassmorphism y bordes redondeados.
     */
    static class GlassPanel extends JPanel {
        private final int cornerRadius;
        private final Color bgColor;

        public GlassPanel(int radius, Color bgColor) {
            this.cornerRadius = radius;
            this.bgColor = bgColor;
            setOpaque(false);
        }

        @Override
        protected void paintComponent(Graphics g) {
            Graphics2D g2 = (Graphics2D) g.create();
            g2.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);

            // Sombra suave
            g2.setColor(new Color(0, 0, 0, 70));
            g2.fill(new RoundRectangle2D.Double(2, 2, getWidth() - 4, getHeight() - 4, cornerRadius, cornerRadius));

            // Fondo translúcido
            g2.setColor(bgColor);
            g2.fill(new RoundRectangle2D.Double(0, 0, getWidth() - 1, getHeight() - 1, cornerRadius, cornerRadius));

            // Borde sutil
            g2.setColor(new Color(255, 255, 255, 30));
            g2.draw(new RoundRectangle2D.Double(0, 0, getWidth() - 1, getHeight() - 1, cornerRadius, cornerRadius));

            g2.dispose();
            super.paintComponent(g);
        }
    }

    /**
     * Lienzo de navegación a pantalla completa: mapa satelital/OSM con trazado de ruta neón.
     */
    static class FullMapCanvas extends JPanel {
        private final OsmTileProvider tileProvider;
        private boolean isSimulationActive = false;
        private String vehicleType = "AMBULANCE_4X4";

        public FullMapCanvas(OsmTileProvider tileProvider) {
            this.tileProvider = tileProvider;
            setBackground(new Color(15, 23, 42));
        }

        public void setSimulationState(boolean active, String vehicleType) {
            this.isSimulationActive = active;
            this.vehicleType = vehicleType;
        }

        @Override
        protected void paintComponent(Graphics g) {
            super.paintComponent(g);
            Graphics2D g2 = (Graphics2D) g.create();
            g2.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);

            int w = getWidth();
            int h = getHeight();

            // 1. Fondo: Tesela OpenStreetMap (con oscurecimiento tipo modo nocturno Waze)
            int centerX = OsmTileProvider.lonToTileX(-76.095, 11);
            int centerY = OsmTileProvider.latToTileY(-9.705, 11);
            BufferedImage tile = tileProvider.getTile(11, centerX, centerY);

            if (tile != null) {
                g2.drawImage(tile, 0, 0, w, h, null);
                // Filtro azul oscuro tipo Waze Dark Navigation
                g2.setColor(new Color(15, 23, 42, 210));
                g2.fillRect(0, 0, w, h);
            } else {
                // Fallback Offline: Relieve de cordillera
                g2.setColor(new Color(24, 32, 47));
                Path2D mountains = new Path2D.Double();
                mountains.moveTo(0, h * 0.75);
                mountains.curveTo(w * 0.25, h * 0.45, w * 0.5, h * 0.85, w * 0.75, h * 0.35);
                mountains.curveTo(w * 0.85, h * 0.5, w * 0.95, h * 0.3, w, h * 0.7);
                mountains.lineTo(w, h);
                mountains.lineTo(0, h);
                mountains.closePath();
                g2.fill(mountains);
            }

            int roadY = h / 2 + 30;

            // 2. Trazado de Carretera Principal
            if (!isSimulationActive) {
                // Estado Normal: Línea Waze Cyan Brillante
                drawGlowLine(g2, w / 8, roadY, w * 7 / 8, roadY, new Color(0, 210, 255), 8f);
            } else {
                // Tramo Huánuco -> Carpish Sur (Transitable)
                drawGlowLine(g2, w / 8, roadY, w * 3 / 8, roadY, new Color(0, 210, 255), 8f);

                // Tramo Túnel Carpish (Huaico / Bloqueo en Rojo Alerta)
                g2.setColor(new Color(255, 71, 87));
                float[] dash = {12f, 10f};
                g2.setStroke(new BasicStroke(8f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND, 10f, dash, 0f));
                g2.drawLine(w * 3 / 8, roadY, w * 5 / 8, roadY);

                // Tramo Carpish Norte -> Tingo María
                drawGlowLine(g2, w * 5 / 8, roadY, w * 7 / 8, roadY, new Color(46, 213, 115), 8f);

                // Si es Ambulancia: Renderizar Desvío de Trocha Chinchao (Curva Cyan)
                if ("AMBULANCE_4X4".equals(vehicleType)) {
                    Path2D detour = new Path2D.Double();
                    detour.moveTo(w * 3 / 8, roadY);
                    detour.curveTo(w * 4 / 8, roadY - 140, w * 4 / 8, roadY - 140, w * 5 / 8, roadY);

                    // Resplandor
                    g2.setColor(new Color(0, 210, 255, 70));
                    g2.setStroke(new BasicStroke(12f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
                    g2.draw(detour);

                    g2.setColor(new Color(0, 210, 255));
                    g2.setStroke(new BasicStroke(5f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
                    g2.draw(detour);

                    // Etiqueta del desvío
                    g2.setFont(new Font("SansSerif", Font.BOLD, 12));
                    g2.setColor(new Color(56, 189, 248));
                    g2.drawString("Ruta Alterna Ambulancia (Trocha Chinchao)", w * 4 / 8 - 120, roadY - 150);
                }
            }

            // 3. Marcadores de Nodos Territoriales Estilo Waze
            drawWazePin(g2, w / 8, roadY, "Huánuco", new Color(56, 189, 248), "🏙️");
            drawWazePin(g2, w * 4 / 8, roadY, isSimulationActive ? "Huaico Carpish" : "Túnel Carpish",
                    isSimulationActive ? new Color(255, 71, 87) : new Color(46, 213, 115),
                    isSimulationActive ? "⚠️" : "🚇");
            drawWazePin(g2, w * 7 / 8, roadY, "Tingo María", new Color(56, 189, 248), "🏁");

            // 4. Cursor Vehicular Waze (Navegando)
            int cursorX = isSimulationActive ? (w * 3 / 8 - 10) : (w / 4);
            int cursorY = roadY;
            drawCarCursor(g2, cursorX, cursorY);

            g2.dispose();
        }

        private void drawGlowLine(Graphics2D g2, int x1, int y1, int x2, int y2, Color color, float width) {
            // Capa de resplandor (Glow)
            g2.setColor(new Color(color.getRed(), color.getGreen(), color.getBlue(), 60));
            g2.setStroke(new BasicStroke(width + 6f, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
            g2.drawLine(x1, y1, x2, y2);

            // Línea central sólida
            g2.setColor(color);
            g2.setStroke(new BasicStroke(width, BasicStroke.CAP_ROUND, BasicStroke.JOIN_ROUND));
            g2.drawLine(x1, y1, x2, y2);
        }

        private void drawWazePin(Graphics2D g2, int x, int y, String label, Color color, String emoji) {
            // Círculo exterior
            g2.setColor(color);
            g2.fillOval(x - 16, y - 16, 32, 32);
            g2.setColor(Color.WHITE);
            g2.setStroke(new BasicStroke(2f));
            g2.drawOval(x - 16, y - 16, 32, 32);

            // Icono central
            g2.setFont(new Font("SansSerif", Font.PLAIN, 14));
            g2.drawString(emoji, x - 7, y + 5);

            // Etiqueta flotante
            g2.setFont(new Font("SansSerif", Font.BOLD, 12));
            g2.setColor(Color.WHITE);
            FontMetrics fm = g2.getFontMetrics();
            int textW = fm.stringWidth(label);
            g2.setColor(new Color(15, 23, 42, 200));
            g2.fillRoundRect(x - textW / 2 - 6, y + 22, textW + 12, 20, 8, 8);
            g2.setColor(Color.WHITE);
            g2.drawString(label, x - textW / 2, y + 36);
        }

        private void drawCarCursor(Graphics2D g2, int x, int y) {
            // Cursor vehicular tipo flecha de navegación Waze
            g2.setColor(new Color(0, 210, 255));
            Path2D arrow = new Path2D.Double();
            arrow.moveTo(x + 12, y);
            arrow.lineTo(x - 8, y - 8);
            arrow.lineTo(x - 4, y);
            arrow.lineTo(x - 8, y + 8);
            arrow.closePath();
            g2.fill(arrow);

            g2.setColor(Color.WHITE);
            g2.setStroke(new BasicStroke(1.5f));
            g2.draw(arrow);
        }
    }

    public static void main(String[] args) {
        SwingUtilities.invokeLater(() -> {
            WfeoDemoGui gui = new WfeoDemoGui();
            gui.setVisible(true);
        });
    }
}
