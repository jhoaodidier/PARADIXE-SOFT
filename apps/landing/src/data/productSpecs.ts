export interface VerifiedImageAsset {
  src: string;
  alt: string;
  width: number;
  height: number;
}

export interface ScrollyChapter {
  id: string;
  step: string;
  eyebrow: string;
  title: string;
  summary: string;
  highlights: string[];
  badge: string;
  accent: "blue" | "emerald" | "amber" | "indigo";
  image: VerifiedImageAsset;
}

export interface BentoMetricCard {
  id: string;
  value: string;
  label: string;
  detail: string;
  span: "wide" | "tall" | "standard";
  accent?: "blue" | "emerald" | "amber";
}

export interface ProfileUseCase {
  id: "fleets" | "coer" | "health" | "b2b";
  label: string;
  tagline: string;
  headline: string;
  description: string;
  bullets: string[];
  metricValue: string;
  metricLabel: string;
  codeTitle: string;
  codeSnippet: string;
}

export interface ModuleSlide {
  id: string;
  eyebrow: string;
  title: string;
  summary: string;
  specs: string[];
  image: VerifiedImageAsset;
}

export interface RustCrateSpec {
  name: string;
  responsibility: string;
  guarantee: string;
}

export interface RegionCoverage {
  iso: string;
  name: string;
  segments: string;
  pavedCertified: string;
  hotspots: string;
}

export const PLATFORM_APP_URL =
  import.meta.env.VITE_APP_URL ?? "http://localhost:5173";

export const HERO_MEDIA: VerifiedImageAsset = {
  src: "/images/hero-andes-corridor.jpg",
  alt: "Relieve topográfico 3D nocturno de la cordillera andina con trazos luminosos de rutas seguras en zafiro y esmeralda",
  width: 1376,
  height: 768,
};

export const SCROLLY_CHAPTERS: ScrollyChapter[] = [
  {
    id: "ivv-prediction",
    step: "01",
    eyebrow: "Capa 1 · Predicción Geotécnica IVV",
    title: "Anticipa el riesgo antes del deslizamiento.",
    summary:
      "Cada corredor vial recibe un Índice Dinámico de Vulnerabilidad Vial determinista con aritmética decimal exacta y horizontes predictivos a +24h, +48h y +72h.",
    highlights: [
      "Fórmula calibrada: 0.35 Lluvia · 0.30 Pendiente/Quebrada · 0.20 Suelo · 0.15 Puntos Críticos",
      "Ingesta combinada de Open-Meteo, NASA SRTM 30m e inventario histórico INGEMMET/Provías",
      "Caducidad Fail-Safe automática (STALE_IVV > 3 horas): nunca oculta datos vencidos bajo falso verde",
    ],
    badge: "Precisión MidpointAwayFromZero",
    accent: "amber",
    image: {
      src: "/images/ivv-terrain-diorama.jpg",
      alt: "Diorama geológico 3D de carretera andina sobre talud empinado con lluvia, quebrada hídrica y tensiones geotécnicas",
      width: 1376,
      height: 768,
    },
  },
  {
    id: "failsafe-routing",
    step: "02",
    eyebrow: "Capa 2 · Ruteo Fail-Safe y Multiparada",
    title: "Solo rutas físicamente verificables.",
    summary:
      "El grafo dirigido bidireccional evalúa pendiente ida y vuelta por separado, ancho libre y tonelaje máximo para 5 categorías vehiculares y 3 tipos de carga crítica.",
    highlights: [
      "11 códigos diagnósticos NO_SAFE_ROUTE ordenados por precedencia estricto (Cero rutas inventadas)",
      "Planificación multiparada de 2 a 6 nodos con métricas de tiempo ahorrado, CO₂ y merma agrícola USD",
      "Corredores de transbordo y ventanas humanitarias direccionales con validación geométrica NOT ST_DWithin",
    ],
    badge: "3 Resoluciones Deterministas",
    accent: "emerald",
    image: {
      src: "/images/coer-display-showcase.jpg",
      alt: "Pantalla de mando panorámica mostrando corredores viales en verde, ámbar y rojo sobre topografía oscura",
      width: 1376,
      height: 768,
    },
  },
  {
    id: "offline-consensus",
    step: "03",
    eyebrow: "Capa 3 · Offline-First y Consenso",
    title: "Operación plena sin cobertura celular.",
    summary:
      "Mapas vectoriales regionales en PMTiles v3 y cola transaccional en IndexedDB mantienen a ambulancias y flotas operando en quebradas remotas sin perder un solo reporte.",
    highlights: [
      "4 paquetes regionales PMTiles v3 listos para descarga offline con caché Workbox PWA",
      "Consenso comunitario ponderado por reputación dentro de 500 m (::geography) y ventana de 30 min",
      "Telemetría GPS anónima opt-in con sal criptográfica binaria rotada cada 24 horas fuera de BD",
    ],
    badge: "PMTiles v3 + IndexedDB",
    accent: "blue",
    image: {
      src: "/images/offline-field-tablet.jpg",
      alt: "Tableta de cabina en vehículo 4x4 frente a carretera andina brumosa mostrando mapa topográfico offline",
      width: 1200,
      height: 896,
    },
  },
  {
    id: "coer-b2b",
    step: "04",
    eyebrow: "Capa 4 · Sala COER, Mensajería y B2B",
    title: "Del terreno al ERP en milisegundos.",
    summary:
      "Eventos Server-Sent Events en tiempo real, simulacros en rol de solo lectura, bot WhatsApp/SMS ≤160 caracteres y webhooks empresariales firmados HMAC-SHA256.",
    highlights: [
      "Dual Auth JWT auto-renovable y API Keys con prefijo pxk_live_, hash SHA-256 y rate-limit por cliente",
      "Webhooks salientes anti-SSRF con bloqueo a rangos privados/loopback y firma X-Paradixe-Signature",
      "Simulador COER aislado sobre rol PostgreSQL read-only paradixe_sim_ro sin contaminar datos reales",
    ],
    badge: "46 Endpoints OpenAPI 3.1",
    accent: "indigo",
    image: {
      src: "/images/b2b-network-mesh.jpg",
      alt: "Malla arquitectónica minimalista en cristal oscuro conectando nodos andinos, balizas de salud y enlaces API",
      width: 1376,
      height: 768,
    },
  },
];

