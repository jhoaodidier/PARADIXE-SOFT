package pe.qullaruta.map;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

@DisplayName("Proveedor de Mapas OpenStreetMap (100% Java Estándar)")
class OsmTileProviderTest {

    @Test
    @DisplayName("Convierte lat/lon de Carpish a coordenadas X/Y de tesela OSM con precisión")
    void shouldConvertCoordinatesToOsmTileIndices() {
        // Carpish (-9.705, -76.095) en zoom 11
        int tileX = OsmTileProvider.lonToTileX(-76.095, 11);
        int tileY = OsmTileProvider.latToTileY(-9.705, 11);

        assertThat(tileX).isGreaterThan(0);
        assertThat(tileY).isGreaterThan(0);
        // Validar rango dentro de 2^11 = 2048
        assertThat(tileX).isLessThan(2048);
        assertThat(tileY).isLessThan(2048);
    }

    @Test
    @DisplayName("Comportamiento Fail-Safe: URL inválida o sin red no arroja excepción no controlada")
    void shouldHandleNetworkAbsenceGracefully() {
        OsmTileProvider provider = new OsmTileProvider();
        // Zoom absurdo o fuera de rango
        var tile = provider.getTile(99, 999999, 999999);
        // Debe retornar null de forma segura, permitiendo el fallback vectorial
        assertThat(tile).isNull();
    }
}
