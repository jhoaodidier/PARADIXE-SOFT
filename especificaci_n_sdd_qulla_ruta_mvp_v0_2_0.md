# `spec.md`: QULLA-RUTA — Red Territorial de Resiliencia Vial y Logística Crítica (MVP)

**Versión:** `0.2.0-sdd` *(Revisión de Corrección Integral P0/P1)*  
**Metodología:** Specification-Driven Development (SDD) — *La especificación es la fuente de verdad ejecutable*.  
**Alcance Territorial MVP:** Corredor Vial Central y Sierra-Selva (Huánuco – Carpish – Tingo María y rutas alternas/vecinales).

---

## 1. Objetivo

Establecer la especificación técnica ejecutable, determinista y libre de ambigüedades para el MVP de **QULLA-RUTA**, un sistema de movilidad resiliente orientado al transporte multimodal existente (ambulancias rurales, camiones agrícolas, colectivos y motocicletas) bajo operación **Offline-First** y de bajo ancho de banda.

El sistema articula tres capas operativas y un entorno de simulación estrictamente aislado:
1. **Capa 1 — Predicción Geotécnica Reproducible ($IVV$):** Cálculo versionado y determinista cada 3 horas del Índice Dinámico de Vulnerabilidad Vial por tramo combinando lluvia acumulada (24–72 h), topografía, suelo y puntos críticos históricos.
2. **Capa 2 — Detección Híbrida con Estados Separados:** Separación explícita entre detección preliminar (`SUSPECTED`), confirmación operativa (`CONFIRMED`) y transitabilidad efectiva, integrando telemetría pasiva anónima y consenso comunitario matemáticamente acotado.
3. **Capa 3 — Enrutamiento Resiliente, Puente Logístico Verificado y Corredor Humanitario:** Ruteo sobre grafo con geometría espacial verificada filtrado por capacidad vehicular, transbordo únicamente sobre corredores peatonales/relevo validados como seguros (o respuesta explícita `NO_SAFE_ROUTE`) y ventanas de paso prioritario.
4. **Entorno de Simulación Aislado (Demo WFEO):** Ejecución de escenarios hipotéticos (lluvia intensa y derrumbe en Carpish) con barrera estricta que impide contaminar datos de producción o emitir alertas reales, evaluando métricas de impacto que admiten tanto ahorros como costos adicionales.

### Matriz de Resolución de Hallazgos (Auditoría `0.1.0-sdd` $\rightarrow$ `0.2.0-sdd`)

| Prioridad | Hallazgo Resuelto | Mecanismo Arquitectónico Aplicado |
| :--- | :--- | :--- |
| **P0** | Geometría vial ausente | `road_segments.geom` (`LINESTRING` SRID 4326), dirección explícita y regla de conectividad espacial ($\le 15\text{ m}$ a nodos extremos). |
| **P0** | Consenso matemáticamente inconsistente | Deduplicación por usuario, peso acotado, balance neto frente a reportes contradictorios, mínimo de 2 actores independientes y *override* oficial. |
| **P0** | Sin estado sospechoso | Separación estricta entre `incidents.verification_state` (`SUSPECTED`/`CONFIRMED`/`DISMISSED`/`RESOLVED`) y transitabilidad derivada (`OPEN`/`CAUTION_SUSPECTED`/`PARTIAL_ONE_LANE`/`BLOCKED`). |
| **P0** | Puente logístico sin garantía de seguridad | Entidad `transfer_corridors` con geometría validada en campo, libre de intersección con la zona de peligro; fallback obligatorio `NO_SAFE_ROUTE`. |
| **P0** | Contaminación del simulador en producción | Tablas aisladas por `scenario_id` + *Notification Firewall* que bloquea cualquier despacho real (SMS/WhatsApp/Push) en contexto `SIMULATION`. |
| **P1** | Fórmula $IVV$ no reproducible | Entidad `ivv_model_versions` con pesos, topes de normalización, umbrales y referencia histórica obligatoria en cada evaluación. |
| **P1** | Offline-First incompleto | Secuencia monotónica `sync_version`, almacenamiento local + `outbox_queue`, resolución *Server-Authoritative* y bandera `STALE_DATA` ($> 3\text{ h}$). |
| **P1** | Inconsistencia 3NF | Eliminación de columnas derivadas (`valid_until`, `operational_status` redundante, porcentajes almacenados); justificación de snapshots inmutables. |
| **P1** | Métricas asumen beneficio positivo | Fórmulas versionadas (`impact_parameter_versions`) con soporte explícito para deltas negativos, nulos o positivos ($\Delta \in \mathbb{R}$). |
| **P1** | Identidad, GPS y tokens incompletos | Rotación/revocación de refresh tokens (`auth_refresh_tokens`), `otp_challenges`, salt rotativo diario en GPS (retención 24 h) y `audit_logs`. |

### Compuertas Constitucionales SDD (Fase -1 Pre-Implementación)
* **Artículos I y II (Library-First y CLI):** Los motores de $IVV$, consenso, enrutamiento espacial y cálculo de impacto se implementan como librerías independientes con interfaz CLI JSON (`stdin`/`stdout`).
* **Artículos III y IX (Test-First e Integration-First):** Todo desarrollo inicia con tests de contrato e integración sobre base de datos real (con soporte espacial PostGIS) en fase roja antes de escribir código de producción.
* **Artículos VII y VIII (Simplicidad y Anti-Abstracción):** Máximo 3 componentes desplegables (API Core + Motores, Cliente Web/Mapa, Adaptador Mensajería/Sync) sin abstracciones especulativas.

---

## 2. Esquema de Datos (3NF Estricto)

El modelo relacional cumple con la **Tercera Forma Normal (3NF)**:
* **1NF:** Todos los atributos son atómicos (incluidas geometrías espaciales estándar OGC `POINT`/`LINESTRING` en SRID 4326).
* **2NF:** Toda columna no clave depende de la clave primaria completa.
* **3NF:** Se eliminan todas las dependencias transitivas y atributos calculables:
  * No se almacena `risk_level` ni `valid_until` en `ivv_assessments` (se derivan de `ivv_score`, `calculated_at` y `ivv_model_versions`).
  * No se almacena estado de transitabilidad duplicado en `road_segments` (se deriva de los `incidents` vigentes; cualquier vista materializada es estrictamente una proyección de lectura).
  * No se almacenan deltas ni porcentajes en `simulation_impact_logs` (se derivan de los valores atómicos `baseline_*` y `mitigated_*`).
  * **Justificación de Auditoría Histórica:** `ivv_assessments` y `simulation_impact_logs` conservan claves foráneas a versiones inmutables (`ivv_model_versions`, `impact_parameter_versions`), garantizando reproducibilidad histórica sin introducir redundancia transitiva.

