<div align="center">

<img src="./docs/assets/banner.svg" alt="PARADIXE SOFT — Red Territorial de Resiliencia Vial y Logística Crítica" width="100%" />

<p>
  <strong>Red Territorial Offline-First de Resiliencia Vial, Predicción Geotécnica y Logística Crítica</strong><br/>
  Desarrollado por <strong>TakumiSoft (TAKUMISOFT)</strong> · Corredor piloto <em>Huánuco – Ambo – Carpish – Tingo María</em>
</p>

<p>
  <a href="https://github.com/jhoaodidier/PARADIXE-SOFT/actions/workflows/ci.yml"><img src="https://github.com/jhoaodidier/PARADIXE-SOFT/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <img src="https://img.shields.io/badge/Rust-Core%20Workspace-000000?logo=rust&logoColor=white" alt="Rust Core" />
  <img src="https://img.shields.io/badge/TypeScript-PWA%20Offline--First-3178C6?logo=typescript&logoColor=white" alt="TypeScript PWA" />
  <img src="https://img.shields.io/badge/PostgreSQL%2016-PostGIS%203.4%20(3NF)-336791?logo=postgresql&logoColor=white" alt="PostgreSQL 16 + PostGIS 3.4" />
  <img src="https://img.shields.io/badge/OpenAPI-3.1.0-6BA539?logo=openapiinitiative&logoColor=white" alt="OpenAPI 3.1.0" />
  <img src="https://img.shields.io/badge/Architecture-Fail--Safe%20SDD-0b4f6c" alt="Fail-Safe SDD" />
</p>

<p>
  <a href="./docs/PARADIXE_SOFT.md">Especificación SDD</a> ·
  <a href="./openapi/openapi-v1.json">Contrato OpenAPI 3.1</a> ·
  <a href="#inicio-rapido">Inicio Rápido</a> ·
  <a href="#motores-cli-en-rust">Motores Rust CLI</a> ·
  <a href="./SECURITY.md">Seguridad</a> ·
  <a href="./CONTRIBUTING.md">Contribuir</a>
</p>

</div>

---

## Visión del Producto

En corredores andino-amazónicos como **Huánuco – Carpish – Tingo María**, un único deslizamiento o huaico durante la temporada de lluvias puede aislar provincias enteras, retrasar referencias médicas hacia el **Hospital Regional Hermilio Valdizán** y deteriorar toneladas de carga agrícola perecible (papa, café, cacao, frutas).

**PARADIXE SOFT** optimiza la red de transporte multimodal existente —**ambulancias $4\times 4$, camionetas, camiones agrícolas, carga pesada y motocicletas**— combinando datos abiertos (`OSM`, `DEM SRTM/ALOS`, `MTC/INGEMMET`, `SENAMHI`, `NASA GPM`, `Open-Meteo`), operación **Offline-First** en móviles rurales y un núcleo determinista en **Rust** para garantizar ultra bajo consumo energético y latencia predecible.

> [!IMPORTANT]
> **Principio Operativo *Fail-Safe* (Apoyo a Decisión):**  
> PARADIXE SOFT no sustituye a **COER**, **Defensa Civil** ni personal médico, ni emite órdenes vinculantes de tránsito. Cuando faltan datos climáticos vigentes (`STALE_IVV`), no existen atributos físicos validados en campo (`UNVERIFIED_PHYSICAL_ATTRIBUTES`) o el riesgo impide una ruta verificable, el sistema responde explícitamente **`NO_SAFE_ROUTE`** con causa diagnóstica única en lugar de improvisar rutas peligrosas.

---

## Arquitectura de Producción (Opción A)

```mermaid
flowchart TD
    ACTORS["Actores Territoriales (Conductores, Salud/SAMU, Defensa Civil, COER)"]
    WEB["apps/web (PWA TypeScript + React + MapLibre GL JS + PMTiles + IndexedDB)"]
    MSG["Mensajería Multicanal (WhatsApp Cloud API + SMS + Webhooks HMAC)"]
    API["Backend API Rust (Axum + SQLx + OpenAPI 3.1 + RBAC/JWT)"]
    ENG["Motores CLI en Rust (ivv, consensus, routing, impact + paradixe-domain)"]
    PG["PostgreSQL 16 + PostGIS 3.4 (27 Tablas 3NF ::geography + sync_control)"]
    S3["Almacenamiento S3 / Cloudflare R2 / RustFS (Fotos + PMTiles)"]

    ACTORS --> WEB
    ACTORS --> MSG
    WEB -- "HTTPS /api/v1 (JWT + Sync Transaccional)" --> API
    WEB -- "Mapas Vectoriales Offline PMTiles" --> S3
    MSG -- "X-Webhook-Signature (HMAC + Anti-Replay)" --> API
    API --> ENG
    API -- "SQLx Verificado (Roles Prod vs Simulacros RO)" --> PG
    API -- "URLs Prefirmadas" --> S3
```

