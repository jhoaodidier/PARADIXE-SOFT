import { useId, useState, type FormEvent } from "react";
import {
  ApiClientError,
  postRoutingCalculate,
  type CargoType,
  type CorridorSegmentFeature,
  type NoSafeRouteReasonCode,
  type RoutingCalculateResponse,
  type VehicleCategoryCode,
} from "../api/client.ts";

export interface RoutingPanelProps {
  features: CorridorSegmentFeature[];
  isOnline: boolean;
  authToken: string | null;
}

const VEHICLE_OPTIONS: Array<{ code: VehicleCategoryCode; label: string }> = [
  { code: "AMBULANCE_4X4", label: "Ambulancia 4x4 (SAMU / Posta)" },
  { code: "PICKUP_4X4", label: "Camioneta 4x4" },
  { code: "LIGHT_TRUCK", label: "Camión agrícola ligero" },
  { code: "HEAVY_TRAILER", label: "Tráiler de carga pesada" },
  { code: "MOTORCYCLE", label: "Motocicleta de última milla" },
];

const CARGO_OPTIONS: Array<{ code: CargoType; label: string }> = [
  { code: "MEDICAL_PATIENT", label: "Paciente médico crítico" },
  { code: "MEDICAL_SUPPLIES", label: "Insumos médicos" },
  { code: "PERISHABLE_AGRI", label: "Carga agrícola perecible" },
];

const REASON_DESCRIPTIONS: Record<NoSafeRouteReasonCode, string> = {
  UNVERIFIED_PHYSICAL_ATTRIBUTES:
    "Existen tramos sin validación física de ancho, pendiente o capacidad portante.",
  STALE_IVV:
    "El Índice de Vulnerabilidad Vial (IVV) vigente caducó o falta observación climática.",
  VEHICLE_RESTRICTION:
    "La categoría vehicular excede pendiente, ancho, peso o restricción de trocha.",
  TRANSFER_REVALIDATION_REQUIRED:
    "El corredor de transbordo requiere revalidación tras evento geotécnico reciente.",
  TRANSFER_INSPECTION_EXPIRED:
    "La inspección física del corredor de transbordo expiró.",
  HAZARD_INTERSECTS_TRANSFER_PATH:
    "El radio geotécnico de peligro intersecta el corredor de transbordo.",
  TRANSFER_IVV_EXCEEDED:
    "El IVV del sector supera el umbral seguro del corredor de transbordo.",
  TRANSFER_CARGO_CAPACITY_EXCEEDED:
    "El peso de carga supera la capacidad máxima verificada del puente logístico.",
  TRANSFER_CARGO_NOT_ALLOWED:
    "El corredor de transbordo no está habilitado para este tipo de carga o paciente.",
  NO_VERIFIED_TRANSFER_CORRIDOR:
    "No existe corredor de transbordo inspeccionado disponible entre ambos lados.",
  NETWORK_DISCONNECTED:
    "No existe conectividad verificada entre el nodo de origen y destino.",
};

