# `spec.md`: QULLA-RUTA — Red Territorial de Resiliencia Vial y Logística Crítica (MVP)

**Versión:** `0.2.2-sdd` *(Correcciones de concurrencia, seguridad operativa, contratos y datos de demostración)*  
**Metodología:** Specification-Driven Development (SDD) — *La especificación es la fuente de verdad ejecutable*.  
**Alcance Territorial MVP:** Corredor Vial Central y Sierra-Selva (Huánuco – Carpish – Tingo María y rutas alternas/vecinales).

---

## 1. Objetivo

Establecer una especificación técnica ejecutable y verificable para el MVP de **QULLA-RUTA**, manteniendo la arquitectura de tres capas y el entorno de simulación aislado. Esta versión distingue explícitamente datos reales, atributos no verificados, parámetros sintéticos de demo y decisiones operativas que siempre requieren validación humana. El objetivo es habilitar `/speckit.plan` para **una demostración controlada**, no certificar una plataforma de despacho de emergencias.

El sistema opera sobre el transporte multimodal existente (ambulancias rurales, camiones agrícolas, colectivos y motocicletas) bajo diseño **Offline-First**:
1. **Capa 1 — Predicción Geotécnica Reproducible ($IVV$):** Cálculo versionado cada 3 horas del Índice Dinámico de Vulnerabilidad Vial combinando precipitación acumulada (24–72 h), topografía, suelo y puntos críticos históricos.
2. **Capa 2 — Detección Híbrida con Estados Separados:** Separación explícita entre detección preliminar (`SUSPECTED`), confirmación operativa (`CONFIRMED`) y transitabilidad derivada, combinando telemetría GPS **seudónima** y consenso comunitario matemáticamente acotado.
3. **Capa 3 — Enrutamiento Espacial Resiliente, Puente Logístico Verificado y Corredor Humanitario:** Ruteo con soporte explícito para segmentos bidireccionales, pesos versionados ($\alpha, \beta$), transbordo únicamente por corredores con inspección vigente y libre de peligro (o respuesta obligatoria `NO_SAFE_ROUTE`), y ventanas de paso prioritario.
4. **Entorno de Simulación Aislado (Sandbox WFEO):** Escenarios hipotéticos completamente aislados de producción con contratos completos y métricas con soporte explícito para resultados positivos, negativos o no mitigables (`NO_SAFE_ROUTE`).

### Matriz de Cierre de Hallazgos (`0.2.0-sdd` $\rightarrow$ `0.2.1-sdd`)

| Prioridad | Hallazgo en v0.2.0 | Corrección Cerrada en v0.2.1 |
| :--- | :--- | :--- |
| **P0** | Fórmula $IVV$ usaba `M.w_1..M.w_4` inexistentes en esquema. | Alineación exacta con `M.w_precip`, `M.w_slope_ravine`, `M.w_soil` y `M.w_hotspot`. |
| **P0** | Constante $\alpha$ de enrutamiento sin valor ni versionado. | Nueva entidad `routing_config_versions` con `alpha_ivv_weight` ($\alpha \in [0.00, 3.00]$, default `0.75`) y `beta_suspected_penalty` ($\beta \in [0.00, 2.00]$, default `0.50`). |
| **P0** | Sincronización diferencial omitía entidades y eliminaciones. | Nueva tabla global `sync_change_log` (secuencia monotónica) cubriendo `UPSERT` y `DELETE` de todas las entidades sincronizables. |
| **P0** | `NO_SAFE_ROUTE` exigía métricas mitigadas positivas. | Columnas `mitigated_*` pasan a `NULLABLE` con `CHECK` condicional: `NULL` obligatorio cuando `resolution_type = 'NO_SAFE_ROUTE'`; deltas devuelven `null` y estado `UNMITIGATED_NO_SAFE_ROUTE`. |
| **P0 / Geo** | `ST_Distance`/`ST_Buffer` sobre `GEOMETRY(4326)` opera en grados, no metros. | Todas las mediciones métricas y buffers exigen conversión explícita a `GEOGRAPHY` (`::geography`) en PostGIS. |
| **P1** | Dependencia transitiva en `ivv_assessments` (`observation_id` $\rightarrow$ `segment_id`). | Eliminación de `segment_id` en `ivv_assessments` (3NF estricta: se resuelve vía `weather_observations.segment_id`). |
| **P1** | `transfer_corridors` sin caducidad de inspección. | Columna `inspection_valid_days` (rango `1..90`, default `30`), expiración automática (`TRANSFER_INSPECTION_EXPIRED`) e invalidación por evento geotécnico `RED`/derrumbe. |
| **P1** | Recorrido de segmentos bidireccionales sin especificar. | Expansión determinista a aristas dirigidas `ORIGIN_TO_DEST` y `DEST_TO_ORIGIN` (`ST_Reverse(geom)`), `max_slope_reverse_pct` y filtrado direccional por `humanitarian_windows`. |
| **P1** | Contratos del simulador incompletos. | Especificación completa de `/simulator/scenarios/{scenario_id}/routing` y esquemas Request/Response de todos los endpoints del sandbox. |
| **P1** | Generación de identificador rotativo GPS incompleta. | Protocolo explícito cliente–servidor (`client_ephemeral_id` rotado cada 24 h + `HMAC-SHA256(daily_server_salt, client_ephemeral_id)` en servidor) sin almacenar `user_id` ni `device_id`. |

### Matriz de correcciones adicionales (v0.2.1 → v0.2.2)

| Prioridad | Riesgo detectado | Cambio vinculante |
| :--- | :--- | :--- |
| **P0** | `BIGSERIAL` asigna secuencias antes de confirmar transacciones: un cursor puede omitir un evento confirmado tarde. | Contador **transaccional serializado**, orden de confirmación y `payload_json` inmutable en `sync_change_log`. |
| **P0** | Con IVV ausente/caducado se podía devolver una ruta aparentemente verificada. | Estado `UNKNOWN/STALE`, no autorización operativa y respuesta `NO_SAFE_ROUTE` con `STALE_IVV` cuando el cálculo requiera datos vigentes. |
| **P0** | `NO_SAFE_ROUTE` suponía siempre un incidente causante. | `blocking_incident_id` nullable, catálogo de causas para desconexión, restricciones del vehículo o datos no verificados. |
| **P0** | Datos del escenario, versiones y parámetros no bastaban para reproducir impacto y rutas. | Revisión de escenario, versión del IVV, entradas inmutables, política de eventos y semillas de demo etiquetadas. |
| **P1** | OTP de seis dígitos con hash veloz recuperable ante filtración. | HMAC con clave secreta fuera de BD, desafío único, expiración y límites de intento. |
| **P1** | Faltaban controles de idoneidad del transbordo por carga/paciente y de vigencia de información. | Capacidades de transbordo por tipo, validación de campo y aprobación humana explícita. |
| **P1** | Contratos de simulación y ventanas humanitarias incompletos. | Payloads discriminados por evento y comprobación de ventanas durante el paso estimado. |
| **P1** | Un resultado de consenso se usaba para castigar automáticamente reportes opuestos. | Ajustes de reputación negativos solo tras adjudicación humana o evidencia independiente. |

### Compuertas Constitucionales SDD (Fase -1 Pre-Implementación)
* **Artículos I y II (Library-First y CLI):** Los motores de $IVV$, consenso, enrutamiento espacial y cálculo de impacto deben implementarse como librerías independientes con interfaz CLI JSON (`stdin`/`stdout`).
* **Artículos III y IX (Test-First e Integration-First):** Aprobar contratos y criterios de aceptación; escribir pruebas de contrato/integración y demostrar fallo esperado (**RED**) antes de desarrollar cada unidad; implementar hasta **GREEN**, ejecutar E2E y regresión contra PostGIS real. No se exige que una prueba fallida «apruebe».
* **Artículos VII y VIII (Simplicidad y Anti-Abstracción):** Máximo 3 proyectos/componentes desplegables para el MVP (API Core + Motores, Cliente Web/Mapa, Adaptador Mensajería/Sync) sin capas de abstracción especulativas.

### Decisiones cerradas para la DEMO y límites de validez

Estas decisiones permiten planificar sin asumir que parámetros experimentales son evidencias de seguridad real.

| Decisión | Elección para el MVP | Límite explícito |
| :--- | :--- | :--- |
| Mensajería | Adaptador webhook **simulado local**, sin credenciales ni salida WhatsApp/SMS real. | Un proveedor real requiere aprobación separada, validación de firma y pruebas de privacidad. |
| Grafo vial | Extracción offline de OpenStreetMap **curada** como GeoJSON, con fuentes etiquetadas; complementar atributos con fuentes oficiales y levantamiento cuando existan. | Ni OSM ni datos inferidos autorizan el uso operativo de trochas, pendientes, puentes, rutas médicas o transbordos. |
| Aplicación | PWA con mapa y formularios almacenados en IndexedDB, `outbox_queue` persistente y reconexión. | **Offline-First = lectura de caché y reporte en cola**; cálculo de nueva ruta **solo online** en MVP. La ruta guardada offline es informativa, no una autorización de paso. |
| Modelo IVV demostrativo | `w_precip=0.350`, `w_slope_ravine=0.300`, `w_soil=0.200`, `w_hotspot=0.150`; topes lluvia 24h=80 mm, 72h=180 mm, pendiente=45%, quebrada=500 m; umbrales 40/70; validez=3 h. | Parámetros **hipotéticos** para pruebas repetibles, no estimación calibrada de probabilidad de derrumbe. |
| Enrutador demostrativo | `alpha_ivv_weight=0.75`; `beta_suspected_penalty=0.50`; las aristas tienen costo no negativo. | Costo de optimización ≠ tiempo de llegada medido. |
| Emisiones y carga | Usar la semilla sintética de §6.3 para `impact_parameter_versions`. | No reportar beneficios de CO₂ ni merma como valores reales sin factores y línea base validados. |
| IoT opcional | Integrar un pluviómetro ESP32 al adaptador `LOCAL_STATION` **solo** si hay calibración y sello temporal; si no, datos sintéticos exclusivos del sandbox. | No afirmar equivalencia entre sensor prototipo y una estación certificada. |

**Criterio de alcance:** El MVP no envía órdenes de tránsito, no abre vías, no declara un sendero seguro por inferencia algorítmica y no suplanta a COER, Defensa Civil ni personal médico. Toda ruta se acompaña de procedencia, hora, nivel de incertidumbre y estado operativo. La plataforma nunca debe decidir por sí sola un traslado de pacientes.

---

## 2. Esquema de Datos (3NF de entidades operativas + auditoría inmutable + PostGIS)

El **núcleo operativo** sigue 3NF: `ivv_assessments` obtiene el tramo mediante su observación, los niveles de riesgo se derivan y el estado de transitabilidad se calcula a partir de incidentes vigentes. Campos de constatación administrativa (`is_topology_verified`, `is_verified_safe`) son hechos de verificación humana, no simples valores derivados. `sync_change_log.payload_json` y `simulation_impact_logs.request_snapshot_json` son **instantáneas inmutables de auditoría**, y no se presentan como tablas operativas normalizadas. No se pretende que todas las estructuras JSON de auditoría cumplan 3NF.

**Invariantes SQL:** UUID como PK/FK; `CHECK` para rangos y estados; `UNIQUE` para claves naturales y estado activo; FK explícitas, PostGIS habilitado; índices GiST en geometrías y (cuando se consultan) expresiones `geom::geography`; políticas RBAC en API y en accesos directos a BD. Cambiar una versión publicada de IVV, enrutamiento o impacto está prohibido salvo `is_active`. Queda un historial auditable de activaciones.

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
| `phone_e164` | VARCHAR(20) | UNIQUE, NOT NULL | Teléfono internacional (clave para App y Bot). |
| `full_name` | VARCHAR(120) | NOT NULL | Nombre completo. |
| `password_hash` | VARCHAR(255) | NULLABLE | Hash Argon2id/bcrypt (nulo si solo autentica vía OTP). |
| `reputation_score` | DECIMAL(4,2) | NOT NULL, DEFAULT `0.50` ($0.10\text{--}1.00$) | Reputación dinámica acotada. |
| `is_active` | BOOLEAN | NOT NULL, DEFAULT `TRUE` | Estado de la cuenta. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de creación. |