### Capacidades por Capa

| Capa / Módulo | Responsabilidad Clave | Garantía Técnica |
| :--- | :--- | :--- |
| **Capa 1 — Predicción Geotécnica ($IVV$)** | Calcula cada 3 horas el Índice Dinámico de Vulnerabilidad Vial (`GREEN`, `AMBER`, `RED`) cruzando precipitación 24–72 h, pendiente máxima, distancia a quebrada, suelo y puntos críticos. | Precisión `rust_decimal` con redondeo `MidpointAwayFromZero` idéntico a `NUMERIC` de PostgreSQL; si vence (`> 3 h`) pasa a `UNKNOWN` / `is_stale`. |
| **Capa 2 — Detección Híbrida y Privacidad** | Separa el estado de verificación (`SUSPECTED`, `CONFIRMED`, `DISMISSED`, `RESOLVED`) de la transitabilidad (`OPEN`, `CAUTION_SUSPECTED`, `PARTIAL_ONE_LANE`, `BLOCKED`). | Telemetría GPS con `client_ephemeral_id` rotado diariamente + `HMAC-SHA256` (clave fuera de BD, purga `24 h`); consenso comunitario acotado por proximidad `::geography <= 500 m`. |
| **Capa 3 — Enrutamiento y Puente Logístico** | Grafo dirigido bidireccional (`ST_Reverse`, pendientes ida/vuelta), poda conservadora por vehículo, ventanas humanitarias direccionales y propuesta de corredores de transbordo. | Todo puente nace `PROPOSED`; exige inspección vigente, ausencia de peligro (`NOT ST_DWithin`), idoneidad por carga y aprobación humana explícita. |
| **Módulo de Simulacros COER** | Evaluación aislada de escenarios estacionales (`WEATHER_OVERRIDE`, `BLOCKAGE_OVERRIDE`) con cálculo de tiempo, $\text{CO}_2$ y merma agrícola. | Principal PostgreSQL de solo lectura sobre tablas operativas; `NO_SAFE_ROUTE` fija deltas en `null` y preserva deltas negativos de $\text{CO}_2$ sin truncar signos. |

---

## Motores CLI en Rust

Siguiendo los **Artículos I y II SDD (*Library-First & CLI*)**, cada motor del núcleo es una librería independiente sin `#![allow(unsafe_code)]` (`#![forbid(unsafe_code)]`) acompañada de un binario CLI JSON determinista (`stdin`/`stdout`):

| Crate | Propósito | Cobertura Clave |
| :--- | :--- | :--- |
| [`paradixe-domain`](./crates/paradixe-domain) | Tipos algebraicos, enums *Fail-Safe*, precedencia de los 11 `reason_code` y redondeo `MidpointAwayFromZero`. | `RoutingResolution`, `SimEventRequest`, `NoSafeRouteReasonCode` |
| [`paradixe-ivv`](./crates/paradixe-ivv) | Cálculo determinista del índice $IVV$ y control de caducidad (`IVV-01`, `IVV-02`). | `100.00`, `3.00`, tramo Carpish `84.00 RED`, expiración `3 h` |
| [`paradixe-consensus`](./crates/paradixe-consensus) | Hash seudónimo HMAC diario, clúster GPS (`SUSPECTED` únicamente) y consenso comunitario (`CON-01`). | Deduplicación `30 min`, apoyo $S \ge 1.00$, margen $\ge 0.50$ |
| [`paradixe-routing`](./crates/paradixe-routing) | Enrutador bidireccional Dijkstra, ventanas humanitarias y Puente Logístico (`RTE-01..04`, `BRG-01..03`). | Poda vehicular, ventanas `[entry, exit]`, precedencia diagnóstica |
| [`paradixe-impact`](./crates/paradixe-impact) | Evaluador reproducible de tiempo, $\text{CO}_2$ y merma agrícola (`SIM-01`, `SIM-02`, `IMP-01`). | `null` en `NO_SAFE_ROUTE`, deltas negativos preservados |
| [`paradixe-api`](./crates/paradixe-api) | API Core Axum + SQLx (endpoints `/api/v1`, JWT/Argon2id/OTP, `sync_control`, certificación auditada, simulacros RO). | `SEC-01..07`, `GEO-01..03`, `SYN-01..04`, `E2E-01` sobre PostGIS |
| [`paradixe-ingestor`](./crates/paradixe-ingestor) | Ingestor multi-región (`#![forbid(unsafe_code)]`): OSM PBF Geofabrik, NASA SRTM 30m, Open-Meteo IVV cada 2 h y `PMTiles v3` en RustFS/S3. | `PE-HUC` (11 provincias y distritos), `PE-PAS`, `PE-JUN`, `PE-UCA`, `PE-SAM`, `PE-ANC`, `PE-ALL`, `--bbox` |
| [`paradixe-messaging`](./crates/paradixe-messaging) | Pasarela multicanal Etapa 3 (`#![forbid(unsafe_code)]`): WhatsApp Cloud API + fallback SMS + parser ciudadano + puente HMAC hacia `paradixe-api`. | Webhooks `GET/POST /webhooks/whatsapp`, `POST /webhooks/sms`, parser ES-PE |

