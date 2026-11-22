# Política de Seguridad — PARADIXE SOFT

**TakumiSoft** trata la seguridad operativa, la protección de datos personales y la integridad criptográfica como requisitos no negociables del sistema.

## Reporte de Vulnerabilidades

Si detectas una vulnerabilidad de seguridad, fuga potencial de secretos o fallo *fail-safe*:

1. **No** abras un *issue* público con detalles reproducibles del exploit.
2. Envía un reporte privado a `security@takumisoft.dev` o mediante **GitHub Security Advisories** en este repositorio.
3. Incluye versión afectada, impacto potencial y pasos mínimos de reproducción.

## Invariantes de Seguridad Aplicados

* **Autenticación y OTP:** Contraseñas con **Argon2id**; OTPs almacenados únicamente como `HMAC-SHA256(OTP_SECRET, challenge_id || phone_e164 || otp)` con secreto fuera de la base de datos, máximo 5 intentos y expiración a los 5 minutos.
* **Rotación de Tokens:** JWT de acceso de corta duración (`15 min`) y *refresh tokens* con detección de reuso (`token_family_id`) que revoca toda la familia de sesión ante uso repetido.
* **Privacidad GPS Seudónima:** `client_ephemeral_id` rotado cada día UTC y convertido en servidor mediante `HMAC-SHA256(daily_salt_bytes, client_ephemeral_id)` con llaves almacenadas fuera de BD, sin guardar `user_id`, JWT ni IP junto a los pings y con purga máxima a las 24 horas.
* **Auditoría Redactada:** `audit_logs` y `sync_change_log.payload_json` prohíben registrar tokens, OTPs, contraseñas, llaves o datos personales identificables.
* **Segregación de Simulacros:** El módulo de simulacros opera bajo un rol PostgreSQL de solo lectura en tablas operativas y sin credenciales ni salida hacia pasarelas reales de WhatsApp/SMS.
* **Seguridad de Memoria en Rust:** `#![forbid(unsafe_code)]` en los crates del dominio y motores de cálculo.