#### `auth_refresh_tokens`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `token_id` | UUID | PK | Identificador del token. |
| `token_family_id` | UUID | NOT NULL | Familia de sesión para detección de reuso (*Reuse Detection*). |
| `user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Usuario propietario. |
| `token_hash` | VARCHAR(64) | UNIQUE, NOT NULL | SHA-256 del refresh token opaco. |
| `device_id` | VARCHAR(128) | NOT NULL | Identificador lógico del dispositivo de sesión. |
| `expires_at` | TIMESTAMPTZ | NOT NULL | Expiración. |
| `revoked_at` | TIMESTAMPTZ | NULLABLE | Marca de revocación explícita o por rotación. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de emisión. |

#### `otp_challenges`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `challenge_id` | UUID | PK | Desafío OTP. |
| `phone_e164` | VARCHAR(20) | NOT NULL | Teléfono destino. |
| `code_hash` | VARCHAR(64) | NOT NULL | HMAC-SHA256 en hexadecimal de `challenge_id || phone_e164 || otp`, con clave secreta fuera de BD; nunca SHA-256 simple de seis dígitos. |
| `attempts` | SMALLINT | NOT NULL, DEFAULT `0` ($\le 5$) | Intentos fallidos. |
| `expires_at` | TIMESTAMPTZ | NOT NULL | Expiración ($+5\text{ min}$). |
| `consumed_at` | TIMESTAMPTZ | NULLABLE | Marca de uso único. |

#### `audit_logs`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `audit_id` | UUID | PK | Registro inmutable de auditoría. |
| `actor_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NULLABLE | Actor humano (nulo si es proceso de sistema). |
| `action` | VARCHAR(64) | NOT NULL | Acción ejecutada. |
| `entity_type` | VARCHAR(48) | NOT NULL | Entidad afectada. |
| `entity_id` | UUID | NOT NULL | ID del registro afectado. |
| `before_state_json` | JSONB | NULLABLE | Estado previo. |
| `after_state_json` | JSONB | NOT NULL | Estado resultante **redactado**: jamás incluir OTP, tokens, contraseñas, claves, teléfono completo ni secretos de GPS. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Marca temporal UTC. |

### 2.2 Catálogo Vehicular, Topología Espacial y Sincronización Global

#### `vehicle_categories`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `category_id` | UUID | PK | Categoría vehicular. |
| `code` | VARCHAR(32) | UNIQUE, NOT NULL | `AMBULANCE_4X4`, `PICKUP_4X4`, `LIGHT_TRUCK`, `HEAVY_TRAILER`, `MOTORCYCLE`. |
| `max_slope_pct` | DECIMAL(4,1) | NOT NULL ($> 0$) | Pendiente máxima ascendente transitable (%). |
| `min_road_width_m` | DECIMAL(4,2) | NOT NULL ($> 0$) | Ancho mínimo de calzada (m). |
| `max_weight_tons` | DECIMAL(5,2) | NOT NULL ($> 0$) | Peso bruto vehicular estándar (t). |
| `allows_dirt_track` | BOOLEAN | NOT NULL | Permite circulación por trocha. |
| `priority_tier` | SMALLINT | NOT NULL ($1\text{--}5$) | Prioridad en corredor humanitario (`1` = máxima). |

#### `road_nodes`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `node_id` | UUID | PK | Nodo del grafo vial. |
| `name` | VARCHAR(120) | NOT NULL | Nombre del punto. |
| `node_type` | VARCHAR(32) | NOT NULL | `INTERSECTION`, `TOWN_CENTER`, `HOSPITAL`, `SAFE_TRANSFER_ZONE`, `TUNNEL_PORTAL`. |
| `location_geom` | GEOMETRY(POINT, 4326) | NOT NULL, GIST INDEX | Coordenada WGS84. |
| `elevation_m` | INTEGER | NOT NULL | Altitud sobre el nivel del mar (DEM/SRTM). |
| `is_geotechnically_stable` | BOOLEAN | NOT NULL | Apto como zona segura de transbordo. |
| `urban_radius_m` | INTEGER | NOT NULL, DEFAULT `0` ($\ge 0$) | Radio urbano de exclusión GPS (metros). |

#### `road_segments`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `segment_id` | UUID | PK | Tramo vial del grafo. |
| `origin_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo origen. |
| `destination_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo destino (`origin_node_id <> destination_node_id`). |
| `corridor_name` | VARCHAR(100) | NOT NULL | Nombre del corredor. |
| `geom` | GEOMETRY(LINESTRING, 4326) | NOT NULL, GIST INDEX | Trazado geométrico en sentido `ORIGIN_TO_DEST`. |
| `direction_type` | VARCHAR(16) | NOT NULL | `ONE_WAY` o `BIDIRECTIONAL`. |
| `length_km` | DECIMAL(6,2) | NOT NULL ($> 0$) | Longitud geodésica del tramo. |
| `avg_slope_pct` | DECIMAL(4,1) | NOT NULL ($\ge 0$) | Pendiente promedio (DEM). |
| `max_slope_forward_pct` | DECIMAL(4,1) | NOT NULL ($\ge 0$) | Pendiente máxima ascendente en sentido `ORIGIN_TO_DEST`. |
| `max_slope_reverse_pct` | DECIMAL(4,1) | NULLABLE ($\ge 0$) | Pendiente máxima ascendente en sentido `DEST_TO_ORIGIN` (`NOT NULL` si `BIDIRECTIONAL`, `NULL` si `ONE_WAY`). |
| `road_width_m` | DECIMAL(4,2) | NOT NULL ($> 0$) | Ancho efectivo de calzada. |
| `max_supported_tons` | DECIMAL(5,2) | NOT NULL ($> 0$) | Peso máximo soportado. |
| `surface_type` | VARCHAR(20) | NOT NULL | `PAVED`, `GRAVEL`, `DIRT_TRACK`. |
| `soil_susceptibility_idx` | DECIMAL(3,2) | NOT NULL ($0.00\text{--}1.00$) | Susceptibilidad geotécnica del suelo. |
| `distance_to_ravine_m` | INTEGER | NOT NULL ($\ge 0$) | Distancia mínima a quebrada activa. |
| `is_historical_hotspot` | BOOLEAN | NOT NULL | Punto crítico histórico (Provías). |
| `base_travel_time_min` | DECIMAL(6,2) | NOT NULL ($> 0$) | Tiempo base en condiciones normales. |
| `is_topology_verified` | BOOLEAN | NOT NULL, DEFAULT `FALSE` | `TRUE` solo después de chequeos espaciales y revisión de conectividad y procedencia; no acredita resistencia estructural del camino. |
| `geometry_source` | VARCHAR(64) | NOT NULL | Fuente del trazado (`OSM_CURATED`, `OFFICIAL`, `FIELD_SURVEY`). |
| `attributes_verified_at` | TIMESTAMPTZ | NULLABLE | Fecha de validación de atributos físicos; si no existe, los límites de anchura/carga/pendiente son **no verificados** y no pueden autorizar despacho real. |

