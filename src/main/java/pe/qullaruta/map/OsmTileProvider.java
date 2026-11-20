package pe.qullaruta.map;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.time.Duration;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Proveedor de Teselas de OpenStreetMap (OSM) 100% Java Estándar.
 * Cumple la filosofía Offline-First: descarga teselas con timeout estricto y almacena en caché en memoria.
 * Si no hay conexión a internet, se degrada a modo offline de forma transparente.
 */
public final class OsmTileProvider {

    private static final String TILE_URL_FORMAT = "https://tile.openstreetmap.org/%d/%d/%d.png";
    private final HttpClient httpClient;
    private final Map<String, BufferedImage> tileCache = new ConcurrentHashMap<>();

    public OsmTileProvider() {
        this.httpClient = HttpClient.newBuilder()
                .connectTimeout(Duration.ofSeconds(2))
                .build();
    }

    public static int lonToTileX(double lon, int zoom) {
        return (int) Math.floor((lon + 180.0) / 360.0 * (1 << zoom));
    }

    public static int latToTileY(double lat, int zoom) {
        double latRad = Math.toRadians(lat);
        return (int) Math.floor((1.0 - Math.log(Math.tan(latRad) + 1.0 / Math.cos(latRad)) / Math.PI) / 2.0 * (1 << zoom));
    }

    /**
     * Obtiene una tesela de mapa (Tile) por coordenadas slippy map de forma síncrona con timeout.
     * Retorna null si no hay internet (fail-safe offline).
     */
    public BufferedImage getTile(int zoom, int x, int y) {
        String key = zoom + "/" + x + "/" + y;
        if (tileCache.containsKey(key)) {
            return tileCache.get(key);
        }

        try {
            String url = String.format(TILE_URL_FORMAT, zoom, x, y);
            HttpRequest request = HttpRequest.newBuilder()
                    .uri(URI.create(url))
                    .header("User-Agent", "QullaRuta-DisasterResilience/0.2.0 (Hackathon demo)")
                    .timeout(Duration.ofSeconds(2))
                    .GET()
                    .build();

            HttpResponse<byte[]> response = httpClient.send(request, HttpResponse.BodyHandlers.ofByteArray());
            if (response.statusCode() == 200) {
                BufferedImage img = ImageIO.read(new ByteArrayInputStream(response.body()));
                if (img != null) {
                    tileCache.put(key, img);
                    return img;
                }
            }
        } catch (Exception ignored) {
            // Falla de red o modo offline: retorna null para que el renderizador use el gráfico vectorial
        }
        return null;
    }

    public void prefetchCorridorAsync(double centerLat, double centerLon, int zoom, Runnable onComplete) {
        Thread.startVirtualThread(() -> {
            int centerX = lonToTileX(centerLon, zoom);
            int centerY = latToTileY(centerLat, zoom);

            // Precargar cuadrante 3x3 alrededor del centro
            for (int dx = -1; dx <= 1; dx++) {
                for (int dy = -1; dy <= 1; dy++) {
                    getTile(zoom, centerX + dx, centerY + dy);
                }
            }
            if (onComplete != null) {
                onComplete.run();
            }
        });
    }

    public int getCachedTilesCount() {
        return tileCache.size();
    }
}