export const BENTO_METRICS: BentoMetricCard[] = [
  {
    id: "segments",
    value: "31,622",
    label: "Tramos viales modelados",
    detail:
      "Red topológica dirigida y depurada con cortes por nivel de puente/paso a desnivel sobre Macro-Región Centro.",
    span: "wide",
    accent: "blue",
  },
  {
    id: "certified",
    value: "6,482",
    label: "Ejes pavimentados 100% certificados",
    detail:
      "Validación física completa de ancho, pendiente direccional ida/vuelta y capacidad en toneladas.",
    span: "standard",
    accent: "emerald",
  },
  {
    id: "territory",
    value: "424",
    label: "Distritos en 4 regiones",
    detail: "Huánuco, Junín, Pasco y Ucayali distribuidos en 27 provincias reales.",
    span: "standard",
  },
  {
    id: "hospitals",
    value: "621",
    label: "Centros de salud y hospitales",
    detail: "Georreferenciados y enlazados al cálculo de evacuación médica.",
    span: "standard",
    accent: "emerald",
  },
  {
    id: "failsafe",
    value: "11",
    label: "Códigos Fail-Safe NO_SAFE_ROUTE",
    detail:
      "Jerarquía diagnóstica desde falta de certificación física o datos vencidos hasta red desconectada.",
    span: "standard",
    accent: "amber",
  },
  {
    id: "rust",
    value: "8 Crates",
    label: "Rust con #![forbid(unsafe_code)]",
    detail:
      "Núcleo compilado en memoria segura, binarios CLI JSON reproducibles y zero garbage collector.",
    span: "wide",
    accent: "blue",
  },
  {
    id: "openapi",
    value: "46 Rutas",
    label: "Contrato OpenAPI 3.1.0",
    detail:
      "API REST/SSE tipada de extremo a extremo hacia TypeScript con autenticación Dual JWT / X-API-Key.",
    span: "wide",
  },
  {
    id: "forecast",
    value: "+72 h",
    label: "Horizonte predictivo IVV",
    detail:
      "Proyecciones geotécnicas deterministas a +24h, +48h y +72h con caducidad automática a las 3 horas.",
    span: "wide",
    accent: "amber",
  },
];