export function RoutingPanel({ features, isOnline, authToken }: RoutingPanelProps) {
  const formTitleId = useId();
  const payloadErrorId = useId();
  const payloadHintId = useId();

  const nodeOptions = Array.from(
    new Map(
      features.flatMap((f) => [
        [f.properties.origin_node_id, `Nodo origen (${f.properties.corridor_name})`],
        [f.properties.destination_node_id, `Nodo destino (${f.properties.corridor_name})`],
      ])
    ).entries()
  );

  const [originNodeId, setOriginNodeId] = useState("");
  const [destinationNodeId, setDestinationNodeId] = useState("");
  const [vehicleCode, setVehicleCode] = useState<VehicleCategoryCode>("AMBULANCE_4X4");
  const [cargoType, setCargoType] = useState<CargoType>("MEDICAL_PATIENT");
  const [cargoPayloadKg, setCargoPayloadKg] = useState("1200");
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [result, setResult] = useState<RoutingCalculateResponse | null>(null);

  const effectiveOrigin = originNodeId || nodeOptions[0]?.[0] || "";
  const effectiveDest =
    destinationNodeId || nodeOptions[nodeOptions.length - 1]?.[0] || nodeOptions[0]?.[0] || "";

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setErrorMsg(null);
    if (!isOnline) {
      setErrorMsg(
        "El cálculo de nueva ruta requiere conexión en línea. Las rutas en caché offline son únicamente informativas."
      );
      return;
    }
    if (!effectiveOrigin || !effectiveDest) {
      setErrorMsg("Debe seleccionar un nodo de origen y destino válidos.");
      return;
    }

    setSubmitting(true);
    try {
      const parsedWeight = Number(cargoPayloadKg);
      const response = await postRoutingCalculate(
        {
          origin_node_id: effectiveOrigin,
          destination_node_id: effectiveDest,
          vehicle_category_code: vehicleCode,
          cargo_type: cargoType,
          ...(cargoType === "MEDICAL_PATIENT"
            ? { patient_count: 1 }
            : { cargo_payload_kg: parsedWeight }),
        },
        authToken
      );
      setResult(response);
    } catch (err) {
      if (err instanceof ApiClientError) {
        setErrorMsg(`${err.errorCode}: ${err.message}`);
      } else {
        setErrorMsg("No fue posible consultar el enrutador. Verifique conectividad y sesión.");
      }
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="card" aria-labelledby={formTitleId}>
      <h2 id={formTitleId}>Cálculo de Ruta de Apoyo a Decisión</h2>
      <p className="hint">
        Toda propuesta exige autorización operativa humana; nunca reemplaza al COER, Defensa Civil ni
        personal sanitario.
      </p>

      {!isOnline && (
        <div className="banner banner--warning" role="status">
          <span>
            Modo sin conexión: cálculo de nueva ruta deshabilitado. Consulte solo tramos en caché.
          </span>
        </div>
      )}

      <form onSubmit={handleSubmit} aria-labelledby={formTitleId}>
        <div className="field">
          <label htmlFor="route-origin">Nodo origen</label>
          <select
            id="route-origin"
            name="origin_node_id"
            required
            value={effectiveOrigin}
            onChange={(e) => setOriginNodeId(e.target.value)}
            data-error="origin-error"
          >
            {nodeOptions.length === 0 ? (
              <option value="">Sin nodos disponibles</option>
            ) : (
              nodeOptions.map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))
            )}
          </select>
          <span id="origin-error" className="error-msg">
            <span role="img" aria-label="Error">
              ⚠
            </span>{" "}
            Seleccione un nodo de origen.
          </span>
        </div>

        <div className="field">
          <label htmlFor="route-dest">Nodo destino</label>
          <select
            id="route-dest"
            name="destination_node_id"
            required
            value={effectiveDest}
            onChange={(e) => setDestinationNodeId(e.target.value)}
            data-error="dest-error"
          >
            {nodeOptions.length === 0 ? (
              <option value="">Sin nodos disponibles</option>
            ) : (
              nodeOptions.map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))
            )}
          </select>
          <span id="dest-error" className="error-msg">
            <span role="img" aria-label="Error">
              ⚠
            </span>{" "}
            Seleccione un nodo de destino.
          </span>
        </div>

        <fieldset>
          <legend>Categoría vehicular</legend>
          <div className="radio-group">
            {VEHICLE_OPTIONS.map((opt) => (
              <label key={opt.code} className="radio-option">
                <input
                  type="radio"
                  name="vehicle_category_code"
                  value={opt.code}
                  checked={vehicleCode === opt.code}
                  onChange={() => setVehicleCode(opt.code)}
                />
                <span>{opt.label}</span>
              </label>
            ))}
          </div>
        </fieldset>

        <fieldset>
          <legend>Tipo de carga o traslado</legend>
          <div className="radio-group">
            {CARGO_OPTIONS.map((opt) => (
              <label key={opt.code} className="radio-option">
                <input
                  type="radio"
                  name="cargo_type"
                  value={opt.code}
                  checked={cargoType === opt.code}
                  onChange={() => setCargoType(opt.code)}
                />
                <span>{opt.label}</span>
              </label>
            ))}
          </div>
        </fieldset>

        {cargoType !== "MEDICAL_PATIENT" && (
          <div className="field">
            <label htmlFor="cargo-payload-kg">Peso de carga (kg)</label>
            <span id={payloadHintId} className="hint">
              Requerido para verificar capacidad real de puente logístico (kg &gt; 0).
            </span>
            <input
              id="cargo-payload-kg"
              name="cargo_payload_kg"
              type="text"
              inputMode="decimal"
              required
              pattern="^[1-9]\d*(\.\d+)?$"
              value={cargoPayloadKg}
              onChange={(e) => setCargoPayloadKg(e.target.value)}
              aria-describedby={payloadHintId}
              data-error={payloadErrorId}
            />
            <span id={payloadErrorId} className="error-msg">
              <span role="img" aria-label="Error">
                ⚠
              </span>{" "}
              Ingrese un peso positivo en kilogramos (ej. 1200).
            </span>
          </div>
        )}

        <button
          type="submit"
          className="btn btn--primary"
          disabled={submitting || !isOnline || nodeOptions.length === 0}
        >
          {submitting ? "Evaluando ruta segura..." : "Calcular ruta verificable"}
        </button>
      </form>

      {errorMsg && (
        <div className="banner banner--danger" role="alert" style={{ marginBlockStart: "1rem" }}>
          <span>{errorMsg}</span>
        </div>
      )}

      {result && (
        <div className="card" style={{ marginBlockStart: "1rem" }} aria-live="polite">
          <h3>Resultado de Enrutamiento</h3>
          {result.resolution_type === "ALTERNATE_ROUTE" && (
            <div>
              <p>
                <strong>Ruta Alterna Propuesta</strong> ({result.routing_config_tag}) —{" "}
                <strong>Autorización operativa: {result.operational_authorization}</strong>
              </p>
              <div className="metrics-grid">
                <div className="metric-box">
                  <span className="metric-label">Distancia total</span>
                  <span className="metric-value">{result.total_distance_km.toFixed(1)} km</span>
                </div>
                <div className="metric-box">
                  <span className="metric-label">ETA nominal ({result.eta_quality})</span>
                  <span className="metric-value">{result.estimated_time_min} min</span>
                </div>
                <div className="metric-box">
                  <span className="metric-label">IVV máximo en ruta</span>
                  <span className="metric-value">{result.max_ivv_on_route.toFixed(2)}</span>
                </div>
                <div className="metric-box">
                  <span className="metric-label">Tramos direccionales</span>
                  <span className="metric-value">{result.hops.length}</span>
                </div>
              </div>
            </div>
          )}

          {result.resolution_type === "LOGISTIC_BRIDGE_REQUIRED" && (
            <div>
              <p>
                <strong>Puente Logístico Propuesto (PROPOSED)</strong> —{" "}
                <strong>Autorización operativa: {result.bridge_proposal.operational_authorization}</strong>
              </p>
              <p className="hint">
                Modo: {result.bridge_proposal.transfer_mode} · Longitud:{" "}
                {result.bridge_proposal.length_m} m · Tiempo estimado de relevo:{" "}
                {result.bridge_proposal.estimated_transfer_time_min} min · Inspección válida hasta{" "}
                {new Date(result.bridge_proposal.inspection_expires_at).toLocaleDateString("es-PE")}
              </p>
            </div>
          )}

          {result.resolution_type === "NO_SAFE_ROUTE" && (
            <div className="banner banner--danger" role="alert">
              <div>
                <strong>NO_SAFE_ROUTE ({result.reason_code})</strong>
                <p style={{ marginBlock: "0.25rem 0" }}>
                  No hay una solución verificable con los datos disponibles:{" "}
                  {REASON_DESCRIPTIONS[result.reason_code]}
                </p>
              </div>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
