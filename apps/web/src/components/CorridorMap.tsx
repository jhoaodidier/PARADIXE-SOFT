import { useEffect, useRef } from "react";
import maplibregl from "maplibre-gl";
import { Protocol } from "pmtiles";
import type { CorridorSegmentFeature } from "../api/client.ts";
import { RiskBadge } from "./RiskBadge.tsx";

export interface CorridorMapProps {
  features: CorridorSegmentFeature[];
  selectedSegmentId: string | null;
  onSelectSegment: (segmentId: string) => void;
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

let protocolRegistered = false;

export function CorridorMap({
  features,
  selectedSegmentId,
  onSelectSegment,
}: CorridorMapProps) {
  const mapContainerRef = useRef<HTMLDivElement | null>(null);
  const mapInstanceRef = useRef<maplibregl.Map | null>(null);

  useEffect(() => {
    const container = mapContainerRef.current;
    if (!container || mapInstanceRef.current) return;

    if (!protocolRegistered) {
      const protocol = new Protocol();
      maplibregl.addProtocol("pmtiles", protocol.tile);
      protocolRegistered = true;
    }

    const pmtilesUrl = import.meta.env?.VITE_PMTILES_URL as string | undefined;
    const baseStyle: maplibregl.StyleSpecification = pmtilesUrl
      ? {
          version: 8,
          sources: {
            basemap: {
              type: "vector",
              url: `pmtiles://${pmtilesUrl}`,
            },
          },
          layers: [
            {
              id: "background",
              type: "background",
              paint: { "background-color": "#e2e8f0" },
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
              paint: { "background-color": "#dbe4ee" },
            },
          ],
        };

    const map = new maplibregl.Map({
      container,
      style: baseStyle,
      center: [-76.12, -9.68],
      zoom: 9.5,
      attributionControl: { compact: true },
    });

    map.addControl(new maplibregl.NavigationControl(), "top-right");

    map.on("load", () => {
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
          "line-width": ["case", ["==", ["get", "passability_state"], "BLOCKED"], 6, 4.5],
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

      map.on("click", "corridors-line", (event) => {
        const clickedId = event.features?.[0]?.properties?.segment_id;
        if (typeof clickedId === "string") {
          onSelectSegment(clickedId);
        }
      });
    });

    mapInstanceRef.current = map;
    return () => {
      map.remove();
      mapInstanceRef.current = null;
    };
  }, [onSelectSegment]);

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

  return (
    <section className="map-section" aria-labelledby="map-section-title">
      <div className="card">
        <h2 id="map-section-title">Mapa Territorial del Corredor Vial</h2>
        <div
          ref={mapContainerRef}
          className="map-frame"
          role="region"
          aria-label="Mapa interactivo de resiliencia vial Huánuco – Carpish – Tingo María"
        />
      </div>

      <div className="card">
        <div className="table-scroll">
          <table className="corridor-table">
            <caption>Estado geotécnico y transitabilidad por tramo vial</caption>
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
              {features.length === 0 ? (
                <tr>
                  <td colSpan={5}>
                    No hay tramos sincronizados en caché local. Conéctese para descargar la red
                    vial verificada.
                  </td>
                </tr>
              ) : (
                features.map((f) => {
                  const p = f.properties;
                  const isSelected = selectedSegmentId === p.segment_id;
                  return (
                    <tr key={p.segment_id}>
                      <td>
                        <strong>{p.corridor_name}</strong>
                        {!p.is_attributes_verified && (
                          <div className="hint">Atributos físicos sin verificación de campo</div>
                        )}
                      </td>
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
      </div>
    </section>
  );
}
