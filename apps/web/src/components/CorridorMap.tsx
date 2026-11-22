import { useEffect, useId, useMemo, useRef, useState } from "react";
import maplibregl from "maplibre-gl";
import { Protocol } from "pmtiles";
import type { CorridorSegmentFeature } from "../api/client.ts";
import { RiskBadge } from "./RiskBadge.tsx";

export interface CorridorMapProps {
  features: CorridorSegmentFeature[];
  selectedSegmentId: string | null;
  onSelectSegment: (segmentId: string) => void;
  pmtilesUrl?: string | null;
  focusBounds?: [[number, number], [number, number]] | null;
  resolvedScheme?: "light" | "dark";
  regionLabel?: string;
}

const PASSABILITY_LABELS: Record<
  CorridorSegmentFeature["properties"]["passability_state"],
  string
> = {
  OPEN: "Abierto",
  CAUTION_SUSPECTED: "Sospecha reportada",
  PARTIAL_ONE_LANE: "Paso restringido (1 carril)",
  BLOCKED: "Bloqueado",
};

const RISK_COLORS: Record<CorridorSegmentFeature["properties"]["risk_level"], string> = {
  GREEN: "#15803d",
  AMBER: "#d97706",
  RED: "#dc2626",
  UNKNOWN: "#64748b",
};

const PAGE_SIZE = 25;
let protocolRegistered = false;

export function resolveClientPmtilesUrl(explicitUrl?: string | null): string | null {
  const envUrl = import.meta.env?.VITE_PMTILES_URL as string | undefined;
  const candidate =
    typeof envUrl === "string" && envUrl.trim().length > 0
      ? envUrl.trim()
      : explicitUrl && explicitUrl.trim().length > 0
        ? explicitUrl.trim()
        : "/tiles/pe-huc.pmtiles";

  if (/^https?:\/\/(localhost|127\.0\.0\.1):9000\/paradixe-tiles\//i.test(candidate)) {
    return candidate.replace(
      /^https?:\/\/(localhost|127\.0\.0\.1):9000\/paradixe-tiles\//i,
      "/tiles/"
    );
  }
  return candidate;
}

function toPmtilesProtocolSourceUrl(urlPathOrHttp: string): string {
  if (typeof window !== "undefined" && window.location?.href) {
    try {
      return `pmtiles://${new URL(urlPathOrHttp, window.location.href).toString()}`;
    } catch {
      return `pmtiles://${urlPathOrHttp}`;
    }
  }
  return `pmtiles://${urlPathOrHttp}`;
}