#### `transfer_corridors`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `corridor_id` | UUID | PK | Sendero/vía de transbordo validado. |
| `side_a_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo seguro extremo A. |
| `side_b_node_id` | UUID | FK $\rightarrow$ `road_nodes(node_id)`, NOT NULL | Nodo seguro extremo B (`side_a_node_id <> side_b_node_id`). |
| `path_geom` | GEOMETRY(LINESTRING, 4326) | NOT NULL, GIST INDEX | Geometría verificada del cruce de transbordo. |
| `length_m` | INTEGER | NOT NULL ($> 0$) | Longitud geodésica del cruce. |
| `max_slope_pct` | DECIMAL(4,1) | NOT NULL ($\ge 0$) | Pendiente máxima del sendero. |
| `transfer_mode` | VARCHAR(32) | NOT NULL | `PEDESTRIAN_SAFE_PATH`, `SHUTTLE_RELAY`. |
| `max_safe_ivv` | DECIMAL(5,2) | NOT NULL ($0.00\text{--}100.00$) | Umbral máximo de $IVV$ seguro. |
| `is_verified_safe` | BOOLEAN | NOT NULL | Aprobación física vigente de Defensa Civil/COER. |
| `last_inspected_at` | TIMESTAMPTZ | NOT NULL | Fecha de última inspección geotécnica. |
| `inspection_valid_days` | SMALLINT | NOT NULL, DEFAULT `30` ($1\text{--}90$) | Vigencia máxima en días antes de exigir revalidación. |
| `requires_revalidation` | BOOLEAN | NOT NULL, DEFAULT `FALSE` | Se activa tras evento de riesgo o derrumbe cercano; solo una inspección autorizada puede devolverlo a `FALSE`. |
| `verified_by_user_id` | UUID | FK → `users(user_id)`, NULLABLE | Obligatorio si `is_verified_safe=TRUE`; debe ser autoridad habilitada. |

#### `transfer_corridor_capabilities` *(idoneidad real por carga)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `corridor_id` | UUID | PK compuesta, FK → `transfer_corridors(corridor_id)` | Camino aprobado. |
| `cargo_type` | VARCHAR(32) | PK compuesta | `MEDICAL_PATIENT`, `MEDICAL_SUPPLIES`, `PERISHABLE_AGRI`. |
| `max_payload_kg` | DECIMAL(8,2) | NOT NULL, > 0 | Capacidad máxima de la operación de relevo por movimiento. |
| `requires_medical_supervision` | BOOLEAN | NOT NULL | Si es paciente, exigir autorización y supervisión sanitaria. |
| `estimated_transfer_time_min` | DECIMAL(7,2) | NOT NULL, > 0 | Tiempo estimado de relevo para el tipo de carga; medición verificada o valor sintético marcado en DEMO. |

El motor solo propone un puente si existe capacidad para `cargo_type` **y** están verificadas condiciones de movilidad, asistencia sanitaria (si aplica), personal disponible y conexión de ambos extremos con las partes del viaje. `PROPOSED` nunca implica `ACTIVE`.

#### `sync_control` *(secuencia transaccional, una sola fila)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `singleton_id` | SMALLINT | PK, CHECK (=1) | Fila única del coordinador de cambios. |
| `last_seq` | BIGINT | NOT NULL, DEFAULT 0 | Último cambio **confirmado** mediante `UPDATE` bajo bloqueo de fila. |

#### `sync_change_log` *(eventos completos para sincronización)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `change_seq` | BIGINT | PK | Valor asignado con `UPDATE sync_control SET last_seq=last_seq+1 RETURNING last_seq` dentro de la misma transacción que la mutación y el evento (sin `nextval` ni `BIGSERIAL`). |
| `entity_type` | VARCHAR(32) | NOT NULL | `ROAD_NODE`, `ROAD_SEGMENT`, `TRANSFER_CORRIDOR`, `VEHICLE_CATEGORY`, `IVV_ASSESSMENT`, `INCIDENT`, `LOGISTIC_BRIDGE`, `HUMANITARIAN_WINDOW`. |
| `entity_id` | UUID | NOT NULL | ID del registro mutado o eliminado. |
| `operation` | VARCHAR(12) | NOT NULL | `UPSERT` o `DELETE`. |
| `changed_at` | TIMESTAMPTZ | NOT NULL | Marca temporal UTC de la mutación. |
| `payload_json` | JSONB | NULLABLE | Estado serializado **en el momento de la mutación** para `UPSERT`; `NULL` obligatorio para `DELETE`. Sin datos personales, secretos ni campos privados. |

### 2.3 Capa 1: Clima, Modelo $IVV$ y Configuración de Enrutamiento Versionada

#### `ivv_model_versions`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `model_version_id` | UUID | PK | Versión del modelo $IVV$. |
| `version_tag` | VARCHAR(32) | UNIQUE, NOT NULL | Ej. `ivv-v1.0-andes`. |
| `w_precip` | DECIMAL(4,3) | NOT NULL ($\ge 0$) | Peso de precipitación. |
| `w_slope_ravine` | DECIMAL(4,3) | NOT NULL ($\ge 0$) | Peso de pendiente y quebrada. |
| `w_soil` | DECIMAL(4,3) | NOT NULL ($\ge 0$) | Peso de susceptibilidad de suelo. |
| `w_hotspot` | DECIMAL(4,3) | NOT NULL ($\ge 0$) | Peso de punto crítico (Check: `w_precip + w_slope_ravine + w_soil + w_hotspot = 1.000`). |
| `precip_24h_cap_mm` | DECIMAL(6,2) | NOT NULL ($> 0$) | Tope normalización 24 h. |
| `precip_72h_cap_mm` | DECIMAL(6,2) | NOT NULL ($> 0$) | Tope normalización 72 h. |
| `slope_cap_pct` | DECIMAL(4,1) | NOT NULL ($> 0$) | Tope normalización pendiente. |
| `ravine_max_dist_m` | INTEGER | NOT NULL ($> 0$) | Distancia límite de influencia de quebrada. |
| `amber_threshold` | DECIMAL(5,2) | NOT NULL | Umbral inferior `AMBER` (ej. `40.00`). |
| `red_threshold` | DECIMAL(5,2) | NOT NULL | Umbral inferior `RED` (`red_threshold > amber_threshold`, ej. `70.00`). |
| `validity_hours` | SMALLINT | NOT NULL ($> 0$) | Vigencia del cálculo (estándar: `3`). |
| `is_active` | BOOLEAN | NOT NULL | Índice único parcial: máximo 1 fila `TRUE`. |

#### `routing_config_versions`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `routing_config_id` | UUID | PK | Versión de parámetros de costo del enrutador. |
| `version_tag` | VARCHAR(32) | UNIQUE, NOT NULL | Ej. `routing-v1.0`. |
| `alpha_ivv_weight` | DECIMAL(4,2) | NOT NULL, DEFAULT `0.75` ($0.00\text{--}3.00$) | Factor $\alpha$ de penalización por $IVV$. |
| `beta_suspected_penalty` | DECIMAL(4,2) | NOT NULL, DEFAULT `0.50` ($0.00\text{--}2.00$) | Factor $\beta$ de penalización por incidente `SUSPECTED`. |
| `is_active` | BOOLEAN | NOT NULL | Índice único parcial: máximo 1 fila `TRUE`. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de creación. |

#### `weather_observations`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `observation_id` | UUID | PK | Observación climática de producción. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo evaluado. |
| `source_name` | VARCHAR(32) | NOT NULL | `SENAMHI`, `NASA_GPM`, `LOCAL_STATION` *(prohibido `SIMULATOR`)*. |
| `precip_24h_mm` | DECIMAL(6,2) | NOT NULL ($\ge 0$) | Lluvia acumulada 24 h. |
| `precip_72h_mm` | DECIMAL(6,2) | NOT NULL ($\ge$ `precip_24h_mm`) | Lluvia acumulada 72 h. |
| `observed_at` | TIMESTAMPTZ | NOT NULL | Timestamp de observación. |

#### `ivv_assessments` *(3NF Estricta: Sin `segment_id` Redundante)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `assessment_id` | UUID | PK | Evaluación histórica inmutable. |
| `observation_id` | UUID | FK $\rightarrow$ `weather_observations(observation_id)`, NOT NULL | Determina únicamente el `segment_id` y los datos climáticos. |
| `model_version_id` | UUID | FK $\rightarrow$ `ivv_model_versions(model_version_id)`, NOT NULL | Versión de fórmula aplicada. |
| `ivv_score` | DECIMAL(5,2) | NOT NULL ($0.00\text{--}100.00$) | Puntaje $IVV$ calculado. |
| `calculated_at` | TIMESTAMPTZ | NOT NULL | Timestamp de cálculo. |

### 2.4 Capa 2: Telemetría Seudónima, Reportes e Incidentes Reales

#### `telemetry_daily_salts` *(metadatos de claves del servidor; secreto fuera de BD)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `salt_date_utc` | DATE | PK | Fecha UTC de vigencia del salt. |
| `key_ref` | VARCHAR(128) | NOT NULL | Referencia a secreto diario de 256 bits almacenado **fuera** de BD de aplicación (almacén de secretos; en demo, variable protegida). |

#### `telemetry_pings` *(Retención Máxima: 24 h)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `ping_id` | UUID | PK | Identificador interno del ping seudónimo. |
| `client_ping_uuid` | UUID | UNIQUE, NOT NULL | Clave aleatoria por **ping**, evita duplicados en retransmisiones offline. |
| `anonymous_device_hash` | VARCHAR(64) | NOT NULL | Identificador **seudónimo diario**: `HMAC-SHA256(secret_key_bytes, client_ephemeral_id)`; no equivale a anonimización garantizada. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo proyectado vía PostGIS (`::geography <= 50 m`). |
| `location_geom` | GEOMETRY(POINT, 4326) | NOT NULL | Posición capturada. |
| `speed_kmh` | DECIMAL(5,2) | NOT NULL ($\ge 0$) | Velocidad instantánea. |
| `recorded_at` | TIMESTAMPTZ | NOT NULL | Hora de captura. |
| `synced_at` | TIMESTAMPTZ | NOT NULL | Hora de ingesta. |

#### `field_reports`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `report_id` | UUID | PK | Reporte en servidor. |
| `client_uuid` | UUID | UNIQUE, NOT NULL | Clave de idempotencia offline. |
| `reporter_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Informante. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo reportado. |
| `proposed_passability` | VARCHAR(24) | NOT NULL | `BLOCKED`, `PARTIAL_ONE_LANE`, `OPEN`. |
| `location_geom` | GEOMETRY(POINT, 4326) | NOT NULL | Posición de captura. |
| `photo_uri` | VARCHAR(512) | NULLABLE | Evidencia fotográfica. |
| `channel` | VARCHAR(20) | NOT NULL | `MOBILE_APP`, `WHATSAPP_BOT`, `SMS`. |
| `validation_status` | VARCHAR(24) | NOT NULL | `PENDING`, `VERIFIED`, `REJECTED_DISTANCE`, `REJECTED_CONTRADICTED`, `STALE_ARCHIVED`. |
| `captured_at_client` | TIMESTAMPTZ | NOT NULL | Timestamp del dispositivo. |
| `clock_skew_flagged` | BOOLEAN | NOT NULL, DEFAULT `FALSE` | `TRUE` si `captured_at_client > synced_at_server + 5 min`. |
| `synced_at_server` | TIMESTAMPTZ | NOT NULL | Timestamp de sincronización. |

#### `incidents` *(Solo Operaciones Reales)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `incident_id` | UUID | PK | Incidente real. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo afectado. |
| `trigger_source` | VARCHAR(32) | NOT NULL | `TELEMETRY_CLUSTER`, `FIELD_CONSENSUS`, `AUTHORITY_DIRECT`. |
| `verification_state` | VARCHAR(24) | NOT NULL | `SUSPECTED`, `CONFIRMED`, `DISMISSED`, `RESOLVED`. |
| `impact_passability` | VARCHAR(24) | NOT NULL | `CAUTION_SUSPECTED`, `PARTIAL_ONE_LANE`, `BLOCKED`. |
| `hazard_location_geom` | GEOMETRY(POINT, 4326) | NOT NULL | Punto del derrumbe/huaico. |
| `hazard_radius_m` | INTEGER | NOT NULL, DEFAULT `150` ($> 0$) | Radio de exclusión geotécnica (metros). |
| `detected_at` | TIMESTAMPTZ | NOT NULL | Inicio de detección. |
| `confirmed_at` | TIMESTAMPTZ | NULLABLE | Momento de confirmación. |
| `resolved_at` | TIMESTAMPTZ | NULLABLE | Obligatorio en `RESOLVED`/`DISMISSED`, nulo en los demás estados; al cerrar, la transitabilidad vuelve a `OPEN` solo si no hay otros incidentes confirmados activos. |

#### `incident_report_links`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `incident_id` | UUID | PK, FK $\rightarrow$ `incidents(incident_id)` | Incidente vinculado. |
| `report_id` | UUID | PK, FK $\rightarrow$ `field_reports(report_id)` | Reporte vinculado. |

### 2.5 Capa 3: Puente Logístico y Corredor Humanitario

#### `logistic_bridges`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `bridge_id` | UUID | PK | Operación de puente logístico. |
| `incident_id` | UUID | FK $\rightarrow$ `incidents(incident_id)`, NOT NULL | Incidente `CONFIRMED` asociado. |
| `corridor_id` | UUID | FK $\rightarrow$ `transfer_corridors(corridor_id)`, NOT NULL | Corredor validado y vigente. |
| `cargo_type` | VARCHAR(32) | NOT NULL | `MEDICAL_PATIENT`, `MEDICAL_SUPPLIES`, `PERISHABLE_AGRI`. |
| `status` | VARCHAR(24) | NOT NULL | `PROPOSED`, `ACTIVE`, `SUSPENDED_UNSAFE`, `COMPLETED`. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Creación. |