### 2.1 Identidad, Autenticación, Seguridad y Auditoría

#### `roles`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `role_id` | UUID | PK | Identificador del rol. |
| `code` | VARCHAR(32) | UNIQUE, NOT NULL | `DRIVER`, `HEALTH_OPERATOR`, `CIVIL_DEFENSE`, `COER_ADMIN`, `BOT_SERVICE`. |
| `base_trust_weight` | DECIMAL(3,2) | NOT NULL ($0.10\text{--}1.00$) | Peso base de credibilidad comunitaria. |
| `has_authoritative_override` | BOOLEAN | NOT NULL | `TRUE` solo para `COER_ADMIN` y `CIVIL_DEFENSE`. |

#### `users`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `user_id` | UUID | PK | Identificador del usuario. |
| `role_id` | UUID | FK $\rightarrow$ `roles(role_id)`, NOT NULL | Rol asignado. |
| `phone_e164` | VARCHAR(20) | UNIQUE, NOT NULL | Teléfono internacional (identificador principal para App y Bot). |
| `full_name` | VARCHAR(120) | NOT NULL | Nombre completo. |
| `password_hash` | VARCHAR(255) | NULLABLE | Hash Argon2id/bcrypt (nulo si solo autentica vía OTP). |
| `reputation_score` | DECIMAL(4,2) | NOT NULL, DEFAULT 0.50 | Reputación acotada $[0.10, 1.00]$. |
| `is_active` | BOOLEAN | NOT NULL, DEFAULT TRUE | Estado de habilitación. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de creación. |

#### `auth_refresh_tokens`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `token_id` | UUID | PK | Identificador del token. |
| `token_family_id` | UUID | NOT NULL | Familia de sesión para detección de reuso (*Reuse Detection*). |
| `user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Usuario propietario. |
| `token_hash` | VARCHAR(64) | UNIQUE, NOT NULL | Hash SHA-256 del refresh token opaco. |
| `device_id` | VARCHAR(128) | NOT NULL | Identificador lógico del dispositivo cliente. |
| `expires_at` | TIMESTAMPTZ | NOT NULL | Fecha de expiración. |
| `revoked_at` | TIMESTAMPTZ | NULLABLE | Marca de revocación explícita o por rotación. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de emisión. |

#### `otp_challenges`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `challenge_id` | UUID | PK | Identificador del desafío OTP. |
| `phone_e164` | VARCHAR(20) | NOT NULL | Teléfono destino. |
| `code_hash` | VARCHAR(64) | NOT NULL | Hash del código OTP de 6 dígitos. |
| `attempts` | SMALLINT | NOT NULL, DEFAULT 0 | Intentos fallidos (máximo 5). |
| `expires_at` | TIMESTAMPTZ | NOT NULL | Expiración estricta ($+5\text{ min}$). |
| `consumed_at` | TIMESTAMPTZ | NULLABLE | Marca de uso único. |

#### `audit_logs`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `audit_id` | UUID | PK | Registro inmutable de auditoría. |
| `actor_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NULLABLE | Usuario que ejecutó la acción (nulo si fue proceso de sistema). |
| `action` | VARCHAR(64) | NOT NULL | Ej. `INCIDENT_CONFIRMED`, `WINDOW_CREATED`, `TOKEN_FAMILY_REVOKED`. |
| `entity_type` | VARCHAR(48) | NOT NULL | Nombre de la entidad afectada. |
| `entity_id` | UUID | NOT NULL | Identificador del registro afectado. |
| `before_state_json` | JSONB | NULLABLE | Estado previo relevante. |
| `after_state_json` | JSONB | NOT NULL | Estado posterior resultante. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Marca de tiempo UTC. |

### 2.2 Catálogo Vehicular y Topología Vial Espacial

#### `vehicle_categories`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `category_id` | UUID | PK | Categoría vehicular. |
| `code` | VARCHAR(32) | UNIQUE, NOT NULL | `AMBULANCE_4X4`, `PICKUP_4X4`, `LIGHT_TRUCK`, `HEAVY_TRAILER`, `MOTORCYCLE`. |
| `max_slope_pct` | DECIMAL(4,1) | NOT NULL | Pendiente máxima transitable (%). |
| `min_road_width_m` | DECIMAL(4,2) | NOT NULL | Ancho mínimo de calzada requerido (m). |
| `max_weight_tons` | DECIMAL(5,2) | NOT NULL | Peso bruto vehicular estándar (t). |
| `allows_dirt_track` | BOOLEAN | NOT NULL | Permite tránsito por trocha/afirmado. |
| `priority_tier` | SMALLINT | NOT NULL ($1\text{--}5$) | Prioridad en corredor humanitario (`1` = máxima). |

#### `road_nodes`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `node_id` | UUID | PK | Nodo del grafo territorial. |
| `name` | VARCHAR(120) | NOT NULL | Nombre del punto (ej. *Túnel Carpish Sur*, *Hospital Hermilio Valdizán*). |
| `node_type` | VARCHAR(32) | NOT NULL | `INTERSECTION`, `TOWN_CENTER`, `HOSPITAL`, `SAFE_TRANSFER_ZONE`, `TUNNEL_PORTAL`. |
| `location_geom` | GEOMETRY(POINT, 4326) | NOT NULL, GIST INDEX | Coordenada geoespacial WGS84. |
| `elevation_m` | INTEGER | NOT NULL | Altitud sobre el nivel del mar (DEM/SRTM). |
| `is_geotechnically_stable` | BOOLEAN | NOT NULL | Apto como zona segura de espera/transbordo. |
| `urban_radius_m` | INTEGER | NOT NULL, DEFAULT 0 | Radio urbano de exclusión para telemetría pasiva. |