### Ejemplo Ejecutable (`paradixe-ivv` CLI)

```bash
cargo run -q -p paradixe-ivv << 'JSON'
{
  "model": {
    "version_tag": "ivv-v1.0-initial",
    "w_precip": 0.350,
    "w_slope_ravine": 0.300,
    "w_soil": 0.200,
    "w_hotspot": 0.150,
    "precip_24h_cap_mm": 80,
    "precip_72h_cap_mm": 180,
    "slope_cap_pct": 45,
    "ravine_max_dist_m": 500,
    "amber_threshold": 40,
    "red_threshold": 70,
    "validity_hours": 3
  },
  "segment": {
    "max_slope_forward_pct": 27.0,
    "soil_susceptibility_idx": 0.80,
    "distance_to_ravine_m": 200,
    "is_historical_hotspot": true
  },
  "observation": {
    "precip_24h_mm": 85.0,
    "precip_72h_mm": 190.0,
    "observed_at": "2026-11-21T08:00:00Z"
  }
}
JSON
```

Salida determinista esperada: `"ivv_score": "84.00"`, `"risk_level": "RED"`.

---

## Estructura del Repositorio

| Ruta | Descripción |
| :--- | :--- |
| [`crates/`](./crates) | Cargo Workspace en Rust (`paradixe-domain`, `paradixe-ivv`, `paradixe-consensus`, `paradixe-routing`, `paradixe-impact`, `paradixe-api`, `paradixe-ingestor`) |
| [`apps/web/`](./apps/web) | PWA Offline-First en React + TypeScript + MapLibre GL JS + PMTiles + IndexedDB |
| [`openapi/openapi-v1.json`](./openapi/openapi-v1.json) | Contrato OpenAPI 3.1.0 canónico (`/api/v1`, 22 rutas y esquemas discriminados) |
| [`migrations/0001_initial_schema.sql`](./migrations/0001_initial_schema.sql) | Esquema PostgreSQL 16 + PostGIS 3.4 (27 tablas 3NF, índices `GiST ::geography`, `CHECK`s) |
| [`migrations/0002_seed_base_catalogs.sql`](./migrations/0002_seed_base_catalogs.sql) | Semillas idempotentes (`sync_control`, roles RBAC, categorías vehiculares, versiones base) |
| [`migrations/0003_territorial_regions_and_osm_lineage.sql`](./migrations/0003_territorial_regions_and_osm_lineage.sql) | Unidades territoriales (`territorial_units`), hidrografía (`hydrography_waterways`) y trazabilidad OSM |
| [`docker-compose.yml`](./docker-compose.yml) | Orquestación local de PostGIS 16 y RustFS S3 en Rust (puerto 9000 / consola 9001) |
| [`.github/workflows/ci.yml`](./.github/workflows/ci.yml) | Pipeline CI en GitHub Actions (Rust fmt/clippy/test, PWA Vite/Vitest y migraciones PostGIS) |
| [`docs/PARADIXE_SOFT.md`](./docs/PARADIXE_SOFT.md) | Especificación técnica ejecutable completa (`2.0.0-prod-sdd-rust`) |
| [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md) | Checklist SDD, PostGIS `::geography`, privacidad y pruebas para Pull Requests |
| [`.agents/skills/modern-web-guidance/SKILL.md`](./.agents/skills/modern-web-guidance/SKILL.md) | Skill de estándares web modernos vinculada al desarrollo frontend |

---

## Inicio Rápido

### 1. Requisitos del Sistema
* **Rust** (`rustup` / `rustc` / `cargo` stable con `rustfmt` y `clippy`)
* **Node.js** ($\ge 22$) y **npm**
* **Docker** y **Docker Compose** (opcional en local para PostgreSQL + PostGIS y RustFS)

