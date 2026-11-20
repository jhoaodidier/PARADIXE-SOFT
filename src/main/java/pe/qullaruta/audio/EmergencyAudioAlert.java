package pe.qullaruta.audio;

import javax.sound.sampled.*;
import java.io.ByteArrayInputStream;

/**
 * Generador y Reproductor de Audio de Emergencia 100% Java Estándar (javax.sound.sampled).
 * Sintetiza dinámicamente en memoria ondas PCM para alertas acústicas sin requerir archivos externos.
 */
public final class EmergencyAudioAlert {

    private static final float SAMPLE_RATE = 44100f;

    private EmergencyAudioAlert() {}

    /**
     * Genera y reproduce una sirena de emergencia de dos tonos (880 Hz / 587 Hz) de forma asíncrona.
     */
    public static void playEmergencySirenAsync() {
        Thread.startVirtualThread(() -> {
            try {
                byte[] audioData = synthesizeTwoToneSiren(880, 587, 0.25, 2);
                playPcmData(audioData);
            } catch (Exception ignored) {
                // En entornos de CI/CD headless sin tarjeta de sonido física, se degrada silenciosamente sin fallar
            }
        });
    }

    /**
     * Genera y reproduce un tono suave de confirmación de ruta segura (523 Hz - C5).
     */
    public static void playRouteResolvedChimeAsync() {
        Thread.startVirtualThread(() -> {
            try {
                byte[] audioData = synthesizeSineTone(523.25, 0.35);
                playPcmData(audioData);
            } catch (Exception ignored) {}
        });
    }

    public static byte[] synthesizeSineTone(double freqHz, double durationSec) {
        int totalSamples = (int) (SAMPLE_RATE * durationSec);
        byte[] buffer = new byte[totalSamples * 2]; // 16-bit mono

        for (int i = 0; i < totalSamples; i++) {
            double time = i / (double) SAMPLE_RATE;
            // Modulación de envolvente para evitar clics al inicio/fin
            double envelope = Math.sin(Math.PI * (i / (double) totalSamples));
            short sample = (short) (Math.sin(2 * Math.PI * freqHz * time) * 32767 * 0.4 * envelope);

            buffer[2 * i] = (byte) (sample & 0xFF);
            buffer[2 * i + 1] = (byte) ((sample >> 8) & 0xFF);
        }
        return buffer;
    }

    public static byte[] synthesizeTwoToneSiren(double freqHigh, double freqLow, double durationToneSec, int cycles) {
        int samplesPerTone = (int) (SAMPLE_RATE * durationToneSec);
        int totalSamples = samplesPerTone * 2 * cycles;
        byte[] buffer = new byte[totalSamples * 2];
        int bufferIdx = 0;

        for (int c = 0; c < cycles; c++) {
            // Tono Alto
            for (int i = 0; i < samplesPerTone; i++) {
                double time = i / (double) SAMPLE_RATE;
                short sample = (short) (Math.sin(2 * Math.PI * freqHigh * time) * 32767 * 0.5);
                buffer[bufferIdx++] = (byte) (sample & 0xFF);
                buffer[bufferIdx++] = (byte) ((sample >> 8) & 0xFF);
            }
            // Tono Bajo
            for (int i = 0; i < samplesPerTone; i++) {
                double time = i / (double) SAMPLE_RATE;
                short sample = (short) (Math.sin(2 * Math.PI * freqLow * time) * 32767 * 0.5);
                buffer[bufferIdx++] = (byte) (sample & 0xFF);
                buffer[bufferIdx++] = (byte) ((sample >> 8) & 0xFF);
            }
        }
        return buffer;
    }

    private static void playPcmData(byte[] pcmData) throws LineUnavailableException, Exception {
        AudioFormat format = new AudioFormat(SAMPLE_RATE, 16, 1, true, false);
        try (AudioInputStream ais = new AudioInputStream(new ByteArrayInputStream(pcmData), format, pcmData.length / 2)) {
            DataLine.Info info = new DataLine.Info(Clip.class, format);
            if (!AudioSystem.isLineSupported(info)) {
                return;
            }
            Clip clip = (Clip) AudioSystem.getLine(info);
            clip.open(ais);
            clip.start();
            // Esperar que termine de reproducir
            Thread.sleep(clip.getMicrosecondLength() / 1000);
            clip.close();
        }
    }
}