#### `road_segments`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `segment_id` | UUID | PK | Arista dirigida del grafo vial. |
| `origin_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo de inicio. |
| `destination_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo de fin (Check: `origin_node_id <> destination_node_id`). |
| `corridor_name` | VARCHAR(100) | NOT NULL | Corredor al que pertenece. |
| `geom` | GEOMETRY(LINESTRING, 4326) | NOT NULL, GIST INDEX | Trazado geométrico real del tramo en sentido origen $\rightarrow$ destino. |
| `direction_type` | VARCHAR(16) | NOT NULL | `ONE_WAY` o `BIDIRECTIONAL`. |
| `length_km` | DECIMAL(6,2) | NOT NULL ($> 0$) | Longitud geodésica del tramo. |
| `avg_slope_pct` | DECIMAL(4,1) | NOT NULL ($\ge 0$) | Pendiente promedio (DEM). |
| `max_slope_pct` | DECIMAL(4,1) | NOT NULL ($\ge$ `avg_slope_pct`) | Pendiente máxima del tramo. |
| `road_width_m` | DECIMAL(4,2) | NOT NULL ($> 0$) | Ancho efectivo de calzada. |
| `max_supported_tons` | DECIMAL(5,2) | NOT NULL ($> 0$) | Capacidad portante máxima. |
| `surface_type` | VARCHAR(20) | NOT NULL | `PAVED`, `GRAVEL`, `DIRT_TRACK`. |
| `soil_susceptibility_idx` | DECIMAL(3,2) | NOT NULL ($0.00\text{--}1.00$) | Factor geotécnico del suelo. |
| `distance_to_ravine_m` | INTEGER | NOT NULL ($\ge 0$) | Distancia mínima a quebrada activa. |
| `is_historical_hotspot` | BOOLEAN | NOT NULL | Punto crítico histórico (Provías). |
| `base_travel_time_min` | DECIMAL(6,2) | NOT NULL ($> 0$) | Tiempo base en condiciones normales. |
| `is_topology_verified` | BOOLEAN | NOT NULL, DEFAULT FALSE | `TRUE` solo tras validar conectividad espacial de `geom` con origen/destino ($\le 15\text{ m}$). |
| `sync_version` | BIGINT | NOT NULL | Secuencia global monotónica para sincronización diferencial. |

#### `transfer_corridors` *(Caminos de Transbordo Verificados)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `corridor_id` | UUID | PK | Sendero/vía de transbordo validada entre dos nodos seguros. |
| `side_a_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo extremo A (`is_geotechnically_stable = TRUE`). |
| `side_b_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo extremo B (`is_geotechnically_stable = TRUE`, $\ne$ A). |
| `path_geom` | GEOMETRY(LINESTRING, 4326) | NOT NULL, GIST INDEX | Geometría real del paso peatonal o de relevo. |
| `length_m` | INTEGER | NOT NULL ($> 0$) | Longitud del cruce de transbordo. |
| `max_slope_pct` | DECIMAL(4,1) | NOT NULL | Pendiente máxima del sendero. |
| `transfer_mode` | VARCHAR(32) | NOT NULL | `PEDESTRIAN_SAFE_PATH`, `SHUTTLE_RELAY`. |
| `max_safe_ivv` | DECIMAL(5,2) | NOT NULL | Umbral máximo de $IVV$ bajo el cual el sendero sigue siendo seguro. |
| `is_verified_safe` | BOOLEAN | NOT NULL | Validación física en campo por Defensa Civil/COER. |
| `last_inspected_at` | TIMESTAMPTZ | NOT NULL | Fecha de última inspección geotécnica. |

### 2.3 Capa 1: Clima y Modelo Versionado de Riesgo ($IVV$)

#### `ivv_model_versions`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `model_version_id` | UUID | PK | Identificador de versión del modelo $IVV$. |
| `version_tag` | VARCHAR(32) | UNIQUE, NOT NULL | Ej. `ivv-v1.0-rainy-andes`. |
| `w_precip` | DECIMAL(4,3) | NOT NULL | Peso $w_1$ de precipitación. |
| `w_slope_ravine` | DECIMAL(4,3) | NOT NULL | Peso $w_2$ de pendiente y quebrada. |
| `w_soil` | DECIMAL(4,3) | NOT NULL | Peso $w_3$ de suelo. |
| `w_hotspot` | DECIMAL(4,3) | NOT NULL | Peso $w_4$ de punto crítico (Check: $w_1+w_2+w_3+w_4 = 1.000$). |
| `precip_24h_cap_mm` | DECIMAL(6,2) | NOT NULL ($> 0$) | Tope de saturación 24 h para normalización (ej. `80.00`). |
| `precip_72h_cap_mm` | DECIMAL(6,2) | NOT NULL ($> 0$) | Tope de saturación 72 h para normalización (ej. `180.00`). |
| `slope_cap_pct` | DECIMAL(4,1) | NOT NULL ($> 0$) | Tope de pendiente para normalización (ej. `45.0`). |
| `ravine_max_dist_m` | INTEGER | NOT NULL ($> 0$) | Distancia a partir de la cual el factor quebrada es 0 (ej. `500`). |
| `amber_threshold` | DECIMAL(5,2) | NOT NULL | Umbral inferior de `AMBER` (ej. `40.00`). |
| `red_threshold` | DECIMAL(5,2) | NOT NULL | Umbral inferior de `RED` (ej. `70.00`, Check: `red > amber`). |
| `validity_hours` | SMALLINT | NOT NULL | Horas de validez del cálculo (estándar: `3`). |
| `is_active` | BOOLEAN | NOT NULL | Solo una versión activa simultáneamente. |

#### `weather_observations`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `observation_id` | UUID | PK | Observación climática de producción. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo evaluado. |
| `source_name` | VARCHAR(32) | NOT NULL | `SENAMHI`, `NASA_GPM`, `LOCAL_STATION` *(se prohíbe `SIMULATOR`)*. |
| `precip_24h_mm` | DECIMAL(6,2) | NOT NULL ($\ge 0$) | Lluvia acumulada 24 h. |
| `precip_72h_mm` | DECIMAL(6,2) | NOT NULL ($\ge$ `precip_24h_mm`) | Lluvia acumulada 72 h. |
| `observed_at` | TIMESTAMPTZ | NOT NULL | Hora de observación. |

#### `ivv_assessments`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `assessment_id` | UUID | PK | Evaluación histórica inmutable de $IVV$. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo evaluado. |
| `observation_id` | UUID | FK $\rightarrow$ `weather_observations(observation_id)`, NOT NULL | Insumo climático exacto. |
| `model_version_id` | UUID | FK $\rightarrow$ `ivv_model_versions(model_version_id)`, NOT NULL | Versión exacta de fórmula aplicada. |
| `ivv_score` | DECIMAL(5,2) | NOT NULL ($0.00\text{--}100.00$) | Puntaje calculado. |
| `calculated_at` | TIMESTAMPTZ | NOT NULL | Timestamp de cálculo. |
| `sync_version` | BIGINT | NOT NULL | Secuencia monotónica para sincronización diferencial. |

### 2.4 Capa 2: Detección Híbrida, Incidentes y Privacidad GPS

#### `telemetry_pings` *(Retención máxima: 24 h)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `ping_id` | UUID | PK | Pulso GPS anónimo. |
| `anonymous_device_hash` | VARCHAR(64) | NOT NULL | `HMAC-SHA256(daily_salt, device_id)` (sin vínculo a `users`). |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo asignado por proyección espacial a `road_segments.geom`. |
| `location_geom` | GEOMETRY(POINT, 4326) | NOT NULL | Posición capturada. |
| `speed_kmh` | DECIMAL(5,2) | NOT NULL ($\ge 0$) | Velocidad instantánea. |
| `recorded_at` | TIMESTAMPTZ | NOT NULL | Hora de captura en dispositivo. |
| `synced_at` | TIMESTAMPTZ | NOT NULL | Hora de ingesta en servidor. |