En Arch / CachyOS Linux:
```bash
sudo pacman -S --needed docker docker-compose postgresql-libs postgresql nodejs npm
sudo systemctl enable --now docker
sudo usermod -aG docker "$USER"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --component rustfmt,clippy
```

### 2. Verificación del Núcleo Rust
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### 3. Verificación de la PWA Offline-First
```bash
cd apps/web
npm install
npm run gen:api
npm run typecheck && npm test && npm run build
```

### 4. Base de Datos Espacial, Migraciones y Pruebas de Integración PostGIS
```bash
cp .env.example .env
docker compose up -d
./scripts/bootstrap_dev_db.sh
cargo test -p paradixe-api --test integration_postgis -- --ignored --test-threads=1
```

### 5. Ingesta Territorial Multi-Región (OSM, NASA SRTM 30m, Open-Meteo IVV y PMTiles v3)
```bash
# Ingesta completa de Huánuco (`PE-HUC`, 11 provincias y distritos) y publicación en RustFS
cargo run --release -p paradixe-ingestor -- sync-all --region PE-HUC --download

# Escalamiento a otros departamentos (`PE-PAS`, `PE-JUN`, `PE-UCA`, `PE-SAM`, `PE-ANC`, `PE-ALL`) o bounding box arbitrario
cargo run --release -p paradixe-ingestor -- sync-all --region PE-PAS
cargo run --release -p paradixe-ingestor -- sync-all --region PE-CUSTOM --bbox "-77.2,-10.4,-74.5,-8.2" --region-name "Macro-Región Centro"

# Refresco periódico de lluvia 24h/72h e IVV determinista cada 2 horas
./scripts/run_weather_daemon.sh PE-HUC 2
```

### 6. Consola Web en Vivo y Pasarela WhatsApp/SMS (API + RustFS PMTiles + PWA)
```bash
# 1. Crear usuarios operadores locales (COER_ADMIN, CIVIL_DEFENSE, HEALTH_OPERATOR)
cargo run -p paradixe-api --bin seed_dev_users

# 2. Levantar API Core en http://127.0.0.1:8080
cargo run -p paradixe-api --bin paradixe-api

# 3. Levantar pasarela WhatsApp Cloud API + SMS en http://127.0.0.1:8090
cargo run -p paradixe-messaging -- serve

# 4. Levantar PWA con proxy hacia /api y /tiles (http://localhost:5173)
cd apps/web && npm install && npm run dev
```

---

## Hoja de Ruta de Producción

- [x] **Etapa 0 — Arquitectura SDD y Base de Datos:** Especificación `2.0.0-prod-sdd-rust`, esquema PostgreSQL 16 + PostGIS 3.4 (27 tablas 3NF), semillas base e infraestructura Docker (PostGIS + RustFS).
- [x] **Etapa 1A — Contrato OpenAPI 3.1 y Motores CLI en Rust:** `openapi-v1.json`, Cargo Workspace con `#![forbid(unsafe_code)]`, precisión `MidpointAwayFromZero` y suite de pruebas unitarias SDD.
- [x] **Etapa 1B — PWA Offline-First (`apps/web`):** MapLibre GL JS + PMTiles, IndexedDB atómica (`sync_control` + `outbox_queue`), rotación diaria de ID GPS, validación accesible y CI en GitHub Actions.
- [x] **Etapa 2A — API Core (`paradixe-api`):** Servidor Axum/SQLx (`/api/v1`), roles RBAC, certificación física auditada, rol `paradixe_sim_ro` de solo lectura operativa, `sync_control` transaccional y suite de integración sobre PostgreSQL 16 + PostGIS 3.4 real.
- [x] **Etapa 2B — Ingestor Real Multi-Región (`paradixe-ingestor`):** Worker escalable de ingesta OSM Geofabrik + NASA SRTM 30m + clima `OPEN_METEO` + generación y publicación de archivos `PMTiles v3` regionales sobre RustFS/S3 (activo sobre todo Huánuco `PE-HUC`).
- [x] **Etapa 3 — Mensajería Multicanal (`paradixe-messaging`):** Adaptadores de producción para WhatsApp Cloud API y fallback SMS con parser ciudadano en español y firmas HMAC constantes.
- [ ] **Etapa 4 — Piloto Operativo B2B/B2G:** Despliegue controlado en Huánuco–Carpish–Tingo María con operadores logísticos, gremios agrícolas, COER y redes de salud.

---

<div align="center">
  <strong>TakumiSoft (TAKUMISOFT)</strong> · Ingeniería de resiliencia territorial sostenible
</div>
