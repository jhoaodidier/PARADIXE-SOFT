# PARADIXE SOFT

**Red Territorial de Resiliencia Vial y Logística Crítica** — Desarrollado por **TakumiSoft (TAKUMISOFT)**

PARADIXE SOFT es una plataforma **Offline-First** de apoyo a la decisión vial y logística crítica diseñada para corredores andino-amazónicos vulnerables a huaicos, deslizamientos e inundaciones (piloto territorial inicial: **Huánuco – Ambo – Carpish – Tingo María / Chavinillo, Puente Durand y rutas alternas/vecinales**).

> **Principio Operativo *Fail-Safe*:** PARADIXE SOFT es un sistema de apoyo a la decisión. No emite órdenes vinculantes de tránsito ni sustituye a COER, Defensa Civil o personal médico. Cuando faltan datos verificados o existe riesgo incompatible, responde explícitamente `NO_SAFE_ROUTE` en lugar de improvisar rutas.

---

## Capacidades Principales

1. **Capa 1 — Predicción Geotécnica Reproducible ($IVV$):** Cálculo versionado cada 3 horas del Índice Dinámico de Vulnerabilidad Vial combinando precipitación acumulada 24–72 h (`SENAMHI`, `NASA_GPM`, `OPEN_METEO`, `LOCAL_STATION`), pendiente y quebradas (`DEM SRTM/ALOS`), susceptibilidad del suelo y puntos críticos (`INGEMMET`/`Provías`), con expiración estricta (`UNKNOWN / STALE`).
2. **Capa 2 — Detección Híbrida y Privacidad Seudónima:** Separación entre verificación (`SUSPECTED`, `CONFIRMED`, `DISMISSED`, `RESOLVED`) y transitabilidad (`OPEN`, `CAUTION_SUSPECTED`, `PARTIAL_ONE_LANE`, `BLOCKED`). Telemetría GPS con identificadores diarios efímeros y `HMAC-SHA256` (purga máxima 24 h) más consenso comunitario acotado por proximidad PostGIS.
3. **Capa 3 — Enrutamiento Multimodal Resiliente y Puente Logístico Verificado:** Grafo dirigido con soporte bidireccional real (`ST_Reverse` y pendientes diferenciadas por sentido), poda por categoría vehicular (`AMBULANCE_4X4`, `PICKUP_4X4`, `LIGHT_TRUCK`, `HEAVY_TRAILER`, `MOTORCYCLE`), ventanas humanitarias direccionales y propuesta de corredores de transbordo inspeccionados vigentes.
4. **Módulo Institucional de Simulacros y Contingencia (*What-If / Drills*):** Evaluación de escenarios de temporada de lluvias en estricto aislamiento de solo lectura sobre tablas operativas y sin egreso a pasarelas externas.

---

## Arquitectura y Stack Tecnológico (Opción A)

* **Backend Core, Motores CLI e Ingestor (`Rust`):** `Axum`, `Tokio`, `SQLx`, `utoipa` (OpenAPI 3.1), `rust_decimal` (`MidpointAwayFromZero` idéntico a PostgreSQL `NUMERIC`), `Argon2id`, `HMAC-SHA256`, `secrecy`/`zeroize`, `tracing`/OpenTelemetry.
* **Base de Datos Espacial:** `PostgreSQL 16` + `PostGIS 3.4` en 3NF operativa estricta, cálculos métricos en `::geography`, auditoría inmutable y sincronización transaccional por bloqueo de fila en `sync_control` (sin `BIGSERIAL`/`nextval`).
* **Frontend PWA Offline-First (`apps/web`):** `TypeScript`, `React`, `Vite`, `MapLibre GL JS`, `PMTiles`, `Workbox` + `IndexedDB` (`outbox_queue` idempotente), siguiendo estándares modernos vía [Modern Web Guidance](./.agents/skills/modern-web-guidance/SKILL.md).
* **Mensajería Multicanal (`paradixe-messaging`):** `trait NotificationGateway` con adaptadores para **WhatsApp Cloud API (Meta)** y **SMS fallback** + webhooks firmados HMAC anti-replay ($\le 5\text{ min}$).
* **Almacenamiento de Objetos S3:** `Cloudflare R2` (producción) / `MinIO` (desarrollo local) para evidencias fotográficas y `PMTiles` regionales.