export function CorridorMap({
  features,
  selectedSegmentId,
  onSelectSegment,
  pmtilesUrl,
  focusBounds,
  resolvedScheme = "light",
  regionLabel = "Huánuco y Red Territorial",
}: CorridorMapProps) {
  const mapContainerRef = useRef<HTMLDivElement | null>(null);
  const mapInstanceRef = useRef<maplibregl.Map | null>(null);
  const tableCaptionId = useId();
  const searchInputId = useId();
  const riskSelectId = useId();

  const [searchTerm, setSearchTerm] = useState("");
  const [riskFilter, setRiskFilter] = useState<string>("ALL");
  const [pageIndex, setPageIndex] = useState(0);

  const effectivePmtilesUrl = resolveClientPmtilesUrl(pmtilesUrl);

  useEffect(() => {
    const container = mapContainerRef.current;
    if (!container) return;

    if (!protocolRegistered) {
      const protocol = new Protocol();
      maplibregl.addProtocol("pmtiles", protocol.tile);
      protocolRegistered = true;
    }

    const bgColor = resolvedScheme === "dark" ? "#0f172a" : "#e2e8f0";
    const baseStyle: maplibregl.StyleSpecification = effectivePmtilesUrl
      ? {
          version: 8,
          sources: {
            territorial: {
              type: "vector",
              url: toPmtilesProtocolSourceUrl(effectivePmtilesUrl),
            },
          },
          layers: [
            {
              id: "background",
              type: "background",
              paint: { "background-color": bgColor },
            },
          ],
        }
      : {
          version: 8,
          sources: {},
          layers: [
            {
              id: "background",
              type: "background",
              paint: { "background-color": bgColor },
            },
          ],
        };

    const map = new maplibregl.Map({
      container,
      style: baseStyle,
      center: [-76.12, -9.68],
      zoom: 8.2,
      attributionControl: { compact: true },
    });

    map.addControl(new maplibregl.NavigationControl(), "top-right");

    map.on("load", () => {
      if (effectivePmtilesUrl) {
        map.addLayer({
          id: "pmtiles-corridors",
          type: "line",
          source: "territorial",
          "source-layer": "corridors",
          layout: {
            "line-cap": "round",
            "line-join": "round",
          },
          paint: {
            "line-width": ["interpolate", ["linear"], ["zoom"], 6, 1.5, 10, 3.5],
            "line-opacity": 0.55,
            "line-color": [
              "case",
              ["==", ["get", "passability_state"], "BLOCKED"],
              RISK_COLORS.RED,
              [">=", ["coalesce", ["get", "ivv_score"], 0], 70],
              RISK_COLORS.RED,
              [">=", ["coalesce", ["get", "ivv_score"], 0], 40],
              RISK_COLORS.AMBER,
              RISK_COLORS.GREEN,
            ],
          },
        });

        map.addLayer({
          id: "pmtiles-districts-points",
          type: "circle",
          source: "territorial",
          "source-layer": "districts",
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 6, 2.5, 10, 5],
            "circle-color": "#0284c7",
            "circle-stroke-width": 1,
            "circle-stroke-color": "#ffffff",
            "circle-opacity": 0.75,
          },
        });

        map.addLayer({
          id: "pmtiles-nodes-towns",
          type: "circle",
          source: "territorial",
          "source-layer": "nodes",
          filter: ["==", ["get", "node_type"], "TOWN_CENTER"],
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 6, 2, 10, 4.5],
            "circle-color": "#0f766e",
            "circle-stroke-width": 1,
            "circle-stroke-color": "#ffffff",
          },
        });

        map.addLayer({
          id: "pmtiles-nodes-hospitals",
          type: "circle",
          source: "territorial",
          "source-layer": "nodes",
          filter: ["==", ["get", "node_type"], "HOSPITAL"],
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 6, 3.5, 10, 6.5],
            "circle-color": "#be123c",
            "circle-stroke-width": 1.5,
            "circle-stroke-color": "#ffffff",
          },
        });
      }

      map.addSource("corridors-geojson", {
        type: "geojson",
        data: {
          type: "FeatureCollection",
          features: [],
        },
      });

      map.addLayer({
        id: "corridors-line",
        type: "line",
        source: "corridors-geojson",
        layout: {
          "line-cap": "round",
          "line-join": "round",
        },
        paint: {
          "line-width": ["case", ["==", ["get", "passability_state"], "BLOCKED"], 6, 4.2],
          "line-color": [
            "match",
            ["get", "effective_risk"],
            "GREEN",
            RISK_COLORS.GREEN,
            "AMBER",
            RISK_COLORS.AMBER,
            "RED",
            RISK_COLORS.RED,
            RISK_COLORS.UNKNOWN,
          ],
        },
      });

      map.addLayer({
        id: "corridors-selected-line",
        type: "line",
        source: "corridors-geojson",
        filter: ["==", ["get", "segment_id"], ""],
        layout: {
          "line-cap": "round",
          "line-join": "round",
        },
        paint: {
          "line-width": 8,
          "line-color": "#0ea5e9",
          "line-opacity": 0.9,
        },
      });

      const handleClick = (event: maplibregl.MapLayerMouseEvent) => {
        const clickedId = event.features?.[0]?.properties?.segment_id;
        if (typeof clickedId === "string" && clickedId.length > 0) {
          onSelectSegment(clickedId);
        }
      };

      map.on("click", "corridors-line", handleClick);
      if (effectivePmtilesUrl) {
        map.on("click", "pmtiles-corridors", handleClick);
      }
    });

    mapInstanceRef.current = map;
    return () => {
      map.remove();
      mapInstanceRef.current = null;
    };
  }, [onSelectSegment, effectivePmtilesUrl]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map) return;
    if (typeof map.getLayer === "function" && map.getLayer("background")) {
      map.setPaintProperty?.(
        "background",
        "background-color",
        resolvedScheme === "dark" ? "#0f172a" : "#e2e8f0"
      );
    }
  }, [resolvedScheme]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map || !focusBounds) return;
    map.fitBounds?.(focusBounds, { padding: 36, maxZoom: 12, duration: 500 });
  }, [focusBounds]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map) return;
    const source = map.getSource("corridors-geojson") as maplibregl.GeoJSONSource | undefined;
    if (!source) return;

    const enriched = features.map((f) => ({
      ...f,
      properties: {
        ...f.properties,
        effective_risk: f.properties.is_stale ? "UNKNOWN" : f.properties.risk_level,
      },
    }));

    source.setData({
      type: "FeatureCollection",
      features: enriched,
    });
  }, [features]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map) return;
    if (typeof map.getLayer === "function" && map.getLayer("corridors-selected-line")) {
      map.setFilter?.("corridors-selected-line", [
        "==",
        ["get", "segment_id"],
        selectedSegmentId ?? "",
      ]);
    }
  }, [selectedSegmentId]);

  const filteredFeatures = useMemo(() => {
    const q = searchTerm.trim().toLowerCase();
    return features.filter((f) => {
      const p = f.properties;
      const effRisk = p.is_stale ? "UNKNOWN" : p.risk_level;
      if (riskFilter !== "ALL" && effRisk !== riskFilter) {
        return false;
      }
      if (q.length > 0) {
        return (
          p.corridor_name.toLowerCase().includes(q) ||
          p.segment_id.toLowerCase().includes(q)
        );
      }
      return true;
    });
  }, [features, searchTerm, riskFilter]);

  const totalPages = Math.max(1, Math.ceil(filteredFeatures.length / PAGE_SIZE));
  const safePageIndex = Math.min(pageIndex, totalPages - 1);
  const pageFeatures = filteredFeatures.slice(
    safePageIndex * PAGE_SIZE,
    safePageIndex * PAGE_SIZE + PAGE_SIZE
  );

  return (
    <section className="map-section" aria-labelledby="map-section-title">
      <div className="card">
        <div className="map-card-header">
          <h2 id="map-section-title" style={{ marginBlockEnd: 0 }}>
            Mapa Territorial — {regionLabel}
          </h2>
          <div className="map-legend" aria-label="Leyenda cartográfica">
            <span className="legend-dot legend-dot--hospital">Hospitales</span>
            <span className="legend-dot legend-dot--town">Centros poblados</span>
            <span className="legend-dot legend-dot--district">Distritos</span>
          </div>
        </div>
        <div
          ref={mapContainerRef}
          className="map-frame"
          role="region"
          aria-label={`Mapa interactivo de resiliencia vial ${regionLabel}`}
        />
      </div>

      <div className="card">
        <div className="table-filter-bar">
          <div className="field" style={{ marginBlockEnd: 0, flex: "1 1 220px" }}>
            <label htmlFor={searchInputId}>Buscar corredor vial</label>
            <input
              id={searchInputId}
              type="search"
              placeholder="Ej. Carpish, Tingo María, Ambo..."
              value={searchTerm}
              onChange={(e) => {
                setSearchTerm(e.target.value);
                setPageIndex(0);
              }}
            />
          </div>

          <div className="field" style={{ marginBlockEnd: 0, flex: "0 1 200px" }}>
            <label htmlFor={riskSelectId}>Filtrar riesgo IVV</label>
            <select
              id={riskSelectId}
              value={riskFilter}
              onChange={(e) => {
                setRiskFilter(e.target.value);
                setPageIndex(0);
              }}
            >
              <option value="ALL">Todos ({features.length})</option>
              <option value="RED">RED (Alto)</option>
              <option value="AMBER">AMBER (Moderado)</option>
              <option value="GREEN">GREEN (Bajo)</option>
              <option value="UNKNOWN">UNKNOWN / STALE</option>
            </select>
          </div>
        </div>

        <div tabIndex={0} role="region" aria-labelledby={tableCaptionId} className="table-wrapper">
          <table className="corridor-table">
            <caption id={tableCaptionId}>
              Estado geotécnico y transitabilidad por tramo vial ({filteredFeatures.length} tramos)
            </caption>
            <thead>
              <tr>
                <th scope="col">Corredor</th>
                <th scope="col">Longitud</th>
                <th scope="col">Transitabilidad</th>
                <th scope="col">Riesgo IVV</th>
                <th scope="col">Acción</th>
              </tr>
            </thead>
            <tbody>
              {pageFeatures.length === 0 ? (
                <tr>
                  <td colSpan={5}>
                    {features.length === 0
                      ? "No hay tramos sincronizados en caché local. Conéctese para descargar la red vial verificada."
                      : "Ningún corredor coincide con el filtro de búsqueda actual."}
                  </td>
                </tr>
              ) : (
                pageFeatures.map((f) => {
                  const p = f.properties;
                  const isSelected = selectedSegmentId === p.segment_id;
                  return (
                    <tr key={p.segment_id}>
                      <th scope="row">
                        <strong>{p.corridor_name}</strong>
                        {!p.is_attributes_verified && (
                          <div className="hint">Atributos físicos sin verificación de campo</div>
                        )}
                      </th>
                      <td>{p.length_km.toFixed(1)} km</td>
                      <td>{PASSABILITY_LABELS[p.passability_state]}</td>
                      <td>
                        <RiskBadge
                          riskLevel={p.risk_level}
                          ivvScore={p.ivv_score}
                          isStale={p.is_stale}
                        />
                      </td>
                      <td>
                        <button
                          type="button"
                          onClick={() => onSelectSegment(p.segment_id)}
                          aria-pressed={isSelected}
                        >
                          {isSelected ? "Seleccionado" : "Inspeccionar"}
                        </button>
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>

        {totalPages > 1 && (
          <nav className="pagination-bar" aria-label="Paginación de tramos viales">
            <button
              type="button"
              disabled={safePageIndex === 0}
              onClick={() => setPageIndex((p) => Math.max(0, p - 1))}
            >
              Anterior
            </button>
            <span className="hint" style={{ marginBlockEnd: 0 }}>
              Página {safePageIndex + 1} de {totalPages}
            </span>
            <button
              type="button"
              disabled={safePageIndex >= totalPages - 1}
              onClick={() => setPageIndex((p) => Math.min(totalPages - 1, p + 1))}
            >
              Siguiente
            </button>
          </nav>
        )}
      </div>
    </section>
  );
}
