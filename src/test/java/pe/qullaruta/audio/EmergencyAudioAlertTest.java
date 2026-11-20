package pe.qullaruta.audio;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import static org.assertj.core.api.Assertions.assertThat;

@DisplayName("Audio de Emergencia (Sintetizador PCM 100% Java)")
class EmergencyAudioAlertTest {

    @Test
    @DisplayName("Sintetiza buffer de onda sinusoidal PCM sin errores")
    void shouldSynthesizeSineTonePcmBuffer() {
        byte[] pcm = EmergencyAudioAlert.synthesizeSineTone(440.0, 0.1);
        assertThat(pcm).isNotEmpty();
        // 44100 muestras/s * 0.1 s = 4410 muestras * 2 bytes = 8820 bytes
        assertThat(pcm.length).isEqualTo(8820);
    }

    @Test
    @DisplayName("Sintetiza sirena de dos tonos con longitud exacta de muestras")
    void shouldSynthesizeTwoToneSiren() {
        byte[] pcm = EmergencyAudioAlert.synthesizeTwoToneSiren(880, 587, 0.1, 2);
        assertThat(pcm).isNotEmpty();
        // 4410 muestras * 2 tonos * 2 ciclos = 17640 muestras * 2 bytes = 35280 bytes
        assertThat(pcm.length).isEqualTo(35280);
    }

    @Test
    @DisplayName("Invocación asíncrona de sirena no bloquea el hilo actual")
    void shouldTriggerSirenAsynchronouslyWithoutBlocking() {
        long start = System.currentTimeMillis();
        EmergencyAudioAlert.playEmergencySirenAsync();
        long elapsed = System.currentTimeMillis() - start;

        // Debe retornar de inmediato (< 50 ms) gracias al hilo virtual
        assertThat(elapsed).isLessThan(50);
    }
}