#### `field_reports`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `report_id` | UUID | PK | Identificador en servidor. |
| `client_uuid` | UUID | UNIQUE, NOT NULL | Clave de idempotencia offline. |
| `reporter_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Informante. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo reportado. |
| `proposed_passability` | VARCHAR(24) | NOT NULL | `BLOCKED`, `PARTIAL_ONE_LANE`, `OPEN`. |
| `location_geom` | GEOMETRY(POINT, 4326) | NOT NULL | Posición capturada offline. |
| `photo_uri` | VARCHAR(512) | NULLABLE | Evidencia fotográfica. |
| `channel` | VARCHAR(20) | NOT NULL | `MOBILE_APP`, `WHATSAPP_BOT`, `SMS`. |
| `validation_status` | VARCHAR(20) | NOT NULL | `PENDING`, `VERIFIED`, `REJECTED_DISTANCE`, `REJECTED_CONTRADICTED`, `STALE_ARCHIVED`. |
| `captured_at_client` | TIMESTAMPTZ | NOT NULL | Timestamp del dispositivo. |
| `clock_skew_flagged` | BOOLEAN | NOT NULL, DEFAULT FALSE | `TRUE` si `captured_at_client > synced_at_server + 5 min`. |
| `synced_at_server` | TIMESTAMPTZ | NOT NULL | Timestamp de sincronización. |

#### `incidents` *(Solo Operaciones Reales)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `incident_id` | UUID | PK | Incidente vial real. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo afectado. |
| `trigger_source` | VARCHAR(32) | NOT NULL | `TELEMETRY_CLUSTER`, `FIELD_CONSENSUS`, `AUTHORITY_DIRECT`. |
| `verification_state` | VARCHAR(24) | NOT NULL | `SUSPECTED`, `CONFIRMED`, `DISMISSED`, `RESOLVED`. |
| `impact_passability` | VARCHAR(24) | NOT NULL | `CAUTION_SUSPECTED`, `PARTIAL_ONE_LANE`, `BLOCKED`. |
| `hazard_location_geom` | GEOMETRY(POINT, 4326) | NOT NULL | Punto central del derrumbe/huaico. |
| `hazard_radius_m` | INTEGER | NOT NULL ($> 0$, DEFAULT 150) | Radio de exclusión geotécnica del peligro activo. |
| `detected_at` | TIMESTAMPTZ | NOT NULL | Inicio de detección. |
| `confirmed_at` | TIMESTAMPTZ | NULLABLE | Momento de paso a `CONFIRMED`. |
| `resolved_at` | TIMESTAMPTZ | NULLABLE | Momento de cierre (`RESOLVED` o `DISMISSED`). |
| `sync_version` | BIGINT | NOT NULL | Secuencia monotónica para sincronización diferencial. |

#### `incident_report_links`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `incident_id` | UUID | PK, FK $\rightarrow$ `incidents(incident_id)` | Incidente vinculado. |
| `report_id` | UUID | PK, FK $\rightarrow$ `field_reports(report_id)` | Reporte vinculado. |

### 2.5 Capa 3: Puente Logístico y Corredor Humanitario

#### `logistic_bridges`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `bridge_id` | UUID | PK | Operación activa de puente logístico. |
| `incident_id` | UUID | FK $\rightarrow$ `incidents(incident_id)`, NOT NULL | Incidente `CONFIRMED` asociado. |
| `corridor_id` | UUID | FK $\rightarrow$ `transfer_corridors(corridor_id)`, NOT NULL | Camino de transbordo verificado como seguro. |
| `cargo_type` | VARCHAR(32) | NOT NULL | `MEDICAL_PATIENT`, `MEDICAL_SUPPLIES`, `PERISHABLE_AGRI`. |
| `status` | VARCHAR(20) | NOT NULL | `PROPOSED`, `ACTIVE`, `SUSPENDED_UNSAFE`, `COMPLETED`. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Timestamp de creación. |

#### `humanitarian_windows`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `window_id` | UUID | PK | Ventana de paso prioritario. |
| `incident_id` | UUID | FK $\rightarrow$ `incidents(incident_id)`, NOT NULL | Incidente `CONFIRMED` con `impact_passability = PARTIAL_ONE_LANE`. |
| `authorized_by_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Autoridad que aprueba. |
| `vehicle_category_id` | UUID | FK $\rightarrow$ `vehicle_categories(category_id)`, NOT NULL | Categoría habilitada. |
| `direction` | VARCHAR(16) | NOT NULL | `ORIGIN_TO_DEST`, `DEST_TO_ORIGIN`. |
| `starts_at` | TIMESTAMPTZ | NOT NULL | Inicio de ventana. |
| `ends_at` | TIMESTAMPTZ | NOT NULL | Fin de ventana (Check: `ends_at > starts_at`). |
| `status` | VARCHAR(20) | NOT NULL | `SCHEDULED`, `ACTIVE`, `CLOSED`. |

### 2.6 Entorno de Simulación Aislado (Sandbox Demo WFEO)