---

## Estructura del Repositorio

| Ruta | Propósito |
| :--- | :--- |
| [docs/PARADIXE_SOFT.md](./docs/PARADIXE_SOFT.md) | Especificación técnica y arquitectónica de producción (`2.0.0-prod-sdd-rust`) |
| [migrations/0001_initial_schema.sql](./migrations/0001_initial_schema.sql) | Esquema inicial PostgreSQL 16 + PostGIS 3.4 (27 tablas 3NF, `GiST ::geography`, `CHECK`s) |
| [migrations/0002_seed_base_catalogs.sql](./migrations/0002_seed_base_catalogs.sql) | Catálogos base (`sync_control`, roles RBAC, categorías vehiculares y versiones iniciales) |
| [docker-compose.yml](./docker-compose.yml) | Orquestación local de PostgreSQL + PostGIS y MinIO S3 (`paradixe-media`, `paradixe-tiles`) |
| [.env.example](./.env.example) | Plantilla documentada de variables de entorno y secretos |
| [.agents/skills/modern-web-guidance/SKILL.md](./.agents/skills/modern-web-guidance/SKILL.md) | Skill de estándares web modernos para el desarrollo del frontend PWA |

---

## Requisitos e Inicio Local

### Requisitos del Sistema
* **Rust** (`rustup`, `rustc`, `cargo` stable)
* **Docker** y **Docker Compose**
* **Node.js** ($\ge 22$) y **npm** (para la PWA y herramientas `modern-web-guidance`)
* Cliente **PostgreSQL** (`psql`) opcional para administración directa

En Arch / CachyOS Linux:
```bash
sudo pacman -S --needed rustup docker docker-compose postgresql-libs postgresql nodejs npm
rustup default stable
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"
```

### Arranque de Servicios Base
```bash
cp .env.example .env
docker compose up -d
```

---

## Principios de Ingeniería (SDD + TDD)

* **Library-First + CLI JSON:** Motores desacoplados (`paradixe-ivv`, `paradixe-consensus`, `paradixe-routing`, `paradixe-impact`) con interfaz `stdin`/`stdout` JSON determinista.
* **Contract-First & Integration-First:** OpenAPI 3.1 como fuente única, tests de contrato e integración sobre PostGIS real (**RED $\rightarrow$ GREEN**) cubriendo los 34 casos obligatorios (`SEC`, `CTR`, `GEO`, `IVV`, `RTE`, `CON`, `SYN`, `BRG`, `SIM`, `IMP`, `E2E-01`).
* **Cero fugas y auditoría estricta:** Ningún secreto, token, OTP o identificador efímero de GPS se persiste en `audit_logs`, `sync_change_log` ni trazas HTTP.

---

## Hoja de Ruta (4 Etapas)

1. **Etapa 1 — Núcleo Rust + PostGIS Productivo:** Cargo Workspace, API Axum/SQLx, OpenAPI 3.1 y suite de 34 pruebas contra PostGIS real.
2. **Etapa 2 — Pipeline de Datos Reales (`paradixe-ingestor`) + PMTiles:** Importación OSM Geofabrik Perú, pendientes DEM SRTM/ALOS, puntos críticos MTC/INGEMMET, worker climático cada 3 h y tiles vectoriales `PMTiles`.
3. **Etapa 3 — PWA Offline-First (`apps/web`) + Mensajería Multicanal:** Mapa interactivo offline con MapLibre + IndexedDB, sincronización diferencial y pasarela WhatsApp Cloud API / SMS.
4. **Etapa 4 — Piloto Operativo B2B/B2G y Calibración:** Validación en el corredor Huánuco–Carpish–Tingo María con operadores logísticos, gremios agrícolas, COER y redes de salud, y calibración empírica del $IVV$.

---

**TakumiSoft** — Ingeniería de resiliencia territorial sostenible.