export const PROFILE_TABS: ProfileUseCase[] = [
  {
    id: "fleets",
    label: "Flotas y Logística",
    tagline: "Transporte agrícola y carga pesada",
    headline: "Evita retenes, protege perecibles y planifica hasta 6 paradas.",
    description:
      "Calcula desvíos verificados por peso bruto y geometría vial del vehículo. Compara tiempo ahorrado frente a bloqueo, variación real de CO₂ y merma agrícola evitada en dólares antes de despachar.",
    bullets: [
      "Validación estricta por HEAVY_TRAILER, LIGHT_TRUCK, PICKUP_4X4 o MOTORCYCLE",
      "Ruteo multiparada /routing/multi-stop de 2 a 6 destinos con enlaces compartibles",
      "Alertas tempranas por WhatsApp ante incremento de riesgo en Carretera Central o Carpish",
    ],
    metricValue: "2 – 6",
    metricLabel: "Paradas optimizables por despacho con cálculo económico",
    codeTitle: "POST /api/v1/routing/multi-stop (Extracto de respuesta)",
    codeSnippet: `{
  "overall_status": "COMPLETE",
  "vehicle_category_code": "PICKUP_4X4",
  "cargo_type": "PERISHABLE_AGRI",
  "total_distance_km": 48.6,
  "estimated_time_min": 64.2,
  "economic_impact": {
    "time_saved_vs_blockage_min": 240.0,
    "estimated_co2_kg": 14.58,
    "estimated_cargo_loss_avoided_usd": 150.00
  }
}`,
  },
  {
    id: "coer",
    label: "COER y Defensa Civil",
    tagline: "Centro de Operaciones de Emergencia Regional",
    headline: "Sala situacional en vivo, evidencia fotográfica y simulacros sin riesgo.",
    description:
      "Monitorea alertas en tiempo real vía SSE, inspecciona fotografías almacenadas en RustFS con lightbox seguro, audita atributos físicos en un clic y ejecuta simulacros estacionales sin alterar la operación.",
    bullets: [
      "Simulador aislado bajo rol PostgreSQL solo lectura (paradixe_sim_ro) marcado [SIMULATION]",
      "Aprobación y vencimiento auditable de corredores de transbordo y ventanas humanitarias",
      "Gestión centralizada de operadores COER, Defensa Civil y Salud desde Ajustes",
    ],
    metricValue: "100%",
    metricLabel: "Aislamiento PostgreSQL read-only durante simulacros COER",
    codeTitle: "POST /api/v1/routing/calculate (Salida Fail-Safe garantizada)",
    codeSnippet: `{
  "resolution_type": "NO_SAFE_ROUTE",
  "reason_code": "UNVERIFIED_PHYSICAL_ATTRIBUTES",
  "blocking_incident_id": null,
  "warnings": [
    "SEGMENT_MISSING_FIELD_CERTIFICATION"
  ]
}`,
  },
  {
    id: "health",
    label: "Red de Salud y SAMU",
    tagline: "Ambulancias 4×4 e insumos médicos",
    headline: "Referencia hospitalaria segura incluso ante carreteras fracturadas.",
    description:
      "Conecta 621 establecimientos de salud con perfiles específicos para AMBULANCE_4X4, carga MEDICAL_PATIENT y MEDICAL_SUPPLIES, proponiendo rutas seguras o puentes de transbordo cuando un tramo colapsa.",
    bullets: [
      "Puentes peatonales o shuttle aprobados únicamente con inspección sanitaria vigente",
      "Verificación geográfica automática de ausencia de peligro en el tramo de transbordo",
      "Cartografía PMTiles descargada localmente en cabina para zonas sin señal",
    ],
    metricValue: "621",
    metricLabel: "Establecimientos de salud indexados en 4 departamentos",
    codeTitle: "Resolución puente logístico (LOGISTIC_BRIDGE_REQUIRED)",
    codeSnippet: `{
  "resolution_type": "LOGISTIC_BRIDGE_REQUIRED",
  "bridge_proposal": {
    "transfer_mode": "PEDESTRIAN_SAFE_PATH",
    "length_m": 180,
    "operational_authorization": "VERIFIED_INSPECTION_VALID",
    "estimated_transfer_time_min": 20.0
  }
}`,
  },
  {
    id: "b2b",
    label: "Desarrolladores B2B / ERP",
    tagline: "Integración logística e insurtech",
    headline: "API Keys dedicadas y webhooks HMAC inmunes a SSRF.",
    description:
      "Integra el estado en vivo de corredores andinos en tu TMS o ERP mediante llaves X-API-Key con control de tasa por minuto (RPM) y suscríbete a eventos de riesgo críticos firmados criptográficamente.",
    bullets: [
      "Emisión segura (pxk_live_*) almacenada únicamente como hash SHA-256 irrepetible",
      "Bloqueo activo contra URLs locales, 127.0.0.1 o metadatos cloud 169.254.169.254 (HTTP 422)",
      "Verificación de entregas con cabecera X-Paradixe-Signature (HMAC-SHA256)",
    ],
    metricValue: "HMAC-256",
    metricLabel: "Firma verificable en cada notificación webhook saliente",
    codeTitle: "Consulta batch de corredores B2B vía X-API-Key",
    codeSnippet: `curl -X POST https://api.paradixe.pe/api/v1/b2b/corridors/status \\
  -H "X-API-Key: pxk_live_••••••••••••••••" \\
  -H "Content-Type: application/json" \\
  -d '{"region":"PE-JUN","min_risk_level":"AMBER","limit":25}'`,
  },
];