#### `humanitarian_windows`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `window_id` | UUID | PK | Ventana prioritaria. |
| `incident_id` | UUID | FK $\rightarrow$ `incidents(incident_id)`, NOT NULL | Incidente `CONFIRMED` en `PARTIAL_ONE_LANE`. |
| `authorized_by_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Autoridad que aprueba. |
| `vehicle_category_id` | UUID | FK $\rightarrow$ `vehicle_categories(category_id)`, NOT NULL | Categoría priorizada. |
| `direction` | VARCHAR(16) | NOT NULL | `ORIGIN_TO_DEST` o `DEST_TO_ORIGIN`. |
| `starts_at` | TIMESTAMPTZ | NOT NULL | Inicio. |
| `ends_at` | TIMESTAMPTZ | NOT NULL | Fin (`ends_at > starts_at`). |
| `status` | VARCHAR(20) | NOT NULL | `SCHEDULED`, `ACTIVE`, `CLOSED`. |

### 2.6 Entorno de Simulación Aislado (Sandbox WFEO)

#### `simulation_scenarios`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `scenario_id` | UUID | PK | Escenario sandbox. |
| `name` | VARCHAR(120) | NOT NULL | Nombre del escenario. |
| `created_by_user_id` | UUID | FK $\rightarrow$ `users(user_id)`, NOT NULL | Creador. |
| `status` | VARCHAR(20) | NOT NULL | `DRAFT`, `RUNNING`, `COMPLETED`, `ARCHIVED`. |
| `revision` | BIGINT | NOT NULL, DEFAULT 0 | Incremento transaccional por evento sintético agregado; fija orden y reproducibilidad. |
| `fixture_tag` | VARCHAR(48) | NOT NULL | Versión de datos de prueba, ej. `carpish-demo-v1`. |
| `fixture_payload_json` | JSONB | NOT NULL | Instantánea **solo sandbox** de atributos de tramos, clima inicial, corredor de transbordo y capacidades sintéticas. Inmutable tras creación. |
| `fixture_sha256` | CHAR(64) | NOT NULL | Resumen SHA-256 de representación canónica de la instantánea; impide reemplazos silenciosos. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de creación. |

#### `sim_events`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `sim_event_id` | UUID | PK | Evento sintético. |
| `scenario_id` | UUID | FK $\rightarrow$ `simulation_scenarios(scenario_id)`, NOT NULL | Escenario propietario. |
| `segment_id` | UUID | FK $\rightarrow$ `road_segments(segment_id)`, NOT NULL | Tramo simulado. |
| `event_type` | VARCHAR(24) | NOT NULL | `WEATHER_OVERRIDE`, `BLOCKAGE_OVERRIDE`. |
| `precip_24h_mm` | DECIMAL(6,2) | NULLABLE | Lluvia 24 h (si `WEATHER_OVERRIDE`). |
| `precip_72h_mm` | DECIMAL(6,2) | NULLABLE | Lluvia 72 h (si `WEATHER_OVERRIDE`). |
| `simulated_passability` | VARCHAR(24) | NULLABLE | `BLOCKED`, `PARTIAL_ONE_LANE` (si `BLOCKAGE_OVERRIDE`). |
| `hazard_location_geom` | GEOMETRY(POINT, 4326) | NULLABLE | Punto sintético de peligro. |
| `hazard_radius_m` | INTEGER | NULLABLE ($> 0$) | Radio sintético de peligro (m). |
| `created_at` | TIMESTAMPTZ | NOT NULL | Timestamp. |
| `applied_revision` | BIGINT | NOT NULL, > 0 | Revisión del escenario al crear el evento, permite precedencia explícita. |

#### `impact_parameter_versions`
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `param_version_id` | UUID | PK | Versión de parámetros de impacto. |
| `version_tag` | VARCHAR(32) | NOT NULL, UNIQUE con `vehicle_category_id` | Etiqueta de parámetros por categoría; una misma etiqueta puede aplicarse a varias categorías. |
| `vehicle_category_id` | UUID | FK $\rightarrow$ `vehicle_categories(category_id)`, NOT NULL | Categoría aplicable. |
| `co2_idle_kg_per_hour` | DECIMAL(6,3) | NOT NULL ($> 0$) | Emisión en ralentí ($\text{kg CO}_2/\text{h}$). |
| `co2_drive_kg_per_km` | DECIMAL(6,3) | NOT NULL ($> 0$) | Emisión en circulación ($\text{kg CO}_2/\text{km}$). |
| `agri_cargo_value_usd` | DECIMAL(10,2) | NOT NULL ($\ge 0$) | Valor estándar de carga perecible (USD). |
| `perishability_loss_rate_per_hour` | DECIMAL(5,4) | NOT NULL ($0.0000\text{--}1.0000$) | Tasa horaria de deterioro. |
| `transfer_handling_loss_pct` | DECIMAL(4,3) | NOT NULL ($0.000\text{--}1.000$) | Merma por manipuleo en transbordo. |
| `spoilage_grace_hours` | DECIMAL(5,2) | NOT NULL, DEFAULT 0 | Horas de espera sin aplicar merma agrícola; a partir de este límite comienza la tasa horaria. |
| `is_active` | BOOLEAN | NOT NULL | Única versión activa **por categoría vehicular**, mediante índice único parcial. |
| `created_at` | TIMESTAMPTZ | NOT NULL | Fecha de emisión de la versión inmutable. |

#### `simulation_impact_logs` *(Soporte Explícito para `NO_SAFE_ROUTE`)*
| Campo | Tipo | Restricciones | Descripción |
| :--- | :--- | :--- | :--- |
| `sim_log_id` | UUID | PK | Corrida de evaluación. |
| `scenario_id` | UUID | FK $\rightarrow$ `simulation_scenarios(scenario_id)`, NOT NULL | Escenario evaluado. |
| `param_version_id` | UUID | FK $\rightarrow$ `impact_parameter_versions(param_version_id)`, NOT NULL | Parámetros usados. |
| `routing_config_id` | UUID | FK $\rightarrow$ `routing_config_versions(routing_config_id)`, NOT NULL | Versión de $(\alpha, \beta)$ usada. |
| `ivv_model_version_id` | UUID | FK → `ivv_model_versions(model_version_id)`, NOT NULL | Versión IVV exacta del cálculo de escenario. |
| `scenario_revision` | BIGINT | NOT NULL, >= 0 | Revisión de escenario congelada para esta corrida. |
| `request_snapshot_json` | JSONB | NOT NULL | Entradas exactas de la evaluación (origen, destino, vehículo, carga, línea base y factores); snapshot inmutable para reproducibilidad. |
| `resolution_type` | VARCHAR(32) | NOT NULL | `ALTERNATE_ROUTE`, `LOGISTIC_BRIDGE_REQUIRED`, `NO_SAFE_ROUTE`. |
| `baseline_travel_min` | DECIMAL(7,2) | NOT NULL ($> 0$) | Tiempo total sin QULLA-RUTA. |
| `mitigated_travel_min` | DECIMAL(7,2) | NULLABLE ($> 0$ si no nulo) | Tiempo mitigado (`NULL` si `NO_SAFE_ROUTE`). |
| `baseline_co2_kg` | DECIMAL(8,2) | NOT NULL ($\ge 0$) | Emisiones escenario base. |
| `mitigated_co2_kg` | DECIMAL(8,2) | NULLABLE ($\ge 0$ si no nulo) | Emisiones mitigadas (`NULL` si `NO_SAFE_ROUTE`). |
| `baseline_cargo_loss_usd` | DECIMAL(10,2) | NOT NULL ($\ge 0$) | Pérdida agrícola base. |
| `mitigated_cargo_loss_usd` | DECIMAL(10,2) | NULLABLE ($\ge 0$ si no nulo) | Pérdida agrícola mitigada (`NULL` si `NO_SAFE_ROUTE`). |
| `computed_at` | TIMESTAMPTZ | NOT NULL | Timestamp de cálculo. |

**Restricciones de esquema de auditoría:** `UNIQUE (scenario_id, sim_log_id)` y `CHECK` de `scenario_revision >= 0`. `request_snapshot_json` debe guardar `evaluation_time`, `fixture_sha256`, `baseline_wait_min`, rutas/hops utilizados, minutos de ralentí, km recorridos, carga, parámetros y fuentes del grafo; las versiones de IVV, enrutamiento e impacto deben ser inmutables. No se almacena información personal en el snapshot.

**Restricción `CHECK` obligatoria en `simulation_impact_logs`:**
`(resolution_type = 'NO_SAFE_ROUTE' AND mitigated_travel_min IS NULL AND mitigated_co2_kg IS NULL AND mitigated_cargo_loss_usd IS NULL) OR (resolution_type <> 'NO_SAFE_ROUTE' AND mitigated_travel_min IS NOT NULL AND mitigated_co2_kg IS NOT NULL AND mitigated_cargo_loss_usd IS NOT NULL)`.

---

## 3. Endpoints (Contrato de API)

**Convenciones Globales:**
* **Base URL:** `/api/v1`
* **Autenticación:** Bearer JWT (15 min) en `Authorization: Bearer <token>`; firma HMAC-SHA256 + timestamp anti-replay ($\le 5\text{ min}$) en `X-Webhook-Signature` para `BOT_SERVICE`.
* **Formato de Error Estándar:**
  ```json
  { "error_code": "STRING", "message": "STRING", "details": {} }
  ```

### 3.1 Identidad, Autenticación y Sesiones
| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/auth/otp/request` | Público (Rate-limited) | Emite OTP ($5\text{ min}$, máx. 5 intentos). |
| `POST` | `/auth/login` | Público | Autentica (`phone_e164` + credencial/OTP + `device_id`) y emite tokens. |
| `POST` | `/auth/refresh` | Refresh token válido | Rota refresh token; reuso revoca toda `token_family_id`. |
| `POST` | `/auth/logout` | Autenticado | Revoca el refresh token del dispositivo actual. |
| `POST` | `/auth/revoke-all` | Autenticado | Revoca todas las sesiones activas y audita en `audit_logs`. |

### 3.2 Sincronización Offline-First (log inmutable y cursor de commit)
| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `GET` | `/sync/snapshot` | Autenticado | `?since_version=<BIGINT>&limit=<INT>` (default `500`, máx. `1000`). Consulta eventos **confirmados** con `change_seq > since_version` y entrega a lo sumo `limit` elementos `changes[]` (cada evento incluye `change_seq`, `entity_type`, `entity_id`, `operation=UPSERT|DELETE`, instantánea `payload_json` o `null`), `next_since_version` y `has_more`. Nunca reconstruye un evento antiguo con el estado actual de la entidad. |
| `POST` | `/sync/batch` | Autenticado | Procesa `outbox_queue` (`reports` con `client_uuid` y `telemetry_pings` con `client_ephemeral_id`), devuelve resultado por ítem y reintenta idempotentemente sin duplicar consenso. |

### 3.3 Capa 1: Mapa Territorial e $IVV$
| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `GET` | `/corridors/map` | Público / Auth | FeatureCollection GeoJSON de tramos verificados geométricamente, `passability_state` (ignora incidentes terminales), `ivv_score` vigente o `null`, `risk_level=GREEN|AMBER|RED|UNKNOWN`, `observed_at`, `calculated_at`, `model_version_tag` e `is_stale`. Valores calculados pero vencidos se etiquetan y nunca se interpretan como verificación actual. |
| `POST` | `/ivv/recalculate` | `COER_ADMIN`, `BOT_SERVICE` | Registra `weather_observations`, calcula `ivv_assessments` con la versión activa de `ivv_model_versions`, y dispara `requires_revalidation = TRUE` en `transfer_corridors` cercanos si `risk_level = RED`. |

### 3.4 Capa 2: Detección Híbrida e Incidentes
| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/telemetry/pings` | Autenticado | Recibe `{ "client_ephemeral_id": "uuid", "ephemeral_id_date_utc": "2026-11-20", "pings": [{ "client_ping_uuid": "uuid", "latitude": -9.7, "longitude": -76.1, "speed_kmh": 1.0, "recorded_at": "2026-11-20T08:00:00Z" }] }`, calcula hash diario del **día de captura validado**; descarta `client_ephemeral_id` y `user_id` antes de persistir. Telemetría de más de 15 min no dispara nuevas alertas; de más de 24 h se rechaza. |
| `POST` | `/reports` | Autenticado | Crea `field_report` con `client_uuid`, valida distancia PostGIS y ejecuta consenso. |
| `POST` | `/webhooks/messaging` | HMAC (`BOT_SERVICE`) | Ingesta reporte estructurado vía WhatsApp/SMS y responde confirmación ligera. |
| `PATCH` | `/incidents/{incident_id}/verification` | `COER_ADMIN`, `CIVIL_DEFENSE` | Transiciona `verification_state` / `impact_passability`, registra en `sync_change_log` y `audit_logs`. |

### 3.5 Capa 3: Enrutamiento, Puente Logístico y Corredor Humanitario
| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/routing/calculate` | Autenticado | Calcula ruta de **apoyo a decisión** usando versión activa del enrutador y datos IVV vigentes. Si faltan datos o restricciones verificables, devuelve `NO_SAFE_ROUTE` en vez de certificar tránsito. |
| `POST` | `/bridges` | `COER_ADMIN`, `HEALTH_OPERATOR`, `CIVIL_DEFENSE` | Crea plan `PROPOSED`; solo personal autorizado activa `ACTIVE` tras comprobación de corredor, capacidades para carga/paciente, personal y autorización humana. No existe activación automática por respuesta de enrutamiento. |
| `PATCH` | `/bridges/{bridge_id}/status` | `COER_ADMIN`, `CIVIL_DEFENSE` | Transiciona `PROPOSED→ACTIVE`, `ACTIVE→SUSPENDED_UNSAFE/COMPLETED` tras revalidación y auditoría; un `HEALTH_OPERATOR` no activa solo el cruce. |
| `POST` | `/incidents/{incident_id}/humanitarian-windows` | `COER_ADMIN`, `CIVIL_DEFENSE` | Crea ventana direccional (`ORIGIN_TO_DEST` / `DEST_TO_ORIGIN`) en incidente confirmado; no autoriza una ruta si la hora prevista de salida del tramo excede el fin de la ventana. |

**Contrato `POST /routing/calculate`:**
* **Request:**
  ```json
  {
    "origin_node_id": "uuid",
    "destination_node_id": "uuid",
    "vehicle_category_code": "AMBULANCE_4X4",
    "cargo_type": "MEDICAL_PATIENT",
    "patient_count": 1
  }
  ```