#### `simulation_scenarios`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `scenario_id` | UUID | PK | Escenario aislado de simulación. |
| `name` | VARCHAR(120) | NOT NULL | Ej. *Lluvia Extrema y Derrumbe Carpish*. |
| `created_by_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Operador creador. |
| `status` | VARCHAR(20) | NOT NULL | `DRAFT`, `RUNNING`, `COMPLETED`, `ARCHIVED`. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de creación. |

#### `sim_events`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `sim_event_id` | UUID | PK | Evento sintético dentro de un escenario. |
| `scenario_id` | UUID | FK $\rightarrow$ `simulation_scenarios(scenario_id)`, NOT NULL | Escenario propietario. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo simulado. |
| `event_type` | VARCHAR(24) | NOT NULL | `WEATHER_OVERRIDE`, `BLOCKAGE_OVERRIDE`. |
| `precip_24h_mm` | DECIMAL(6,2) | NULLABLE | Lluvia sintética 24 h (si `WEATHER_OVERRIDE`). |
| `precip_72h_mm` | DECIMAL(6,2) | NULLABLE | Lluvia sintética 72 h (si `WEATHER_OVERRIDE`). |
| `simulated_passability` | VARCHAR(24) | NULLABLE | `BLOCKED`, `PARTIAL_ONE_LANE` (si `BLOCKAGE_OVERRIDE`). |
| `hazard_location_geom` | GEOMETRY(POINT, 4326) | NULLABLE | Punto sintético de derrumbe. |
| `hazard_radius_m` | INTEGER | NULLABLE | Radio sintético de peligro. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Timestamp del evento. |

#### `impact_parameter_versions`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `param_version_id` | UUID | PK | Versión de parámetros económicos/ambientales. |
| `version_tag` | VARCHAR(32) | UNIQUE, NOT NULL | Ej. `impact-v1.0-huanuco-2026`. |
| `vehicle_category_id` | UUID | FK $\rightarrow$ `vehicle_categories(category_id)`, NOT NULL | Categoría a la que aplica. |
| `co2_idle_kg_per_hour` | DECIMAL(6,3) | NOT NULL ($> 0$) | Factor de emisión en ralentí ($\text{kg CO}_2/\text{h}$). |
| `co2_drive_kg_per_km` | DECIMAL(6,3) | NOT NULL ($> 0$) | Factor de emisión en circulación ($\text{kg CO}_2/\text{km}$). |
| `agri_cargo_value_usd` | DECIMAL(10,2) | NOT NULL ($\ge 0$) | Valor de carga perecible estándar (USD). |
| `perishability_loss_rate_per_hour` | DECIMAL(5,4) | NOT NULL ($0.0000\text{--}1.0000$) | Tasa horaria de merma tras superar umbral de espera. |
| `transfer_handling_loss_pct` | DECIMAL(4,3) | NOT NULL ($0.000\text{--}1.000$) | Merma fija por manipuleo en puente logístico. |

#### `simulation_impact_logs` *(Valores Atómicos; Deltas Derivados en Lectura)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `sim_log_id` | UUID | PK | Registro inmutable de corrida comparativa. |
| `scenario_id` | UUID | FK $\rightarrow$ `simulation_scenarios(scenario_id)`, NOT NULL | Escenario ejecutado. |
| `param_version_id` | UUID | FK $\rightarrow$ `impact_parameter_versions(param_version_id)`, NOT NULL | Parámetros usados. |
| `resolution_type` | VARCHAR(32) | NOT NULL | `ALTERNATE_ROUTE`, `LOGISTIC_BRIDGE_REQUIRED`, `NO_SAFE_ROUTE`. |
| `baseline_travel_min` | DECIMAL(7,2) | NOT NULL ($> 0$) | Tiempo total sin QULLA-RUTA (espera + tránsito). |
| `mitigated_travel_min` | DECIMAL(7,2) | NOT NULL ($> 0$) | Tiempo total con desvío o transbordo. |
| `baseline_co2_kg` | DECIMAL(8,2) | NOT NULL ($\ge 0$) | Emisiones totales en escenario base. |
| `mitigated_co2_kg` | DECIMAL(8,2) | NOT NULL ($\ge 0$) | Emisiones totales en escenario mitigado. |
| `baseline_cargo_loss_usd` | DECIMAL(10,2) | NOT NULL ($\ge 0$) | Pérdida agrícola estimada en escenario base. |
| `mitigated_cargo_loss_usd` | DECIMAL(10,2) | NOT NULL ($\ge 0$) | Pérdida agrícola estimada en escenario mitigado. |
| `computed_at` | TIMESTAMPTZ | NOT NULL | Timestamp del cálculo. |

---

## 3. Endpoints (Contrato de API)

**Convenciones Globales:**
* **Base URL:** `/api/v1`
* **Autenticación:** Bearer JWT de corta duración (15 min) en `Authorization: Bearer <token>`; firma `X-Webhook-Signature` (HMAC-SHA256 + timestamp anti-replay $\le 5\text{ min}$) para `BOT_SERVICE`.
* **Aislamiento de Contexto:** Las rutas de producción operan siempre en `PRODUCTION`. Las rutas `/simulator/*` operan exclusivamente en `SIMULATION` y requieren `scenario_id`.
* **Formato de Error Estándar:**
  ```json
  { "error_code": "STRING", "message": "STRING", "details": {} }
  ```

### 3.1 Identidad, Autenticación y Sesiones

| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/auth/otp/request` | Público (Rate-limited) | Emite desafío OTP (5 min, máx. 5 intentos) para `phone_e164`. |
| `POST` | `/auth/login` | Público | Autentica por `phone_e164` + (`password` u `otp_code`) + `device_id`; emite `access_token` y `refresh_token`. |
| `POST` | `/auth/refresh` | Refresh token válido | Rota `refresh_token` y revoca el anterior; ante reuso detectado revoca toda la `token_family_id`. |
| `POST` | `/auth/logout` | Autenticado | Revoca el `refresh_token` del dispositivo actual. |
| `POST` | `/auth/revoke-all` | Autenticado | Revoca todas las sesiones activas del usuario y registra en `audit_logs`. |

### 3.2 Sincronización Offline-First

| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `GET` | `/sync/snapshot` | Autenticado | Parámetro `?since_version=<BIGINT>` (0 para descarga inicial). Retorna deltas de nodos, tramos (`geom` GeoJSON simplificado), $IVV$ vigente, incidentes activos y `current_sync_version`. |
| `POST` | `/sync/batch` | Autenticado | Recibe cola `outbox_queue` (`reports` con `client_uuid` y `telemetry_pings`). Deduplica idempotentemente y retorna estado de cada ítem + `server_time`. |

### 3.3 Capa 1: Mapa Territorial e $IVV$

| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `GET` | `/corridors/map` | Público / Auth | Retorna FeatureCollection GeoJSON (`LINESTRING`) de `road_segments` verificados con `passability_state` derivado, último `ivv_score`, `risk_level` derivado (`GREEN`/`AMBER`/`RED`), `model_version_tag` y `is_stale` (si excede `validity_hours`). |
| `POST` | `/ivv/recalculate` | `COER_ADMIN`, `BOT_SERVICE` | Registra `weather_observations` y calcula `ivv_assessments` usando la versión activa de `ivv_model_versions`. |

### 3.4 Capa 2: Detección Híbrida e Incidentes

| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/telemetry/pings` | Autenticado | Ingesta lote de pings anónimos (`anonymous_device_hash`) con map-matching espacial contra `road_segments.geom`. |
| `POST` | `/reports` | Autenticado | Registra reporte individual con `client_uuid`, valida distancia a `geom` y ejecuta motor de consenso. |
| `POST` | `/webhooks/messaging` | HMAC (`BOT_SERVICE`) | Recibe reporte estructurado desde WhatsApp/SMS y devuelve confirmación ligera. |
| `PATCH` | `/incidents/{incident_id}/verification` | `COER_ADMIN`, `CIVIL_DEFENSE` | Transiciona incidente entre `SUSPECTED` $\rightarrow$ `CONFIRMED`/`DISMISSED` o `CONFIRMED` $\rightarrow$ `RESOLVED` y actualiza `impact_passability`; audita en `audit_logs`. |

### 3.5 Capa 3: Enrutamiento, Puente Logístico y Corredor Humanitario

| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/routing/calculate` | Autenticado | Calcula ruta óptima en producción (o en sandbox si se consulta vía `/simulator/scenarios/{id}/routing`). |
| `POST` | `/bridges` | `COER_ADMIN`, `HEALTH_OPERATOR`, `CIVIL_DEFENSE` | Activa un `logistic_bridge` vinculado a un `transfer_corridor` validado como seguro. |
| `POST` | `/incidents/{incident_id}/humanitarian-windows` | `COER_ADMIN`, `CIVIL_DEFENSE` | Crea ventana de paso prioritario en incidente `CONFIRMED` con `PARTIAL_ONE_LANE` y audita en `audit_logs`. |

**Contrato `POST /routing/calculate`:**
* **Request:**
  ```json
  {
    "origin_node_id": "uuid",
    "destination_node_id": "uuid",
    "vehicle_category_code": "AMBULANCE_4X4",
    "cargo_type": "MEDICAL_PATIENT"
  }
  ```
* **Response `200 OK` (Caso A — Ruta Alterna):**
  ```json
  {
    "resolution_type": "ALTERNATE_ROUTE",
    "total_distance_km": 142.5,
    "estimated_time_min": 195,
    "max_ivv_on_route": 48.2,
    "segments": ["uuid1", "uuid2"],
    "warnings": ["Tramo X en CAUTION_SUSPECTED"]
  }
  ```
* **Response `200 OK` (Caso B — Puente Logístico Verificado):**
  ```json
  {
    "resolution_type": "LOGISTIC_BRIDGE_REQUIRED",
    "blocking_incident_id": "uuid",
    "bridge_proposal": {
      "corridor_id": "uuid",
      "side_a_node_id": "uuid",
      "side_b_node_id": "uuid",
      "transfer_mode": "PEDESTRIAN_SAFE_PATH",
      "length_m": 450,
      "last_inspected_at": "2026-11-20T08:00:00Z"
    }
  }
  ```
* **Response `200 OK` (Caso C — Sin Cruce Seguro):**
  ```json
  {
    "resolution_type": "NO_SAFE_ROUTE",
    "blocking_incident_id": "uuid",
    "reason_code": "NO_VERIFIED_TRANSFER_CORRIDOR | HAZARD_INTERSECTS_TRANSFER_PATH | TRANSFER_IVV_EXCEEDED"
  }
  ```

### 3.6 Simulador Aislado (Sandbox WFEO)

| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/simulator/scenarios` | `COER_ADMIN` | Crea un `simulation_scenario` aislado. |
| `POST` | `/simulator/scenarios/{scenario_id}/events` | `COER_ADMIN` | Inyecta lluvia o derrumbe sintético en `sim_events`. **Nunca** muta tablas de producción ni invoca gateways SMS/WhatsApp. |
| `POST` | `/simulator/scenarios/{scenario_id}/evaluate` | `COER_ADMIN` | Ejecuta el motor de ruteo sobre el grafo superpuesto con `sim_events`, calcula métricas con `impact_parameter_versions` y persiste en `simulation_impact_logs`. |
| `GET` | `/simulator/scenarios/{scenario_id}/impact` | Autenticado | Retorna insumos atómicos y deltas derivados (`time_delta_min`, `time_saved_pct`, `net_agri_savings_usd`, `net_co2_delta_kg`), admitiendo valores negativos, cero o positivos. |

---

## 4. Lógica de Negocio

### 4.1 Validación Topológica del Grafo Vial
Un tramo en `road_segments` solo pasa a `is_topology_verified = TRUE` y entra al motor de ruteo si cumple:
1. `geom` es un `LINESTRING` válido (no auto-intersectante, longitud $> 0$).
2. $\text{ST\_Distance}(\text{ST\_StartPoint}(\text{geom}), \text{origin\_node.location\_geom}) \le 15\text{ m}$.
3. $\text{ST\_Distance}(\text{ST\_EndPoint}(\text{geom}), \text{destination\_node.location\_geom}) \le 15\text{ m}$.

### 4.2 Motor Predictivo $IVV$ Reproducible (Capa 1)
Usando los parámetros de la versión activa $M$ de `ivv_model_versions` y $\text{clamp}(x, 0, 1) = \min(1, \max(0, x))$:
1. **Precipitación ($P$):**
   $$P_{24} = \text{clamp}\left(\frac{\text{precip\_24h\_mm}}{M.\text{precip\_24h\_cap\_mm}}, 0, 1\right), \quad P_{72} = \text{clamp}\left(\frac{\text{precip\_72h\_mm}}{M.\text{precip\_72h\_cap\_mm}}, 0, 1\right)$$
   $$P = 0.6 \cdot P_{24} + 0.4 \cdot P_{72}$$
2. **Topografía y Quebrada ($S$):**
   $$S_{\text{slope}} = \text{clamp}\left(\frac{\text{max\_slope\_pct}}{M.\text{slope\_cap\_pct}}, 0, 1\right), \quad S_{\text{ravine}} = 1 - \text{clamp}\left(\frac{\text{distance\_to\_ravine\_m}}{M.\text{ravine\_max\_dist\_m}}, 0, 1\right)$$
   $$S = 0.5 \cdot S_{\text{slope}} + 0.5 \cdot S_{\text{ravine}}$$
3. **Suelo ($G$) e Histórico ($H$):** $G = \text{soil\_susceptibility\_idx} \in [0,1]$; $H = 1.0$ si `is_historical_hotspot = TRUE`, $0.2$ en caso contrario.
4. **Cálculo y Clasificación Derivada:**
   $$IVV = \text{round}\Big(100 \times (M.w_1 P + M.w_2 S + M.w_3 G + M.w_4 H),\ 2\Big)$$
   * `GREEN`: $IVV < M.\text{amber\_threshold}$
   * `AMBER`: $M.\text{amber\_threshold} \le IVV < M.\text{red\_threshold}$
   * `RED`: $IVV \ge M.\text{red\_threshold}$

### 4.3 Detección Híbrida, Consenso Matemático y Estados Separados (Capa 2)
1. **Telemetría Pasiva Anónima (Siempre `SUSPECTED`):**
   * Se proyecta cada ping sobre `road_segments.geom` (distancia máxima de asignación: $50\text{ m}$).
   * Se excluyen pings dentro de `urban_radius_m` de cualquier nodo `TOWN_CENTER`.
   * Si $\ge 10$ hashes distintos (`anonymous_device_hash`) registran `speed_kmh <= 3.0` durante $\ge 10\text{ min}$ en una ventana de $15\text{ min}$ sobre el mismo tramo, se crea (si no existe uno abierto) un incidente con `verification_state = SUSPECTED` e `impact_passability = CAUTION_SUSPECTED`. Nunca bloquea la vía automáticamente sin verificación.
2. **Validación Espacial y Consenso Comunitario:**
   * **Proximidad a Geometría:** Si $\text{ST\_Distance}(\text{report.location\_geom}, \text{segment.geom}) > 500\text{ m}$, el reporte se marca `REJECTED_DISTANCE` y no entra al cálculo.
   * **Override Oficial:** Si el rol del informante tiene `has_authoritative_override = TRUE`, el reporte pasa de inmediato a `VERIFIED`, confirmando o resolviendo el incidente sin requerir consenso.
   * **Cálculo Comunitario Acotado (Ventana $W = 30\text{ min}$):**
     * **Deduplicación por Actor:** Se toma únicamente el último reporte válido por cada `reporter_user_id` en el tramo.
     * **Peso Efectivo por Informante $r$:** $w_r = b_r \times (0.5 + 0.5 \times \text{reputation\_score}_r)$, donde $b_r = \text{base\_trust\_weight}$ (ej. `HEALTH_OPERATOR = 0.70`, `DRIVER = 0.60`).
     * **Soporte Neto por Estado $s$:** $S(s) = \sum_{r \in R_s} w_r$ y soporte contradictorio $S(\neg s) = \sum_{r \notin R_s} w_r$.
     * **Reglas de Transición:**
       * Si existe al menos 1 reporte para `BLOCKED`/`PARTIAL_ONE_LANE` pero no cumple las condiciones de confirmación, el incidente se crea/mantiene en `SUSPECTED` (`CAUTION_SUSPECTED`).
       * Pasa a `CONFIRMED` únicamente si se cumplen **simultáneamente**: (a) $|U_s| \ge 2$ usuarios distintos, (b) $S(s) \ge 1.00$, y (c) margen de mayoría $S(s) - S(\neg s) \ge 0.50$.
       * Al confirmarse, los reportes alineados pasan a `VERIFIED` ($+0.05$ reputación, tope $1.00$) y los contradictorios a `REJECTED_CONTRADICTED` ($-0.15$ reputación, piso $0.10$).

### 4.4 Arquitectura Offline-First y Resolución de Conflictos
* **Cliente Local:** Mantiene SQLite/IndexedDB con `sync_state(last_sync_version, last_synced_at)` y `outbox_queue` persistente con reintento exponencial.
* **Sincronización Diferencial:** `/sync/snapshot?since_version=N` entrega solo registros con `sync_version > N` ordenados ascendentemente, eliminando dependencia de relojes locales.
* **Resolución de Conflictos:**
  * Reportes duplicados por `client_uuid` retornan `DUPLICATE_IGNORED` sin alterar reputación ni consenso.
  * Reportes diferidos con `captured_at_client < incident.resolved_at` (tras cierre oficial `RESOLVED`) se almacenan como `STALE_ARCHIVED` y **no** reabren incidentes.
  * Si `captured_at_client > synced_at_server + 5 min`, se marca `clock_skew_flagged = TRUE` y se usa `synced_at_server` como referencia temporal de ventana de consenso.
  * El cliente marca visualmente `STALE_DATA` si transcurren más de `validity_hours` ($3\text{ h}$) desde `last_synced_at`.

### 4.5 Enrutamiento Resiliente, Puente Logístico Seguro y Corredor Humanitario (Capa 3)
1. **Derivación de Transitabilidad del Tramo:**
   * Incidente `CONFIRMED` con `BLOCKED` $\rightarrow$ arista podada (intransitable).
   * Incidente `CONFIRMED` con `PARTIAL_ONE_LANE` $\rightarrow$ arista habilitada **solo** si existe `humanitarian_window` activa compatible con `vehicle_category_id` y dirección.
   * Incidente `SUSPECTED` $\rightarrow$ arista transitable con estado `CAUTION_SUSPECTED` y penalización adicional $\beta = 0.50$ en la función de costo + advertencia obligatoria en respuesta.
2. **Poda Física Vehicular:** Se excluyen aristas con `is_topology_verified = FALSE`, `road_width_m < vehicle.min_road_width_m`, `max_slope_pct > vehicle.max_slope_pct`, `max_supported_tons < vehicle.max_weight_tons` o `surface_type = DIRT_TRACK` cuando `allows_dirt_track = FALSE`.
3. **Función de Costo:**
   $$\text{Costo}(e) = \text{base\_travel\_time\_min}(e) \times \left(1 + \alpha \cdot \frac{IVV(e)}{100} + \beta \cdot \mathbb{I}_{\text{suspected}}(e)\right)$$
4. **Protocolo de Puente Logístico Seguro (Fallback):**
   * Si no existe camino vehicular entre origen y destino, el motor busca pares de nodos alcanzables $(A, B)$ a ambos lados del incidente bloqueante que cumplan **todas** estas condiciones:
     1. `A.is_geotechnically_stable = TRUE` y `B.is_geotechnically_stable = TRUE`.
     2. Existe un `transfer_corridor` con `is_verified_safe = TRUE` conectando $A$ y $B$.
     3. La geometría `path_geom` del corredor **no** intersecta el círculo de peligro `ST_Buffer(incident.hazard_location_geom, incident.hazard_radius_m)`.
     4. El $IVV$ vigente del sector es $\le \text{corridor.max\_safe\_ivv}$.
   * Si ningún corredor cumple las 4 condiciones, el motor retorna obligatoriamente `NO_SAFE_ROUTE` (prohibido sugerir cruces no verificados).

### 4.6 Aislamiento del Simulador y Cálculo de Métricas de Impacto
* **Notification Firewall:** Todo servicio de envío externo (WhatsApp/SMS/Push) exige `execution_context == PRODUCTION`; cualquier intento desde `SIMULATION` lanza error de barrera y solo escribe en el log interno del escenario con prefijo `[SIMULATION]`.
* **Métricas con Signo Libre ($\Delta \in \mathbb{R}$):** Usando la versión activa de `impact_parameter_versions`:
  * $\Delta T = T_{\text{base}} - T_{\text{mit}}$ y $\%\Delta T = \frac{\Delta T}{T_{\text{base}}} \times 100$.
  * $\Delta \text{CO}_2 = \text{CO}_{2,\text{base}} - \text{CO}_{2,\text{mit}}$, donde $\text{CO}_2 = (t_{\text{idle}} \cdot f_{\text{idle}}) + (d_{\text{km}} \cdot f_{\text{km}})$. Si la ruta alterna es significativamente más larga que la espera base, $\Delta \text{CO}_2 < 0$ (costo ambiental adicional explícito).
  * $\Delta \text{Loss}_{\text{agri}} = L_{\text{base}} - L_{\text{mit}}$, incorporando `transfer_handling_loss_pct` cuando aplica puente logístico.

---

## 5. Suite de Tests Necesaria

Orden obligatorio de implementación SDD (Red Phase antes de código de producción): **Contrato $\rightarrow$ Integración (BD real con PostGIS) $\rightarrow$ E2E $\rightarrow$ Unitarios**.

### 5.1 Autenticación, Seguridad, Privacidad y Auditoría
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `SEC-01` | Integración | Acceso a `/routing/calculate` o `/sync/batch` sin JWT o con JWT expirado. | HTTP `401 Unauthorized`. |
| `SEC-02` | Integración | Usuario `DRIVER` intenta `PATCH /incidents/{id}/verification` o `POST /simulator/scenarios`. | HTTP `403 Forbidden`; sin mutación ni registro de cambio. |
| `SEC-03` | Integración | Reuso de un `refresh_token` ya rotado en `POST /auth/refresh`. | HTTP `401`; toda la `token_family_id` queda revocada y auditada en `audit_logs`. |
| `SEC-04` | Integración | 6.º intento fallido en `POST /auth/login` con OTP inválido. | Desafío bloqueado (`429`/`401`); no permite fuerza bruta. |
| `SEC-05` | Integración | Webhook `/webhooks/messaging` con firma HMAC inválida o timestamp $> 5\text{ min}$ (replay). | HTTP `401 Unauthorized`. |
| `SEC-06` | Integración | Inspección de `telemetry_pings` y rotación diaria de salt GPS. | Ninguna columna vincula `anonymous_device_hash` con `user_id`; pings $> 24\text{ h}$ son purgados. |

### 5.2 Contratos de API y Topología Espacial
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `CTR-01` | Contrato | Validación de `POST /routing/calculate` para los 3 modos: `ALTERNATE_ROUTE`, `LOGISTIC_BRIDGE_REQUIRED` y `NO_SAFE_ROUTE`. | Esquema JSON exacto con `reason_code` obligatorio en `NO_SAFE_ROUTE`. |
| `CTR-02` | Contrato | Validación de `GET /corridors/map` como FeatureCollection GeoJSON. | Cada feature contiene `LINESTRING` válido, `ivv_score`, `risk_level` derivado y `model_version_tag`. |
| `GEO-01` | Integración | Alta de un `road_segment` cuyo `ST_StartPoint(geom)` está a $50\text{ m}$ del `origin_node`. | `is_topology_verified` permanece `FALSE` y el tramo es excluido del enrutador. |

### 5.3 Lógica Principal ($IVV$, Consenso, Offline, Ruteo, Simulador)
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `IVV-01` | Unitario | Recálculo de un `assessment` histórico usando `ivv_model_versions` v1 tras activar v2. | El cálculo con v1 reproduce exactamente el `ivv_score` almacenado al centésimo; los límites $39.99/40.00/69.99/70.00$ clasifican correctamente. |
| `CON-01` | Integración | Un mismo `DRIVER` envía 5 reportes `BLOCKED` en $10\text{ min}$ sobre Carpish. | Deduplicado a 1 aporte; incidente queda en `SUSPECTED` (`CAUTION_SUSPECTED`), **no** pasa a `CONFIRMED`. |
| `CON-02` | Integración | 2 conductores con reputación $0.70$ reportan `BLOCKED` ($S = 1.02$) mientras 1 conductor con reputación $0.90$ reporta `OPEN` ($S = 0.57$). | Margen $1.02 - 0.57 = 0.45 < 0.50$: incidente permanece `SUSPECTED` por contradicción. |
| `CON-03` | Integración | 10 dispositivos anónimos detenidos $> 10\text{ min}$ en tramo interurbano (fuera de `urban_radius_m`). | Se crea incidente `SUSPECTED` (`CAUTION_SUSPECTED`); la vía **no** se bloquea hasta confirmación. |
| `CON-04` | Integración | Reporte enviado a $600\text{ m}$ de `road_segments.geom` (`LINESTRING`). | Marcado `REJECTED_DISTANCE`; excluido del consenso. |
| `OFF-01` | Integración | Reenvío de `client_uuid` duplicado en `/sync/batch` + reporte offline con `captured_at_client` anterior a un cierre `RESOLVED` oficial. | Duplicado retorna `DUPLICATE_IGNORED`; reporte antiguo pasa a `STALE_ARCHIVED` y el tramo sigue `OPEN`. |
| `OFF-02` | Integración | Cliente consulta `/sync/snapshot?since_version=100` tras 3 mutaciones. | Retorna exactamente los 3 deltas (`101..103`) y `current_sync_version = 103`. |
| `BRG-01` | Integración | Bloqueo en Carpish para `HEAVY_TRAILER` existiendo un `transfer_corridor` cuya `path_geom` intersecta `hazard_radius_m`. | Motor rechaza el corredor intersectado y retorna `NO_SAFE_ROUTE` (`HAZARD_INTERSECTS_TRANSFER_PATH`). |
| `BRG-02` | Integración | Bloqueo en Carpish existiendo un `transfer_corridor` con `is_verified_safe = TRUE`, fuera de `hazard_radius_m` y con $IVV \le \text{max\_safe\_ivv}$. | Retorna `LOGISTIC_BRIDGE_REQUIRED` referenciando ese `corridor_id`. |
| `HUM-01` | Integración | Tramo `CONFIRMED` con `PARTIAL_ONE_LANE` y ventana activa para `AMBULANCE_4X4`. | `AMBULANCE_4X4` obtiene ruta; `LIGHT_TRUCK` es bloqueado durante la ventana. |
| `SIM-01` | Integración | Ejecución de derrumbe en `/simulator/scenarios/{id}/events` con spy/mock de verificación sobre gateway SMS/WhatsApp y consulta a `incidents` de producción. | `0` llamadas al gateway real; `0` filas alteradas en `incidents` ni `weather_observations` de producción. |
| `SIM-02` | Unitario/Int | Escenario donde una ruta alterna de $120\text{ km}$ evita una espera de $20\text{ min}$. | `GET /simulator/scenarios/{id}/impact` devuelve `net_co2_delta_kg < 0` y `time_delta_min < 0` sin truncar ni forzar signos positivos. |

### 5.4 Test End-to-End (Flujo Demo Hackathon WFEO)
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `E2E-01` | E2E | En escenario aislado **Huánuco – Carpish – Tingo María**: (1) inyectar lluvia extrema $\rightarrow$ $IVV$ pasa a `RED` en sandbox, (2) inyectar derrumbe en Carpish $\rightarrow$ alerta solo en canal `[SIMULATION]`, (3) evaluar `AMBULANCE_4X4` (toma ruta alterna) y `HEAVY_TRAILER` (recibe `LOGISTIC_BRIDGE_REQUIRED` sobre corredor seguro o `NO_SAFE_ROUTE`), (4) verificar impacto en `/simulator/scenarios/{id}/impact` y cero mutaciones en producción. | Respuesta determinista $< 2\text{ s}$, métricas reproducibles con `param_version_id` y producción intacta. |