export const MODULE_SLIDES: ModuleSlide[] = [
  {
    id: "sse-live",
    eyebrow: "Tiempo Real",
    title: "Sala Situacional SSE",
    summary:
      "Stream persistente Server-Sent Events que actualiza incidentes, incidentes sospechosos y tableros sin recargas ni polling agresivo.",
    specs: ["GET /api/v1/events/stream", "Modo Kiosco COER", "Auto-reconexión"],
    image: {
      src: "/images/coer-display-showcase.jpg",
      alt: "Visualización de sala situacional COER sobre pantalla panorámica",
      width: 1376,
      height: 768,
    },
  },
  {
    id: "rustfs-evidence",
    eyebrow: "Evidencia S3",
    title: "Fotografías en Campo y Lightbox",
    summary:
      "Carga binaria comprimida verificada por magic-bytes hacia RustFS con URLs prefirmadas SigV4 y visor modal accesible con light-dismiss.",
    specs: ["Magic-bytes check", "RustFS S3 compatible", "<dialog closedby=\"any\">"],
    image: {
      src: "/images/offline-field-tablet.jpg",
      alt: "Operación en terreno cargando evidencia visual offline",
      width: 1200,
      height: 896,
    },
  },
  {
    id: "whatsapp-sms",
    eyebrow: "Cobertura Rural",
    title: "Bot WhatsApp y Fallback SMS",
    summary:
      "Parser en español adaptado a zonas altoandinas con respuestas compactas ≤160 caracteres para teléfonos básicos sin plan de datos.",
    specs: ["WhatsApp Cloud API", "SMS <= 160 chars", "Firma constante HMAC"],
    image: {
      src: "/images/hero-andes-corridor.jpg",
      alt: "Red de conectividad rural a través de corredores de montaña",
      width: 1376,
      height: 768,
    },
  },
  {
    id: "telemetry-privacy",
    eyebrow: "Privacidad por Diseño",
    title: "Telemetría GPS Anónima",
    summary:
      "La clave binaria HMAC diaria vive fuera de PostgreSQL en telemetry_keys/; al rotar cada noche, es matemáticamente imposible reconstruir historiales.",
    specs: [">=10 vehículos <=3 km/h", "Solo estado SUSPECTED", "Purga 24h"],
    image: {
      src: "/images/ivv-terrain-diorama.jpg",
      alt: "Corte geológico andino con monitoreo pasivo de velocidad vehicular",
      width: 1376,
      height: 768,
    },
  },
  {
    id: "observability",
    eyebrow: "Observabilidad",
    title: "Prometheus, Grafana y Backups",
    summary:
      "Métricas /metrics expuestas desde Axum, reglas de alertas configurables y scripts pg_dump comprimidos validados mediante suma SHA-256.",
    specs: ["Prometheus /metrics", "Dashboard COER JSON", "backup_postgis.sh"],
    image: {
      src: "/images/b2b-network-mesh.jpg",
      alt: "Infraestructura observada y nodos distribuidos de red",
      width: 1376,
      height: 768,
    },
  },
];