* **Response `200 OK` (Alterna):**
  ```json
  {
    "resolution_type": "ALTERNATE_ROUTE",
    "routing_config_tag": "routing-v1.0",
    "total_distance_km": 142.5,
    "estimated_time_min": 195,
    "max_ivv_on_route": 48.2,
    "hops": [{ "segment_id": "uuid", "direction": "ORIGIN_TO_DEST" }],
    "eta_quality": "NOMINAL_ESTIMATE",
    "operational_authorization": "REQUIRED",
    "warnings": []
  }
  ```
* **Response `200 OK` (Puente Logístico):**
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
      "last_inspected_at": "2026-11-10T08:00:00Z",
      "inspection_expires_at": "2026-12-10T08:00:00Z",
      "operational_authorization": "REQUIRED",
      "estimated_transfer_time_min": 25.0
    }
  }
  ```
* **Response `200 OK` (Sin Ruta Segura):**
  ```json
  {
    "resolution_type": "NO_SAFE_ROUTE",
    "blocking_incident_id": null,
    "reason_code": "NETWORK_DISCONNECTED"
  }
  ```

**Semántica exacta de respuestas de ruteo:** `reason_code` siempre es **un solo valor de enum**, jamás una cadena con alternativas separadas por `|`. Enum de `NO_SAFE_ROUTE`: `NETWORK_DISCONNECTED`, `TRANSFER_CARGO_CAPACITY_EXCEEDED`, `VEHICLE_RESTRICTION`, `UNVERIFIED_PHYSICAL_ATTRIBUTES`, `STALE_IVV`, `NO_VERIFIED_TRANSFER_CORRIDOR`, `HAZARD_INTERSECTS_TRANSFER_PATH`, `TRANSFER_IVV_EXCEEDED`, `TRANSFER_INSPECTION_EXPIRED`, `TRANSFER_REVALIDATION_REQUIRED`, `TRANSFER_CARGO_NOT_ALLOWED`. `blocking_incident_id` es `null` cuando no existe un incidente confirmado que explique el resultado. El significado de `NO_SAFE_ROUTE` es «no hay una solución **verificable con los datos disponibles**», no la demostración de que toda alternativa física es imposible. Si coinciden varias causas se usa un orden de precedencia reproducible (§6.2) y se devuelven las demás en `warnings`.

**Tiempo y prioridad:** `estimated_time_min` es suma de tiempos nominales más retrasos explícitos si existen, **no** el costo de optimización IVV. En producción `evaluation_time` es el reloj UTC del servidor capturado una vez; el sandbox recibe `evaluation_time` explícito y fija ese instante para vigencias, ventanas, ETA y restricciones. La llegada estimada a cada arista se calcula a partir de un único `evaluation_time` capturado al inicio de la petición; una ventana parcial solo habilita el paso si cubre completamente `estimated_entry_at..estimated_exit_at`. Las estimaciones no sustituyen información de un operador. El acceso a `/routing/calculate` no equivale a permiso oficial de circulación.

### 3.6 Entorno de Simulación Aislado (Contratos Completos)

| Método | Ruta | Acceso | Propósito |
| :--- | :--- | :--- | :--- |
| `POST` | `/simulator/scenarios` | `COER_ADMIN` | Crea escenario sandbox desde `fixture_tag` permitido; copia el fixture íntegro y su SHA-256; sin tablas de operación real modificadas. |
| `POST` | `/simulator/scenarios/{scenario_id}/events` | `COER_ADMIN` | Inyecta evento sintético en `sim_events` (sin mutar producción ni emitir alertas reales). |
| `POST` | `/simulator/scenarios/{scenario_id}/routing` | `COER_ADMIN` | Ejecuta el motor de ruteo sobre el grafo superpuesto con los `sim_events` del escenario. |
| `POST` | `/simulator/scenarios/{scenario_id}/evaluate` | `COER_ADMIN` | Evalúa un viaje (origen, destino, categoría vehicular, espera base estimada) y persiste `simulation_impact_logs`. |
| `GET` | `/simulator/scenarios/{scenario_id}/impact` | Autenticado | Devuelve insumos atómicos y deltas derivados (o `null` si `NO_SAFE_ROUTE`). |

**Contratos JSON del Simulador:**
* **`POST /simulator/scenarios`**
  * Request: `{ "name": "Derrumbe Carpish Lluvia Extrema", "fixture_tag": "carpish-demo-v1" }`
  * Response `201 Created`: `{ "scenario_id": "uuid", "name": "string", "fixture_tag": "carpish-demo-v1", "revision": 0, "status": "DRAFT", "created_at": "ISO8601" }`
* **`POST /simulator/scenarios/{scenario_id}/events`**
   * Request `WEATHER_OVERRIDE`: `{ "segment_id": "uuid", "event_type": "WEATHER_OVERRIDE", "precip_24h_mm": 85.0, "precip_72h_mm": 190.0 }`.
   * Request `BLOCKAGE_OVERRIDE`: `{ "segment_id": "uuid", "event_type": "BLOCKAGE_OVERRIDE", "simulated_passability": "BLOCKED", "hazard_lat": -9.712, "hazard_lon": -76.091, "hazard_radius_m": 200 }`.
   * Validación: **exactamente una variante** por petición; campos de otra variante se rechazan con `422`; no se aceptan `null` arbitrarios. Cada alta incrementa `simulation_scenarios.revision` y crea `sim_events.applied_revision`.
  * Response `201 Created`: `{ "sim_event_id": "uuid", "scenario_id": "uuid", "execution_context": "SIMULATION", "simulated_notifications": [{ "channel": "WHATSAPP_BOT", "preview": "[SIMULATION] Alerta de bloqueo..." }] }`
* **`POST /simulator/scenarios/{scenario_id}/routing`**
  * Request: idéntico a `POST /routing/calculate`, añadiendo `"scenario_revision": 2` (selección de revisión reproducible) y `"evaluation_time": "2026-11-20T09:00:00Z"`.
  * Response `200 OK`: idéntico a `POST /routing/calculate`, añadiendo `"execution_context": "SIMULATION"` y `"scenario_id": "uuid"`.
* **`POST /simulator/scenarios/{scenario_id}/evaluate`**
  * Request: `{ "origin_node_id": "uuid", "destination_node_id": "uuid", "vehicle_category_code": "HEAVY_TRAILER", "cargo_type": "PERISHABLE_AGRI", "cargo_payload_kg": 1200, "baseline_wait_min": 360, "scenario_revision": 2, "evaluation_time": "2026-11-20T09:00:00Z" }`
   * Response `201 Created` (ejemplo): `{ "sim_log_id": "uuid", "resolution_type": "ALTERNATE_ROUTE", "mitigation_status": "MITIGATED", "scenario_revision": 2, "evaluation_time": "2026-11-20T09:00:00Z", "param_version_tag": "impact-demo-v1", "model_version_tag": "ivv-demo-v1", "routing_config_tag": "routing-v1.0" }`. Campos `resolution_type`/`mitigation_status` son enums separados, no cadenas compuestas.
* **`GET /simulator/scenarios/{scenario_id}/impact`**
  * Response `200 OK`:
    ```json
    {
      "scenario_id": "uuid",
      "evaluations": [
        {
          "sim_log_id": "uuid",
          "resolution_type": "NO_SAFE_ROUTE",
          "mitigation_status": "UNMITIGATED_NO_SAFE_ROUTE",
          "baseline": { "travel_min": 480, "co2_kg": 42.5, "cargo_loss_usd": 1200.0 },
          "mitigated": { "travel_min": null, "co2_kg": null, "cargo_loss_usd": null },
          "deltas": { "time_delta_min": null, "time_saved_pct": null, "net_co2_delta_kg": null, "net_agri_savings_usd": null }
        }
      ]
    }
    ```

---

### 3.7 Contratos transversales, sincronización e identidad

**`POST /sync/batch`**: un `reports[]` lleva `client_uuid`, `segment_id`, `proposed_passability`, `latitude`, `longitude`, `captured_at_client` y `photo_uri` opcional. `telemetry_batches[]` lleva `client_ephemeral_id`, `ephemeral_id_date_utc` y `pings[]` con `client_ping_uuid` y `recorded_at`. El servidor toma `reporter_user_id` exclusivamente del JWT, nunca del body. Respuesta por elemento: `CREATED`, `DUPLICATE_IGNORED`, `REJECTED_DISTANCE`, `STALE_ARCHIVED`, `EXPIRED_NO_DETECTION` o `VALIDATION_ERROR`; incluye `server_time`. Mismo `client_uuid` y usuario/payload distintos retorna `409 IDEMPOTENCY_CONFLICT` sin exponer datos del registro original.

**`GET /sync/snapshot`**: `since_version=0` reproduce el log completo para bootstrap; en el MVP los eventos se conservan durante todo el hackathon, incluidos tombstones. El frontend escribe página + cursor en **una transacción local**; `next_since_version` es el último `change_seq` efectivamente entregado, no un máximo global adelantado. Si `changes=[]`, mantiene cursor. `has_more` se calcula sobre los eventos confirmados visibles. El log solo contiene información autorizada para el rol y datos territoriales; se rechazan campos personales/secreto en `payload_json`. Cambio de permisos exige nueva sincronización desde 0. `UPSERT` entrega snapshot del evento, no consulta viva a la tabla. Rango de `limit`: `1..1000`.

**Estados y errores**: `401` sin sesión/credencial válida; `403` sin rol; `409` conflicto de idempotencia o versión; `422` esquema incorrecto; `429` abuso de autenticación. Todo endpoint incorpora `request_id` para trazabilidad sin secretos. `POST /auth/otp/request` siempre responde de modo indistinguible para teléfonos existentes o no; la comprobación OTP requiere `challenge_id`, `phone_e164`, `otp_code`, comparación en tiempo constante, 5 intentos y bloqueo por teléfono/IP. Los secretos y tokens nunca se registran en `audit_logs` ni logs HTTP.

**Segregación de demo**: los endpoints `/simulator/*` se ejecutan bajo principal con privilegios **solo de lectura** sobre tablas productivas base; los atributos supuestos, clima de arranque y corredores seguros *ficticios* se leen únicamente de `fixture_payload_json`, nunca del inventario oficial; escritura únicamente en tablas `simulation_*`/`sim_events` y un almacén interno de vistas previas. No basta con `execution_context` en el código; gateway externo de demo usa credenciales ausentes y salida de red bloqueada. Los eventos sintéticos se ordenan por `applied_revision`; una consulta con `scenario_revision=N` aplica solo eventos con `applied_revision<=N`, y para cada `(segment_id,event_type)` prevalece el último de ese subconjunto. Una revisión ausente o mayor que la actual produce `422`; `evaluation_time` es obligatorio en sandbox para reproducibilidad. Nunca mutan `requires_revalidation` de producción.

---

## 4. Lógica de Negocio

### 4.1 Validación Topológica Espacial (PostGIS Métrico)

Un segmento admite `is_topology_verified=TRUE` **solo** si supera estas comprobaciones y revisión de conectividad de mapa:

1. `ST_IsValid(geom) AND ST_IsSimple(geom) AND ST_Length(geom::geography) > 0` (línea sin autointersección).
2. `ST_DWithin(ST_StartPoint(geom)::geography, origin_node.location_geom::geography, 15)`.
3. `ST_DWithin(ST_EndPoint(geom)::geography, destination_node.location_geom::geography, 15)`.
4. `direction_type='BIDIRECTIONAL'` exige `max_slope_reverse_pct NOT NULL`; `ONE_WAY` exige `NULL`.
5. Las intersecciones **reales a nivel de calzada** deben compartir nodo; cruces separados por puentes/túneles NO crean conectividad. La geometría, dirección y atributos llevan procedencia verificable. Una tolerancia de 15 m solo ayuda a limpiar digitalizaciones, **no** crea conexiones físicas.

Todas las distancias en metros emplean `geography` (o proyección métrica comprobada): `ST_DWithin` sobre geografía toma metros y `ST_Buffer(geography, metros)` devuelve geografía. Preferir para excluir zonas peligrosas `NOT ST_DWithin(corridor.path_geom::geography, incident.hazard_location_geom::geography, incident.hazard_radius_m)`, equivalente para un peligro circular local con radio de seguridad. El buffer puede usarse solo para dibujo del radio, nunca en grados.

### 4.2 Motor Predictivo $IVV$ Reproducible (Capa 1)
Usando la versión activa $M$ de `ivv_model_versions` y $\text{clamp}(x, 0, 1) = \min(1, \max(0, x))$:
1. **Precipitación ($P$):**
   $$P_{24} = \text{clamp}\left(\frac{\text{precip\_24h\_mm}}{M.\text{precip\_24h\_cap\_mm}}, 0, 1\right), \quad P_{72} = \text{clamp}\left(\frac{\text{precip\_72h\_mm}}{M.\text{precip\_72h\_cap\_mm}}, 0, 1\right)$$
   $$P = 0.6 \cdot P_{24} + 0.4 \cdot P_{72}$$
2. **Topografía y Quebrada ($S$):**
   Con $\text{slope}_{\max} = \max(\text{max\_slope\_forward\_pct}, \text{COALESCE}(\text{max\_slope\_reverse\_pct}, 0))$:
   $$S_{\text{slope}} = \text{clamp}\left(\frac{\text{slope}_{\max}}{M.\text{slope\_cap\_pct}}, 0, 1\right), \quad S_{\text{ravine}} = 1 - \text{clamp}\left(\frac{\text{distance\_to\_ravine\_m}}{M.\text{ravine\_max\_dist\_m}}, 0, 1\right)$$
   $$S = 0.5 \cdot S_{\text{slope}} + 0.5 \cdot S_{\text{ravine}}$$
3. **Suelo ($G$) e Histórico ($H$):** $G = \text{soil\_susceptibility\_idx} \in [0, 1]$; $H = 1.0$ si `is_historical_hotspot = TRUE`, $0.2$ en caso contrario.
4. **Cálculo y Clasificación Derivada (Nombres Exactos del Esquema):**
   $$IVV = \text{round}\Big(100 \times (M.\text{w\_precip} \cdot P + M.\text{w\_slope\_ravine} \cdot S + M.\text{w\_soil} \cdot G + M.\text{w\_hotspot} \cdot H),\ 2\Big)$$
   * `GREEN`: $IVV < M.\text{amber\_threshold}$
   * `AMBER`: $M.\text{amber\_threshold} \le IVV < M.\text{red\_threshold}$
   * `RED`: $IVV \ge M.\text{red\_threshold}$

### 4.3 Privacidad GPS, Telemetría Pasiva y Consenso Matemático (Capa 2)

1. **Identificador seudónimo, no anonimato total.** Cliente genera UUID v4 aleatorio para cada día UTC; servidor calcula `HMAC-SHA256(secret_key_bytes_for_capture_day, client_ephemeral_id)` donde la clave es el valor binario recuperado de `telemetry_daily_salts.key_ref` (nunca el texto de un hex). El servidor autentica el JWT para controlar abuso, pero no guarda su identificador junto con pings. Aun así, las trayectorias GPS pueden ser reidentificables; requerir consentimiento, minimización y no registrar JWT, IP ni `client_ephemeral_id` junto con pings en trazas accesibles.
2. **Reloj y duración.** El día de la clave se deriva de `recorded_at` **validado** y de la versión del identificador diario declarada por el cliente; si está en día distinto o fuera de tolerancia, rechazar/poner en cuarentena, nunca asignar silenciosamente salt del día actual. `pings` >24 h: `EXPIRED_NO_DETECTION`; 15 min a 24 h: auditoría acotada, sin crear alerta nueva. Purga por `synced_at` y ventana de captura, mínimo necesario, sin conservar trazas históricas ilimitadas. Las claves del día se conservan solo mientras permiten ingesta válida y luego se destruyen.
3. **Detección temporal.** Map-match al segmento más cercano verificado dentro de 50 m; ambiguos entre vías paralelas/superpuestas se descartan del consenso GPS. Excluir radios urbanos usando `ST_DWithin(...::geography,...::geography, urban_radius_m)`. Un dispositivo califica si registra al menos 3 pings con `speed_kmh <= 3.0`, con intervalos consecutivos <=5 min, abarcando al menos 10 min del mismo segmento dentro de 15 min. Se requieren al menos 10 hashes distintos, controles antifraude y no se crea más de un `SUSPECTED` abierto por tramo. La telemetría **jamás** confirma cierre.
4. **Reportes.** Rechazar `REJECTED_DISTANCE` fuera de `ST_DWithin(report.location_geom::geography, segment.geom::geography, 500)`. En ventana de 30 min deduplicar último reporte válido por `reporter_user_id`; `w_r = base_trust_weight_r × (0.5 + 0.5 × reputation_score_r)`. Para confirmar: 2 informantes distintos, apoyo `S(s)>=1.00` y margen `S(s)-S(¬s)>=0.50`. Valores base de demo: `DRIVER=0.60`, `HEALTH_OPERATOR=0.70`, `CIVIL_DEFENSE=1.00`, `COER_ADMIN=1.00`, `BOT_SERVICE=0.10`. La cuenta nueva no constituye independencia física demostrada; el resultado es **apoyo comunitario**, no prueba geotécnica.
5. **Autoridad y reputación.** Un actor autorizado puede confirmar o cerrar vía `PATCH /incidents/{id}/verification` tras verificación propia documentada (nunca desde el rol `BOT_SERVICE`); `OPEN` comunitario no levanta automáticamente un cierre oficial. Ganancia `+0.05` solo tras corroboración; penalización `-0.15` solo si una autoridad o evidencia independiente confirma falsedad, nunca por una simple mayoría que se autovalida. Alterar reputaciones exige auditoría.
6. **Estado por tramo:** `CONFIRMED/BLOCKED` domina; luego `CONFIRMED/PARTIAL_ONE_LANE`; luego `SUSPECTED/CAUTION_SUSPECTED`; sin incidentes vigentes → `OPEN`. `DISMISSED` y `RESOLVED` quedan históricos y no mantienen el bloqueo; cambios y resoluciones son transacciones auditadas. La confirmación requiere `confirmed_at`, el cierre requiere `resolved_at` y ningún estado terminal permite `SUSPECTED` activo simultáneo para el mismo evento.

### 4.4 Enrutamiento Direccional, Bidireccionalidad y Puente Logístico Seguro (Capa 3)

1. **Grafo dirigido:** para cada tramo verificado, usar `origin→destination` (pendiente `max_slope_forward_pct`) y, si es `BIDIRECTIONAL`, también `destination→origin` (pendiente `max_slope_reverse_pct`, geometría `ST_Reverse(geom)`). El `hops[]` de respuesta conserva `{segment_id,direction}`; no se modifica la geometría canónica del segmento.
2. **Poda vehicular conservadora:** omitir tramos cerrados, pendientes superiores a la capacidad vehicular, ancho insuficiente, peso estándar del vehículo superior a la capacidad soportada o trochas no permitidas. Atributos físicos sin verificación (`attributes_verified_at IS NULL`) no habilitan una **ruta operativamente autorizada**. La demo permite su representación exclusivamente con `operational_authorization=REQUIRED` y etiqueta `SYNTHETIC_ATTRIBUTES`.
3. **Riesgo faltante:** si no existe evaluación de IVV o `NOW()>calculated_at+validity_hours`, mostrar `risk_level=UNKNOWN` e `is_stale=TRUE` para la decisión actual. El enrutador operativo descarta esa arista como verificada, retorna `NO_SAFE_ROUTE` con `STALE_IVV` si el bloqueo de verificación impide elegir ruta. Nunca usar `IVV=0` por omisión. La visualización de rutas con datos sintéticos es **solo DEMO**, no despacho.
4. **Costo y ETA:** para cada arista elegible, `cost(e)=base_travel_time_min(e) × (1 + alpha_ivv_weight×IVV(e)/100 + beta_suspected_penalty×I_suspected(e))`. Ambas constantes se toman de una versión activa. Dijkstra/A* minimiza **costo ponderado**; `estimated_time_min` suma tiempos nominales y demoras explícitas, separadas del costo penalizado. En rutas parciales, `estimated_entry_at` y `estimated_exit_at` se comparan con la ventana humanitaria (el paso completo cabe dentro de ella); no se permite salir de la ventana a mitad de tramo.
5. **Puente logístico:** solo si no existe ruta vehicular, buscar una pareja `(A,B)` tal que A sea alcanzable desde origen y B permita llegar a destino; ambos son nodos geotécnicamente estables. Corredor inspeccionado por autoridad habilitada, inspección no expirada, `requires_revalidation=FALSE`, distancia a peligro `>hazard_radius_m`, IVV del sector **vigente** y menor o igual a `max_safe_ivv`, y capacidad apta en `transfer_corridor_capabilities` para `cargo_type` y `cargo_payload_kg`; si supera `max_payload_kg`, rechazar con `TRANSFER_CARGO_CAPACITY_EXCEEDED`. `MEDICAL_PATIENT` usa `patient_count=1` y autorización sanitaria documentada. Para `PERISHABLE_AGRI` y `MEDICAL_SUPPLIES`, `cargo_payload_kg` es obligatorio y positivo; para pacientes, este campo se omite. La API no admite cargas desconocidas cuando pretende proponer un puente. Si `MEDICAL_PATIENT`, supervisión sanitaria y autorización humana son obligatorias. Una propuesta informa dos piernas vehiculares, transbordo y sus incertidumbres; solo la autoridad puede cambiar `PROPOSED→ACTIVE` tras reconfirmación en campo.
6. **Sin solución verificable:** retornar `NO_SAFE_ROUTE`, `blocking_incident_id=null` si no hay bloqueo atribuible y `reason_code` según catálogo. No construir un «paso seguro» geométrico inventado, ni inferir que una pista cruzable sea viable para paciente o carga pesada. Puede existir una alternativa física no modelada; solicitar evaluación de campo.

### 4.5 Sincronización Offline-First, Simulador Aislado y Métricas

1. **Orden de commit, no de asignación anticipada:** todo `INSERT/UPDATE/DELETE` de las ocho entidades sincronizadas ocurre en una transacción que primero asigna el siguiente `change_seq` por `UPDATE sync_control ... RETURNING` bajo bloqueo de la fila única, y luego inserta `sync_change_log` con `payload_json` inmutable de la versión del evento. El bloqueo de fila se mantiene hasta `COMMIT`; otro escritor espera, por lo que un `change_seq` mayor jamás se confirma antes de otro menor. Una reversión revierte también contador y log. **Prohibido `BIGSERIAL/nextval`** para este cursor.
2. **Consumo offline:** `/sync/snapshot?since_version=N` pagina los eventos confirmados y entrega snapshots exactos; cliente aplica página y checkpoint en una misma transacción IndexedDB. `DELETE` es una lápida explícita. Se conserva el log durante todo el MVP, sin purga que pudiera dejar a clientes offline irrecuperables. `outbox_queue` permanece hasta respuesta idempotente reconocida. Un reloj local no ordena cambios del servidor.
3. **Fecha real de datos:** `last_synced_at` señala conectividad, **no** validez de lluvia. Para riesgos usar `calculated_at`, `observed_at` y vigencia de su modelo; `STALE_DATA` de la interfaz aparece si alguna fuente necesaria vence o si caché tiene >3 h. Un cliente offline no debe presentar una ruta vieja como aprobada.
4. **Simulador:** `sim_events` se aplica sobre el **fixture inmutable del escenario** (`fixture_payload_json`) y, cuando sea necesario para el mapa, sobre geometrías base de solo lectura; cada evaluación congela la revisión de `simulation_scenarios`. Prevalece último evento por `(segment_id,event_type)` según `applied_revision`; `WEATHER_OVERRIDE` genera IVV sintético solo en memoria/sandbox, `BLOCKAGE_OVERRIDE` modifica transitabilidad solo en sandbox. Nunca cambia incidentes, observaciones o `requires_revalidation` reales. El adaptador demo no tiene conexión/cuenta de mensajería externa. **Ningún `transfer_corridor` marcado como aprobado en un fixture se escribe en la tabla operativa `transfer_corridors`**; los IDs sintéticos solo existen en la revisión del escenario y son rechazados por `/bridges` real.
5. **Impacto reproducible:** `baseline_travel_min = viaje_normal_min + baseline_wait_min`. `mitigated_travel_min` se calcula por ruta/trasbordo medido bajo supuestos del escenario; **no** por costo de riesgo ponderado. `CO₂ = horas_idle×co2_idle_kg_per_hour + km_drive×co2_drive_kg_per_km`. Para perecibles: `pérdida = min(1, max(0, horas_espera−spoilage_grace_hours)×perishability_loss_rate_per_hour + tasa_manipuleo_si_hay_puente) × agri_cargo_value_usd`. Los valores son modelos simplificados y sintéticos hasta validación. `NO_SAFE_ROUTE` → métricas mitigadas y deltas `null`; otros tipos → deltas con cualquier signo. Guardar entradas y versión en `request_snapshot_json`.

---

## 5. Suite de Tests Necesaria

Orden SDD verificable por unidad: **aprobar contrato → escribir prueba de contrato/integración → RED esperado → implementación → GREEN → E2E → regresión**. Validar en PostgreSQL con extensión PostGIS real. Ningún test se considera aprobado mientras falla.

### 5.1 Seguridad, Autenticación y Privacidad GPS
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `SEC-01` | Integración | Petición a `/routing/calculate` o `/sync/batch` sin JWT o con JWT expirado. | HTTP `401 Unauthorized`. |
| `SEC-02` | Integración | Usuario `DRIVER` intenta `PATCH /incidents/{id}/verification` o `POST /simulator/scenarios`. | HTTP `403 Forbidden`. |
| `SEC-03` | Integración | Reuso de `refresh_token` ya rotado en `/auth/refresh`. | HTTP `401`; revocación completa de `token_family_id` registrada en `audit_logs`. |
| `SEC-04` | Integración | Webhook `/webhooks/messaging` con HMAC inválido o timestamp $> 5\text{ min}$. | HTTP `401 Unauthorized`. |
| `SEC-05` | Integración | *(Regresión P1)* Ingesta en `/telemetry/pings` verificando BD y purga. | `telemetry_pings` almacena el hash seudónimo HMAC y `client_ping_uuid`, sin `user_id` ni `client_ephemeral_id`; `telemetry_daily_salts` solo almacena `key_ref`; pings expirados purgados. |

### 5.2 Contratos de API y Exactitud Espacial PostGIS
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `CTR-01` | Contrato | Validación de `/routing/calculate` para `ALTERNATE_ROUTE` (con `hops[].direction`), `LOGISTIC_BRIDGE_REQUIRED` y `NO_SAFE_ROUTE` (enum completo de `reason_code`). | Esquema JSON exacto. |
| `CTR-02` | Contrato | *(Regresión P1)* Validación completa de los 5 endpoints de `/simulator/scenarios*`, incluyendo `/routing`, `/evaluate` e `/impact`. | Esquemas Request/Response conformes y `execution_context = "SIMULATION"`. |
| `GEO-01` | Integración | *(Regresión P0 PostGIS)* Tramo con `ST_StartPoint` a $14\text{ m}$ vs. $16\text{ m}$ del `origin_node` vía `::geography`. | $14\text{ m}$ $\rightarrow$ `is_topology_verified = TRUE`; $16\text{ m}$ $\rightarrow$ `FALSE` y excluido del ruteo. |
| `GEO-02` | Integración | *(Regresión P0 PostGIS)* Reporte a $490\text{ m}$ vs. $510\text{ m}$ de `road_segments.geom` vía `::geography`. | $490\text{ m}$ aceptado (`PENDING`); $510\text{ m}$ rechazado (`REJECTED_DISTANCE`). |

### 5.3 Regresión de Lógica Principal ($IVV$, Consenso, Sync, Bidireccionalidad, Puente, Simulador)
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `IVV-01` | Unitario/Int | *(Regresión P0/P1)* Cálculo de $IVV$ leyendo `w_precip`, `w_slope_ravine`, `w_soil`, `w_hotspot` y resolviendo `segment_id` desde `weather_observations`. | Resultado determinista al centésimo; cero columnas redundantes en `ivv_assessments`. |
| `RTE-01` | Integración | *(Regresión P0)* Cambio de `routing_config_versions` (`alpha_ivv_weight` de `0.75` a `2.00`) sobre corredor con tramo en `AMBER`. | El enrutador aplica la versión activa, reporta `routing_config_tag` y desvía cuando el costo penalizado supera la alterna. |
| `RTE-02` | Integración | *(Regresión P1)* Tramo `BIDIRECTIONAL` con `max_slope_forward_pct = 6.0` y `max_slope_reverse_pct = 14.0` para `HEAVY_TRAILER` (`max_slope_pct = 10.0`), más `humanitarian_window` solo `ORIGIN_TO_DEST`. | Permite `ORIGIN_TO_DEST` y bloquea `DEST_TO_ORIGIN` correctamente en ambos criterios. |
| `CON-01` | Integración | 1 usuario envía 5 reportes `BLOCKED`; luego 2 usuarios distintos reportan `BLOCKED` vs. 1 usuario con alta reputación reportando `OPEN` (margen $< 0.50$). | Deduplicación activa; incidente permanece `SUSPECTED` hasta cumplir $|U_s| \ge 2$, $S \ge 1.00$ y margen $\ge 0.50$. |
| `SYN-01` | Integración | *(Regresión P0)* Mutación de `transfer_corridors` y eliminación (`DELETE`) de una `humanitarian_window`, seguidas de `GET /sync/snapshot?since_version=N`. | `sync_change_log` entrega ambos eventos (`UPSERT` y `DELETE`) en orden de `change_seq`. |
| `BRG-01` | Integración | *(Regresión P1)* Corredor seguro con `last_inspected_at` hace 31 días (`inspection_valid_days = 30`) o marcado `requires_revalidation = TRUE` tras $IVV$ `RED`. | Motor rechaza el corredor y retorna `NO_SAFE_ROUTE` (`TRANSFER_INSPECTION_EXPIRED` o `TRANSFER_REVALIDATION_REQUIRED`). |
| `BRG-02` | Integración | *(Regresión P0 PostGIS)* Corredor cuya `path_geom::geography` intersecta `ST_Buffer(hazard_location_geom::geography, 150)`. | Retorna `NO_SAFE_ROUTE` (`HAZARD_INTERSECTS_TRANSFER_PATH`). |
| `SIM-01` | Integración | *(Regresión P0)* Evaluación en `/simulator/scenarios/{id}/evaluate` donde el resultado es `NO_SAFE_ROUTE`. | `simulation_impact_logs` persiste `mitigated_* = NULL`; `/impact` devuelve `mitigation_status = "UNMITIGATED_NO_SAFE_ROUTE"` y deltas en `null`; cero alertas reales emitidas. |
| `SIM-02` | Unitario/Int | Escenario con desvío largo donde `mitigated_co2_kg > baseline_co2_kg`. | `/impact` retorna `net_co2_delta_kg < 0` sin truncar signo. |

### 5.4 Casos adicionales obligatorios v0.2.2 (concurrencia, seguridad y exactitud)

| ID | Tipo | Caso obligatorio | Resultado esperado |
| :--- | :--- | :--- | :--- |
| `SYN-02` | Integración/concurrencia | Transacción A obtiene cursor y demora `COMMIT`; B intenta mutar y confirmar; cliente sincroniza entre ambas. | B espera bloqueo de `sync_control`; jamás se confirma un `change_seq` alto antes de uno bajo; al reanudar el cursor ve ambas mutaciones. |
| `SYN-03` | Integración | Dos `UPSERT` al mismo tramo seguidos de `DELETE`, con paginación `limit=1`. | Tres eventos con `payload_json` históricos correctos y tombstone; el cliente aplica y avanza solo el último recibido. |
| `SYN-04` | Integración | Reenvío del mismo `client_ping_uuid` después de recuperar señal. | Un único ping; otro usuario no puede obtener contenido de una clave idempotente ajena. |
| `SEC-06` | Seguridad | Filtrar hipotéticamente `otp_challenges`, intentar enumerar seis dígitos sin secreto de servidor y reusar OTP. | Sin `OTP_SECRET` no se puede validar offline por hash directo; OTP consumido/revocado no reabre sesión. |
| `SEC-07` | Integración | JWT/ID de sesión presente en log HTTP de pings o secreto diario almacenado en BD. | La prueba de política de logs rechaza tokens/IDs/salts; en tabla solo existe `key_ref`. |
| `GEO-03` | Integración | Cruce aparente de dos vías a distinta cota (puente sobre carretera) y una intersección vial real. | No se crea nodo/conectividad en paso elevado, pero sí en cruce real. |
| `IVV-02` | Integración | Tramo sin evaluación IVV o con vigencia caducada, frente a un tramo actualizado. | Mapa marca `UNKNOWN/is_stale`; ruta no se presenta verificada por omisión; `NO_SAFE_ROUTE/STALE_IVV` si impide ruta verificable. |
| `RTE-03` | Integración | Una ventana parcial termina a mitad del `estimated_travel_time_min` del segmento. | El motor no permite iniciar el segmento, aun si la hora de entrada era válida. |
| `RTE-04` | Contrato | Grafo desconectado sin incidente confirmado. | `NO_SAFE_ROUTE`, `blocking_incident_id=null`, `reason_code=NETWORK_DISCONNECTED`. |
| `BRG-03` | Integración | Sendero vigente para suministros pero no habilitado para pacientes, o carga mayor que capacidad. | Rechaza puente para la categoría/peso, no lo activa ni produce tiempo ficticio. |
| `SIM-03` | Integración | Inyectar dos lluvias sintéticas en el mismo tramo y una interrupción; repetir evaluación contra revisión 2. | Gana la mayor `applied_revision` por tipo; evaluación anterior conserva revisión y entradas originales. |
| `SIM-06` | Integración/seguridad | Crear fixture con corredor sintético `is_verified_safe` y habilitar un bloqueo sobre él. | El sandbox puede proponer o rechazar el corredor sintético, pero `/bridges` real no conoce su ID y ninguna fila operativa queda alterada. |
| `SIM-04` | Contrato | Enviar evento `WEATHER_OVERRIDE` con `simulated_passability` simultáneo. | `422` sin escribir `sim_events` ni alterar revisión de escenario. |
| `SIM-05` | Seguridad | Simular llamada deliberada a un gateway real desde sandbox. | Denegada por credenciales inexistentes/regla de egreso, además de firewall lógico; cero mensajes enviados. |
| `IMP-01` | Unitario/Int | Con `NO_SAFE_ROUTE`, el impacto carece de tiempo mitigado; con ruta larga puede aumentar CO₂. | Deltas `null` en el primero; delta CO₂ negativo en el segundo, sin forzar mejoría. |

### 5.5 Test End-to-End (Escenario Demo Hackathon WFEO)
| ID | Tipo | Caso de Prueba Crítico | Resultado Esperado |
| :--- | :--- | :--- | :--- |
| `E2E-01` | E2E | En escenario aislado **Huánuco – Carpish – Tingo María**: (1) inyectar lluvia extrema (`WEATHER_OVERRIDE`) $\rightarrow$ $IVV$ `RED` en sandbox, (2) inyectar derrumbe en Carpish (`BLOCKAGE_OVERRIDE`) $\rightarrow$ alertas solo `[SIMULATION]`, (3) consultar `/simulator/scenarios/{id}/routing` para `AMBULANCE_4X4` (toma alterna) y `HEAVY_TRAILER` (`LOGISTIC_BRIDGE_REQUIRED` sobre corredor vigente o `NO_SAFE_ROUTE`), (4) ejecutar `/evaluate` y validar `/impact` sin mutaciones en producción. | Respuesta determinista <2 s **solo para fixture de referencia** (hasta 1 000 aristas sobre máquina de pruebas documentada), contratos completos, trazabilidad de versiones (`model_version_tag`, `routing_config_tag`, `param_version_tag`) y producción intacta; no garantizar ese tiempo en cualquier móvil/red. |

---

## 6. Invariantes de ejecución, datos sintéticos y criterios de aceptación

### 6.1 Orden de confirmación y paginación segura

Una secuencia `BIGSERIAL` puede asignar identificadores fuera del orden de `COMMIT`. Por ello la fuente de verdad del cursor **no** es una secuencia PostgreSQL común. En cada transacción que modifique una entidad sincronizada:

```sql
-- Dentro de la MISMA transacción que cambia el registro canónico.
UPDATE sync_control
SET last_seq = last_seq + 1
WHERE singleton_id = 1
RETURNING last_seq; -- nuevo change_seq, fila bloqueada hasta COMMIT

-- Insertar event snapshot correspondiente usando ese change_seq.
-- UPSERT: payload_json = representación autorizada de la fila EN ESE INSTANTE.
-- DELETE: payload_json = NULL, operation = 'DELETE'.
-- No insertar eventos desde una transacción separada.
```

**Propiedad que deben probar los tests:** si A toma la secuencia bajo bloqueo pero aún no confirma, B no puede confirmar una secuencia mayor mientras A siga abierta. Si A revierte, el valor queda disponible de nuevo por la naturaleza transaccional de `UPDATE`; no se publica un cursor adelantado. Cada mutación confirmada y su evento son indivisibles. El log preserva su `payload_json` para evitar lecturas inconsistentes si el registro cambia después. El primer cliente obtiene todos los eventos desde cero; no hay compactación durante el MVP.

**Idempotencia de escrituras offline:** `client_uuid` único por reporte y `client_ping_uuid` único por ping; la idempotencia del ping usa también la identidad seudónima diaria y rechaza cambios de contenido para el mismo ID, sin devolver datos personales. Un duplicado exacto devuelve confirmación estable, no recrea incidentes, reputación ni alertas. Un UUID reutilizado con distinto contenido se rechaza. Para mayor robustez el servidor implementa `UNIQUE` y `INSERT...ON CONFLICT` bajo transacción, no una secuencia insegura de `SELECT` y luego `INSERT`.

### 6.2 Reglas de decisión, seguridad y errores reproducibles

- Prioridad de estados viales activos por tramo: `BLOCKED` (confirmado) > `PARTIAL_ONE_LANE` (confirmado) > `CAUTION_SUSPECTED` > `OPEN`. `RESOLVED`/`DISMISSED` no restringen tránsito.
- Datos de riesgo: `observed_at` no debe ser posterior al instante de evaluación (salvo tolerancia explícita ≤5 min); un IVV faltante/caducado produce `UNKNOWN`. **El IVV es un índice experimental, no una probabilidad calibrada de derrumbe.** Los umbrales de color no sustituyen alertas oficiales.
- En el simulador, los datos sintéticos IVV de `WEATHER_OVERRIDE` se consideran vigentes solo en esa revisión y deben marcarse `source=SIMULATION`; nunca se insertan como `weather_observations` de producción. Evaluar una misma revisión y el mismo `evaluation_time` con semilla idéntica produce el mismo resultado; no utilizar `NOW()` directamente al evaluar una revisión histórica.
- Si varios `NO_SAFE_ROUTE` aplican, prioridad diagnóstica estable: `UNVERIFIED_PHYSICAL_ATTRIBUTES` > `STALE_IVV` > `VEHICLE_RESTRICTION` > `TRANSFER_REVALIDATION_REQUIRED` > `TRANSFER_INSPECTION_EXPIRED` > `HAZARD_INTERSECTS_TRANSFER_PATH` > `TRANSFER_IVV_EXCEEDED` > `TRANSFER_CARGO_CAPACITY_EXCEEDED` > `TRANSFER_CARGO_NOT_ALLOWED` > `NO_VERIFIED_TRANSFER_CORRIDOR` > `NETWORK_DISCONNECTED`. El algoritmo registra todas las causas y retorna la primera **aplicable a la ausencia de solución**, no una causa secundaria irrelevante para una ruta viable.
- `NO_SAFE_ROUTE` no puede presentarse como «todos los caminos físicamente imposibles». Es una insuficiencia de solución verificable dentro del grafo y la información disponible.
- Contratos OpenAPI 3.1: los ejemplos JSON son ilustrativos; generar schemas con `enum`, `oneOf` discriminado por `resolution_type` y `event_type`, `required`, `nullable`, rangos, zonas horarias y RBAC. No usar cadenas de ejemplo con `A|B` como valores reales.
- Seguridad operacional: `POST /bridges` genera `PROPOSED`. `PATCH /bridges/{bridge_id}/status` exige constancia de inspección y autorización vigente; un `MEDICAL_PATIENT` requiere operador sanitario. El sistema nunca autoriza a un conductor a cruzar un huaico o un corredor sin verificación de campo.

### 6.3 Semillas de prueba para el hackathon (SINTÉTICAS, no calibradas)

**Fixture inmutable de escenario:** `fixture_tag=carpish-demo-v1` se carga desde un archivo JSON local validado y se copia a `simulation_scenarios.fixture_payload_json` al crear el escenario. Incluye: IDs de nodos y tramos de un GeoJSON curado, atributos **sintéticos** de accesibilidad vehicular, observaciones meteorológicas sintéticas por tramo, corredor simulado con geometría y `transfer_corridor_capabilities` sintéticas, sello `SIMULATION_ONLY`, y mapas de versión del seed. Estas condiciones solo son válidas para calcular el caso de prueba; no se escriben en `transfer_corridors`, `weather_observations` ni en confirmaciones reales. `fixture_sha256` se calcula sobre JSON canónico y se conserva en cada `request_snapshot_json` de evaluación.


**IVV**: `version_tag=ivv-demo-v1`, pesos `[0.350, 0.300, 0.200, 0.150]`; límites `precip_24h_cap_mm=80`, `precip_72h_cap_mm=180`, `slope_cap_pct=45`, `ravine_max_dist_m=500`, `amber_threshold=40`, `red_threshold=70`, `validity_hours=3`. Operación decimal `NUMERIC` con redondeo a dos decimales **half away from zero**, igual en CLI y PostgreSQL. Cálculo de prueba sin variables: si `P=1`, `S=1`, `G=1`, `H=1`, entonces `IVV=100.00`; si `P=S=G=0` y `H=0.2`, entonces `IVV=3.00`. Esas cifras prueban determinismo matemático, no seguridad física.

**Enrutamiento**: `version_tag=routing-v1.0`, `alpha_ivv_weight=0.75`, `beta_suspected_penalty=0.50`. Si tiempo base=20 min, IVV=40 y no hay sospecha, costo de elección=26.00 (20×[1+0.75×0.40]); `estimated_time_min=20` antes de retrasos reales. Cuando un segmento no tiene IVV vigente, el algoritmo no asume que sea verde.

**Impacto de prueba**: `version_tag=impact-demo-v1` para cada categoría, tasas únicamente sintéticas para comparar escenarios. Moneda USD y masas en kg. Los factores se incorporan al seed y se indican como **HIPOTÉTICOS** en la interfaz.

| `vehicle_category_code` | `co2_idle_kg_per_hour` | `co2_drive_kg_per_km` | `agri_cargo_value_usd` | `perishability_loss_rate_per_hour` | `spoilage_grace_hours` | `transfer_handling_loss_pct` |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| `AMBULANCE_4X4` | 1.20 | 0.33 | 0.00 | 0.0000 | 0.00 | 0.000 |
| `PICKUP_4X4` | 1.00 | 0.30 | 2500.00 | 0.0300 | 2.00 | 0.015 |
| `LIGHT_TRUCK` | 1.80 | 0.75 | 4500.00 | 0.0300 | 2.00 | 0.015 |
| `HEAVY_TRAILER` | 3.50 | 1.35 | 9000.00 | 0.0300 | 2.00 | 0.015 |
| `MOTORCYCLE` | 0.15 | 0.075 | 0.00 | 0.0000 | 0.00 | 0.000 |

**Reglas para semillas:** crear un `impact_parameter_versions` por categoría con `UNIQUE(vehicle_category_id, version_tag)` y exactamente uno activo por categoría. Los valores agrícolas en categorías no agrícolas son `0` y **no** representan valor de pacientes o medicinas. Nunca extrapolar estas emisiones y pérdidas a la región sin medición independiente. Guardar con cada simulación la carga, horas de espera, distancia y versión; cambiar el seed exige otra versión, no editar filas históricas.

**Red vial de la demo:** usar un corredor pequeño curado con IDs fijos/GeoJSON, un tramo bloqueable en Carpish y una trocha alterna apta solo para `AMBULANCE_4X4` dentro de supuestos experimentales. Para garantizar que `WEATHER_OVERRIDE` lleva realmente a `RED`, el tramo de Carpish del fixture usa `max_slope_forward_pct=27.0`, `distance_to_ravine_m=200`, `soil_susceptibility_idx=0.80` y `is_historical_hotspot=TRUE`; con lluvia 85/190 mm, `P=1`, `S=0.60`, `G=0.80`, `H=1` y `IVV=84.00` (RED); los demás tramos del escenario tienen datos IVV sintéticos vigentes. Sin autorización física real, las rutas son `SIMULATION` y llevan advertencias de atributos sintéticos. Para el tráiler, incluir **en `fixture_payload_json`**, y no en `transfer_corridors` real, un corredor sintético con idoneidad de `PERISHABLE_AGRI`, capacidad ≥1 200 kg y tiempo de transbordo ilustrativo de 25 min. Si el escenario intersecta el peligro o vence la inspección sintética, devolver `NO_SAFE_ROUTE`. No crear una validación oficial de campo inexistente.

### 6.4 Checklist de liberación SDD

- [ ] Seed IVV, enrutamiento e impacto creado con versiones e indicadores `DEMO/SYNTHETIC`, sin sustituir factores reales.
- [ ] Migraciones PostgreSQL/PostGIS y `CHECK`/FK/índices espaciales validados sobre base de datos real.
- [ ] Todos los contratos OpenAPI 3.1 generados y validados para sus tres tipos de respuesta y dos eventos de simulación.
- [ ] Pruebas contract-first e integration-first fallan en RED de forma esperada y luego pasan a GREEN; suite E2E sin salidas reales.
- [ ] Dos transacciones concurrentes no producen pérdida de cursor de sincronización; `payload_json` y tombstones correctos.
- [ ] Riesgo vencido, grafo desconectado, atributos físicos sin verificar y transbordos vencidos nunca se presentan como autorizaciones.
- [ ] Rutas demo no son reportadas como operativas; no existe una credencial que permita notificaciones externas desde sandbox.
- [ ] Datos GPS seudónimos con consentimientos y purga; OTP y secretos nunca se exponen en BD de eventos ni logs.
- [ ] `E2E-01` medido en entorno documentado, resultados negativos y `null` calculados correctamente.

**Criterio para `/speckit.plan`:** se aprueba la planificación de una **demo aislada y reproducible** cuando el esquema, contratos, parámetros hipotéticos, mapa curado y riesgos se acepten como hipótesis de implementación. Una operación vial o sanitaria real exige validación independiente de datos, autoridades competentes, protocolos y seguridad antes de cualquier despliegue.

### 6.5 Referencias técnicas normativas

- PostGIS — `ST_DWithin` y unidades para geografía: https://postgis.net/docs/ST_DWithin.html
- PostGIS — `ST_Buffer` y geografía: https://postgis.net/docs/ST_Buffer.html
- PostgreSQL — límites de secuencias transaccionales: https://www.postgresql.org/docs/current/functions-sequence.html