export const RUST_CRATES: RustCrateSpec[] = [
  {
    name: "paradixe-domain",
    responsibility:
      "Tipos algebraicos SDD, 11 códigos de precedencia y redondeo MidpointAwayFromZero.",
    guarantee: "#![forbid(unsafe_code)] · Tests SDD",
  },
  {
    name: "paradixe-ivv",
    responsibility:
      "Motor geotécnico IVV determinista y expiración automática > 3 horas (STALE_IVV).",
    guarantee: "Aritmética decimal exact-match PostgreSQL",
  },
  {
    name: "paradixe-consensus",
    responsibility:
      "Deduplicación comunitaria en 30 min / 500 m y clúster GPS con hash HMAC diario.",
    guarantee: "Telemetría jamás cierra vía por sí sola",
  },
  {
    name: "paradixe-routing",
    responsibility:
      "Grafo dirigido Dijkstra, poda vehicular/tonelaje y verificación de puentes logísticos.",
    guarantee: "Fail-Safe estricto ante datos incompletos",
  },
  {
    name: "paradixe-impact",
    responsibility:
      "Evaluador económico de tiempo, CO₂ neto y merma agrícola perecible evitada.",
    guarantee: "Preserva deltas negativos sin truncar",
  },
  {
    name: "paradixe-api",
    responsibility:
      "Servidor Axum + SQLx con 46 rutas OpenAPI 3.1, JWT, Argon2id, B2B y SSE.",
    guarantee: "Nginx CSP estricta + Clear-Site-Data",
  },
  {
    name: "paradixe-ingestor",
    responsibility:
      "Pipeline OSM Geofabrik, SRTM 30m, Open-Meteo, INGEMMET hotspots y PMTiles v3.",
    guarantee: "Escala por región y --bbox a RustFS",
  },
  {
    name: "paradixe-messaging",
    responsibility:
      "Pasarela WhatsApp Cloud API + SMS ≤160 chars con parser ciudadano en español.",
    guarantee: "Puente HMAC + anti-replay",
  },
];

export const REGION_COVERAGE: RegionCoverage[] = [
  {
    iso: "PE-HUC",
    name: "Huánuco",
    segments: "10,694",
    pavedCertified: "1,369 / 1,369 (100%)",
    hotspots: "401",
  },
  {
    iso: "PE-JUN",
    name: "Junín",
    segments: "19,622",
    pavedCertified: "2,985 / 2,985 (100%)",
    hotspots: "540",
  },
  {
    iso: "PE-PAS",
    name: "Pasco",
    segments: "9,795",
    pavedCertified: "953 / 953 (100%)",
    hotspots: "462",
  },
  {
    iso: "PE-UCA",
    name: "Ucayali",
    segments: "10,669",
    pavedCertified: "1,175 / 1,175 (100%)",
    hotspots: "333",
  },
];

export interface IvvCalcInput {
  precip24hMm: number;
  precip72hMm: number;
  slopePct: number;
  ravineDistM: number;
  soilIdx: number;
  isHotspot: boolean;
}

export interface IvvCalcResult {
  score: number;
  risk: "GREEN" | "AMBER" | "RED";
  p: number;
  s: number;
  g: number;
  h: number;
}

/**
 * Replica exacta de la fórmula determinista en `crates/paradixe-ivv/src/lib.rs`:
 * - p = 0.6 * clamp01(precip_24h / 80) + 0.4 * clamp01(precip_72h / 180)
 * - s = 0.5 * clamp01(slope / 45) + 0.5 * (1 - clamp01(ravine_dist / 500))
 * - g = clamp01(soil)
 * - h = isHotspot ? 1.0 : 0.2
 * - score = roundHalfAwayFromZero(100 * (0.35*p + 0.30*s + 0.20*g + 0.15*h), 2)
 */
export function computeDeterministicIvv(input: IvvCalcInput): IvvCalcResult {
  const clamp01 = (v: number) => Math.max(0, Math.min(1, v));
  const safe72 = Math.max(input.precip72hMm, input.precip24hMm);

  const p24 = clamp01(input.precip24hMm / 80);
  const p72 = clamp01(safe72 / 180);
  const p = 0.6 * p24 + 0.4 * p72;

  const sSlope = clamp01(input.slopePct / 45);
  const sRavine = 1 - clamp01(input.ravineDistM / 500);
  const s = 0.5 * sSlope + 0.5 * sRavine;

  const g = clamp01(input.soilIdx);
  const h = input.isHotspot ? 1.0 : 0.2;

  const raw = 100 * (0.35 * p + 0.3 * s + 0.2 * g + 0.15 * h);
  const score = Math.round((raw + Number.EPSILON) * 100) / 100;

  let risk: "GREEN" | "AMBER" | "RED" = "GREEN";
  if (score >= 70) {
    risk = "RED";
  } else if (score >= 40) {
    risk = "AMBER";
  }

  return { score, risk, p, s, g, h };
}
