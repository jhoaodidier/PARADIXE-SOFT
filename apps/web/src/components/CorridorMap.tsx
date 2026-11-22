import { useEffect, useId, useMemo, useRef, useState } from "react";
import maplibregl from "maplibre-gl";
import { Protocol } from "pmtiles";
import type { CorridorSegmentFeature } from "../api/client.ts";
import { attachAnchoredPopoverPosition } from "../utils/topLayer.ts";
import {
  IconCheck,
  IconCrosshair,
  IconLayers,
  IconList,
  IconMap,
  IconMinus,
  IconMoon,
  IconMountain,
  IconPlus,
  IconSatellite,
  IconX,
} from "./Icons.tsx";
import { RiskBadge } from "./RiskBadge.tsx";

export type BasemapChoice = "auto" | "standard" | "satellite" | "terrain" | "dark";
export type ResolvedBasemap = "standard" | "satellite" | "terrain" | "dark";

export interface LayerVisibilityState {
  corridors: boolean;
  hospitals: boolean;
  towns: boolean;
  districts: boolean;
}

export interface RouteOverlayData {
  coordinates: [number, number][];
  origin?: [number, number] | null;
  destination?: [number, number] | null;
  hazard?: { lat: number; lon: number; label: string } | null;
  label?: string;
}

export interface FlyToTarget {
  lon: number;
  lat: number;
  zoom?: number;
  nonce: number;
}

export interface CorridorMapProps {
  features: CorridorSegmentFeature[];
  selectedSegmentId: string | null;
  onSelectSegment: (segmentId: string) => void;
  pmtilesUrl?: string | null;
  focusBounds?: [[number, number], [number, number]] | null;
  flyToTarget?: FlyToTarget | null;
  resolvedScheme?: "light" | "dark";
  regionLabel?: string;
  routeOverlay?: RouteOverlayData | null;
  riskFilter?: string;
  onChangeRiskFilter?: (risk: string) => void;
}

const PASSABILITY_LABELS: Record<
  CorridorSegmentFeature["properties"]["passability_state"],
  string
> = {
  OPEN: "Libre",
  CAUTION_SUSPECTED: "Precaución reportada",
  PARTIAL_ONE_LANE: "Paso 1 carril",
  BLOCKED: "Bloqueado",
};

const SURFACE_LABELS: Record<
  CorridorSegmentFeature["properties"]["surface_type"],
  string
> = {
  PAVED: "Asfaltado",
  GRAVEL: "Afirmado",
  DIRT_TRACK: "Trocha",
};

const RISK_COLORS: Record<CorridorSegmentFeature["properties"]["risk_level"], string> = {
  GREEN: "#10b981",
  AMBER: "#f59e0b",
  RED: "#ef4444",
  UNKNOWN: "#94a3b8",
};

const RISK_NAMES: Record<CorridorSegmentFeature["properties"]["risk_level"], string> = {
  GREEN: "Libre",
  AMBER: "Precaución",
  RED: "Crítico",
  UNKNOWN: "Sin clima",
};

const PAGE_SIZE = 25;
let protocolRegistered = false;

function getBasemapTiles(basemap: ResolvedBasemap): {
  tiles: string[];
  attribution: string;
  maxzoom: number;
} {
  switch (basemap) {
    case "satellite":
      return {
        tiles: [
          "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
        ],
        attribution: "© Esri World Imagery",
        maxzoom: 18,
      };
    case "terrain":
      return {
        tiles: [
          "https://a.tile.opentopomap.org/{z}/{x}/{y}.png",
          "https://b.tile.opentopomap.org/{z}/{x}/{y}.png",
          "https://c.tile.opentopomap.org/{z}/{x}/{y}.png",
        ],
        attribution: "© OpenStreetMap · OpenTopoMap",
        maxzoom: 17,
      };
    case "dark":
      return {
        tiles: [
          "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}",
        ],
        attribution: "© Esri Dark Gray Canvas",
        maxzoom: 16,
      };
    case "standard":
    default:
      return {
        tiles: [
          "https://server.arcgisonline.com/ArcGIS/rest/services/World_Topo_Map/MapServer/tile/{z}/{y}/{x}",
        ],
        attribution: "© Esri · OpenStreetMap",
        maxzoom: 18,
      };
  }
}

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

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function createPopupCard(title: string, subtitle: string, dotColor?: string): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "map-popup-card";

  const headingRow = document.createElement("div");
  headingRow.className = "map-popup-title-row";

  if (dotColor) {
    const dot = document.createElement("span");
    dot.className = "map-popup-dot";
    dot.style.backgroundColor = dotColor;
    headingRow.appendChild(dot);
  }

  const strong = document.createElement("strong");
  strong.textContent = title;
  headingRow.appendChild(strong);

  const sub = document.createElement("span");
  sub.className = "map-popup-sub";
  sub.textContent = subtitle;

  wrap.appendChild(headingRow);
  wrap.appendChild(sub);
  return wrap;
}

export function CorridorMap({
  features,
  selectedSegmentId,
  onSelectSegment,
  pmtilesUrl,
  focusBounds,
  flyToTarget,
  resolvedScheme = "light",
  regionLabel = "Huánuco",
  routeOverlay,
  riskFilter: controlledRiskFilter,
  onChangeRiskFilter,
}: CorridorMapProps) {
  const mapContainerRef = useRef<HTMLDivElement | null>(null);
  const mapInstanceRef = useRef<maplibregl.Map | null>(null);
  const popupRef = useRef<maplibregl.Popup | null>(null);
  const layersBtnRef = useRef<HTMLButtonElement | null>(null);
  const layersPopoverRef = useRef<HTMLDivElement | null>(null);
  const styleBtnRef = useRef<HTMLButtonElement | null>(null);
  const stylePopoverRef = useRef<HTMLDivElement | null>(null);

  const tableCaptionId = useId();
  const searchInputId = useId();
  const riskSelectId = useId();
  const layersPopoverId = useId();
  const stylePopoverId = useId();

  const [basemapChoice, setBasemapChoice] = useState<BasemapChoice>("auto");
  const resolvedBasemap: ResolvedBasemap =
    basemapChoice === "auto"
      ? resolvedScheme === "dark"
        ? "dark"
        : "standard"
      : basemapChoice;
  const initialBasemapRef = useRef<ResolvedBasemap>(resolvedBasemap);

  const [layersVisible, setLayersVisible] = useState<LayerVisibilityState>({
    corridors: true,
    hospitals: true,
    towns: true,
    districts: true,
  });

  const [searchTerm, setSearchTerm] = useState("");
  const [internalRiskFilter, setInternalRiskFilter] = useState<string>("ALL");
  const riskFilter = controlledRiskFilter ?? internalRiskFilter;
  const setRiskFilter = (val: string) => {
    if (controlledRiskFilter === undefined) {
      setInternalRiskFilter(val);
    }
    onChangeRiskFilter?.(val);
  };
  const [pageIndex, setPageIndex] = useState(0);
  const [tableOpen, setTableOpen] = useState(false);

  const effectivePmtilesUrl = resolveClientPmtilesUrl(pmtilesUrl);

  useEffect(() => {
    const cleanLayers = attachAnchoredPopoverPosition(
      layersBtnRef.current,
      layersPopoverRef.current
    );
    const cleanStyle = attachAnchoredPopoverPosition(
      styleBtnRef.current,
      stylePopoverRef.current
    );
    return () => {
      cleanLayers();
      cleanStyle();
    };
  }, []);

  useEffect(() => {
    const container = mapContainerRef.current;
    if (!container) return;

    if (!protocolRegistered) {
      const protocol = new Protocol();
      maplibregl.addProtocol("pmtiles", protocol.tile);
      protocolRegistered = true;
    }

    const startBasemap = initialBasemapRef.current;
    const basemapCfg = getBasemapTiles(startBasemap);
    const bgColor = startBasemap === "dark" ? "#0b1120" : "#e8eef5";

    const sources: Record<string, maplibregl.SourceSpecification> = {
      "basemap-raster": {
        type: "raster",
        tiles: basemapCfg.tiles,
        tileSize: 256,
        maxzoom: basemapCfg.maxzoom,
        attribution: basemapCfg.attribution,
      },
    };
    if (effectivePmtilesUrl) {
      sources.territorial = {
        type: "vector",
        url: toPmtilesProtocolSourceUrl(effectivePmtilesUrl),
      };
    }

    const baseStyle: maplibregl.StyleSpecification = {
      version: 8,
      sources,
      layers: [
        {
          id: "background",
          type: "background",
          paint: { "background-color": bgColor },
        },
        {
          id: "basemap-tiles",
          type: "raster",
          source: "basemap-raster",
          paint: { "raster-opacity": 0.96 },
        },
      ],
    };

    const map = new maplibregl.Map({
      container,
      style: baseStyle,
      center: [-76.12, -9.68],
      zoom: 8.4,
      attributionControl: { compact: true },
    });

    if (typeof maplibregl.Popup === "function") {
      popupRef.current = new maplibregl.Popup({
        closeButton: false,
        closeOnClick: false,
        offset: 12,
        maxWidth: "260px",
      });
    }

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
            "line-width": ["interpolate", ["linear"], ["zoom"], 6, 1.4, 10, 3.2],
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
            "circle-stroke-width": 1.5,
            "circle-stroke-color": "#ffffff",
            "circle-opacity": 0.85,
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
            "circle-color": "#0d9488",
            "circle-stroke-width": 1.25,
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
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 6, 4, 10, 7],
            "circle-color": "#e11d48",
            "circle-stroke-width": 2,
            "circle-stroke-color": "#ffffff",
          },
        });
      }

      map.addSource("corridors-geojson", {
        type: "geojson",
        data: { type: "FeatureCollection", features: [] },
      });

      map.addLayer({
        id: "corridors-casing",
        type: "line",
        source: "corridors-geojson",
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": ["case", ["==", ["get", "passability_state"], "BLOCKED"], 8, 6.2],
          "line-color": "#ffffff",
          "line-opacity": 0.55,
        },
      });

      map.addLayer({
        id: "corridors-line",
        type: "line",
        source: "corridors-geojson",
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": ["case", ["==", ["get", "passability_state"], "BLOCKED"], 5.2, 3.8],
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
        id: "corridors-selected-halo",
        type: "line",
        source: "corridors-geojson",
        filter: ["==", ["get", "segment_id"], ""],
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": 14,
          "line-color": "#3b82f6",
          "line-opacity": 0.36,
        },
      });

      map.addLayer({
        id: "corridors-selected-line",
        type: "line",
        source: "corridors-geojson",
        filter: ["==", ["get", "segment_id"], ""],
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": 6,
          "line-color": "#2563eb",
          "line-opacity": 1,
        },
      });

      // Active route + simulation overlay sources and layers
      map.addSource("active-route-source", {
        type: "geojson",
        data: { type: "FeatureCollection", features: [] },
      });

      map.addLayer({
        id: "active-route-casing",
        type: "line",
        source: "active-route-source",
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": 10,
          "line-color": "#ffffff",
          "line-opacity": 0.85,
        },
      });

      map.addLayer({
        id: "active-route-line",
        type: "line",
        source: "active-route-source",
        layout: { "line-cap": "round", "line-join": "round" },
        paint: {
          "line-width": 5.5,
          "line-color": "#2563eb",
        },
      });

      map.addSource("active-route-points", {
        type: "geojson",
        data: { type: "FeatureCollection", features: [] },
      });

      map.addLayer({
        id: "active-route-pins",
        type: "circle",
        source: "active-route-points",
        paint: {
          "circle-radius": 8,
          "circle-color": ["match", ["get", "role"], "origin", "#2563eb", "destination", "#10b981", "#ef4444"],
          "circle-stroke-width": 3,
          "circle-stroke-color": "#ffffff",
        },
      });

      map.addSource("active-hazard-source", {
        type: "geojson",
        data: { type: "FeatureCollection", features: [] },
      });

      map.addLayer({
        id: "active-hazard-pulse",
        type: "circle",
        source: "active-hazard-source",
        paint: {
          "circle-radius": 22,
          "circle-color": "#ef4444",
          "circle-opacity": 0.22,
          "circle-stroke-width": 2,
          "circle-stroke-color": "#ef4444",
        },
      });

      const handleClickCorridor = (event: maplibregl.MapLayerMouseEvent) => {
        const clickedId = event.features?.[0]?.properties?.segment_id;
        if (typeof clickedId === "string" && clickedId.length > 0) {
          onSelectSegment(clickedId);
        }
      };

      const setPointer = (cursor: string) => {
        const canvas = map.getCanvas?.();
        if (canvas) canvas.style.cursor = cursor;
      };

      const handleCorridorHover = (event: maplibregl.MapLayerMouseEvent) => {
        const props = event.features?.[0]?.properties;
        if (!props) return;
        setPointer("pointer");
        if (!popupRef.current) return;
        const name = String(props.corridor_name ?? "Vía territorial");
        const lenKm = Number(props.length_km);
        const surf = SURFACE_LABELS[props.surface_type as keyof typeof SURFACE_LABELS] ?? "Vía";
        const effRisk = (props.effective_risk ?? props.risk_level ?? "UNKNOWN") as keyof typeof RISK_COLORS;
        const subtitle = `${Number.isFinite(lenKm) ? `${lenKm.toFixed(1)} km · ` : ""}${surf} · ${RISK_NAMES[effRisk] ?? "Sin clima"}`;
        popupRef.current
          .setLngLat(event.lngLat)
          .setDOMContent(createPopupCard(name, subtitle, RISK_COLORS[effRisk] ?? RISK_COLORS.UNKNOWN))
          .addTo(map);
      };

      const handleCorridorLeave = () => {
        setPointer("");
        popupRef.current?.remove();
      };

      const handleNodeHover = (kindLabel: string, color: string) => (event: maplibregl.MapLayerMouseEvent) => {
        const props = event.features?.[0]?.properties;
        if (!props) return;
        setPointer("pointer");
        if (!popupRef.current) return;
        const name = String(props.name ?? kindLabel);
        const elev = props.elevation_m ? ` · ${props.elevation_m} m s.n.m.` : "";
        popupRef.current
          .setLngLat(event.lngLat)
          .setDOMContent(createPopupCard(name, `${kindLabel}${elev}`, color))
          .addTo(map);
      };

      map.on("click", "corridors-line", handleClickCorridor);
      map.on("mousemove", "corridors-line", handleCorridorHover);
      map.on("mouseleave", "corridors-line", handleCorridorLeave);

      if (effectivePmtilesUrl) {
        map.on("click", "pmtiles-corridors", handleClickCorridor);
        map.on("mousemove", "pmtiles-corridors", handleCorridorHover);
        map.on("mouseleave", "pmtiles-corridors", handleCorridorLeave);

        map.on("mousemove", "pmtiles-nodes-hospitals", handleNodeHover("Hospital / Centro de Salud", "#e11d48"));
        map.on("mouseleave", "pmtiles-nodes-hospitals", handleCorridorLeave);
        map.on("mousemove", "pmtiles-nodes-towns", handleNodeHover("Centro poblado", "#0d9488"));
        map.on("mouseleave", "pmtiles-nodes-towns", handleCorridorLeave);
        map.on("mousemove", "pmtiles-districts-points", handleNodeHover("Distrito", "#0284c7"));
        map.on("mouseleave", "pmtiles-districts-points", handleCorridorLeave);
      }
    });

    mapInstanceRef.current = map;
    return () => {
      popupRef.current?.remove();
      popupRef.current = null;
      map.remove();
      mapInstanceRef.current = null;
    };
  }, [onSelectSegment, effectivePmtilesUrl]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map) return;
    const cfg = getBasemapTiles(resolvedBasemap);
    if (typeof map.getLayer === "function" && map.getLayer("background")) {
      map.setPaintProperty?.(
        "background",
        "background-color",
        resolvedBasemap === "dark" ? "#0b1120" : "#e8eef5"
      );
    }
    const basemapSource = map.getSource("basemap-raster") as
      | (maplibregl.RasterTileSource & { setTiles?: (tiles: string[]) => void })
      | undefined;
    if (basemapSource && typeof basemapSource.setTiles === "function") {
      basemapSource.setTiles(cfg.tiles);
    }
  }, [resolvedBasemap]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map || typeof map.getLayer !== "function" || typeof map.setLayoutProperty !== "function") {
      return;
    }
    const toggle = (id: string, visible: boolean) => {
      if (map.getLayer(id)) {
        map.setLayoutProperty(id, "visibility", visible ? "visible" : "none");
      }
    };
    toggle("corridors-line", layersVisible.corridors);
    toggle("corridors-casing", layersVisible.corridors);
    toggle("corridors-selected-halo", layersVisible.corridors);
    toggle("corridors-selected-line", layersVisible.corridors);
    toggle("pmtiles-corridors", layersVisible.corridors);
    toggle("pmtiles-nodes-hospitals", layersVisible.hospitals);
    toggle("pmtiles-nodes-towns", layersVisible.towns);
    toggle("pmtiles-districts-points", layersVisible.districts);
  }, [layersVisible]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map || !focusBounds) return;
    map.fitBounds?.(focusBounds, {
      padding: { top: 88, right: 64, bottom: 72, left: 410 },
      maxZoom: 12,
      duration: prefersReducedMotion() ? 0 : 520,
    });
  }, [focusBounds]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map || !flyToTarget) return;
    map.flyTo?.({
      center: [flyToTarget.lon, flyToTarget.lat],
      zoom: flyToTarget.zoom ?? 12.2,
      duration: prefersReducedMotion() ? 0 : 600,
    });
  }, [flyToTarget]);

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

    source.setData({ type: "FeatureCollection", features: enriched });
  }, [features]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map) return;
    const filterExpr: maplibregl.FilterSpecification = [
      "==",
      ["get", "segment_id"],
      selectedSegmentId ?? "",
    ];
    if (typeof map.getLayer === "function" && map.getLayer("corridors-selected-line")) {
      map.setFilter?.("corridors-selected-line", filterExpr);
    }
    if (typeof map.getLayer === "function" && map.getLayer("corridors-selected-halo")) {
      map.setFilter?.("corridors-selected-halo", filterExpr);
    }
  }, [selectedSegmentId]);

  useEffect(() => {
    const map = mapInstanceRef.current;
    if (!map) return;
    const routeSrc = map.getSource("active-route-source") as maplibregl.GeoJSONSource | undefined;
    const ptsSrc = map.getSource("active-route-points") as maplibregl.GeoJSONSource | undefined;
    const hazardSrc = map.getSource("active-hazard-source") as maplibregl.GeoJSONSource | undefined;
    if (!routeSrc || !ptsSrc || !hazardSrc) return;

    if (!routeOverlay) {
      routeSrc.setData({ type: "FeatureCollection", features: [] });
      ptsSrc.setData({ type: "FeatureCollection", features: [] });
      hazardSrc.setData({ type: "FeatureCollection", features: [] });
      return;
    }

    routeSrc.setData({
      type: "FeatureCollection",
      features:
        routeOverlay.coordinates.length >= 2
          ? [
              {
                type: "Feature",
                geometry: { type: "LineString", coordinates: routeOverlay.coordinates },
                properties: { label: routeOverlay.label ?? "Ruta" },
              },
            ]
          : [],
    });

    const pins: GeoJSON.Feature[] = [];
    if (routeOverlay.origin) {
      pins.push({
        type: "Feature",
        geometry: { type: "Point", coordinates: routeOverlay.origin },
        properties: { role: "origin" },
      });
    }
    if (routeOverlay.destination) {
      pins.push({
        type: "Feature",
        geometry: { type: "Point", coordinates: routeOverlay.destination },
        properties: { role: "destination" },
      });
    }
    ptsSrc.setData({ type: "FeatureCollection", features: pins });

    hazardSrc.setData({
      type: "FeatureCollection",
      features: routeOverlay.hazard
        ? [
            {
              type: "Feature",
              geometry: {
                type: "Point",
                coordinates: [routeOverlay.hazard.lon, routeOverlay.hazard.lat],
              },
              properties: { label: routeOverlay.hazard.label },
            },
          ]
        : [],
    });

    const ptsToFit: [number, number][] = [...routeOverlay.coordinates];
    if (routeOverlay.origin) ptsToFit.push(routeOverlay.origin);
    if (routeOverlay.destination) ptsToFit.push(routeOverlay.destination);
    if (routeOverlay.hazard) ptsToFit.push([routeOverlay.hazard.lon, routeOverlay.hazard.lat]);
    if (ptsToFit.length > 0) {
      let minLon = Infinity;
      let minLat = Infinity;
      let maxLon = -Infinity;
      let maxLat = -Infinity;
      for (const [lon, lat] of ptsToFit) {
        if (lon < minLon) minLon = lon;
        if (lat < minLat) minLat = lat;
        if (lon > maxLon) maxLon = lon;
        if (lat > maxLat) maxLat = lat;
      }
      map.fitBounds?.(
        [
          [minLon - 0.03, minLat - 0.03],
          [maxLon + 0.03, maxLat + 0.03],
        ],
        {
          padding: { top: 96, right: 80, bottom: 96, left: 410 },
          maxZoom: 12.5,
          duration: prefersReducedMotion() ? 0 : 600,
        }
      );
    }
  }, [routeOverlay]);

  const handleInspectSegment = (feature: CorridorSegmentFeature) => {
    onSelectSegment(feature.properties.segment_id);
    const map = mapInstanceRef.current;
    const coords = feature.geometry?.coordinates;
    if (map && Array.isArray(coords) && coords.length > 0) {
      let minLon = Infinity;
      let minLat = Infinity;
      let maxLon = -Infinity;
      let maxLat = -Infinity;
      for (const [lon, lat] of coords) {
        if (lon < minLon) minLon = lon;
        if (lat < minLat) minLat = lat;
        if (lon > maxLon) maxLon = lon;
        if (lat > maxLat) maxLat = lat;
      }
      if (Number.isFinite(minLon) && Number.isFinite(maxLon)) {
        map.fitBounds?.(
          [
            [minLon - 0.015, minLat - 0.015],
            [maxLon + 0.015, maxLat + 0.015],
          ],
          {
            padding: { top: 96, right: 80, bottom: 96, left: 410 },
            maxZoom: 13,
            duration: prefersReducedMotion() ? 0 : 480,
          }
        );
      }
    }
  };

  const handleRecenter = () => {
    const map = mapInstanceRef.current;
    if (!map) return;
    if (focusBounds) {
      map.fitBounds?.(focusBounds, {
        padding: { top: 88, right: 64, bottom: 72, left: 410 },
        maxZoom: 12,
        duration: prefersReducedMotion() ? 0 : 450,
      });
    } else {
      map.flyTo?.({
        center: [-76.12, -9.68],
        zoom: 8.4,
        duration: prefersReducedMotion() ? 0 : 450,
      });
    }
  };

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
    <section
      className="map-stage"
      aria-label={`Mapa territorial interactivo — ${regionLabel}`}
    >
      <div ref={mapContainerRef} className="map-canvas-host" />

      {/* Bottom-center compact legend + Table sheet trigger */}
      <div className="map-dock-bottom-center">
        <div className="map-legend-pill" aria-label="Leyenda del mapa">
          <span className="legend-dot legend-dot--green">Libre</span>
          <span className="legend-dot legend-dot--amber">Precaución</span>
          <span className="legend-dot legend-dot--red">Crítico</span>
          <span className="legend-dot legend-dot--hospital">Hospital</span>
        </div>

        <button
          type="button"
          className="map-dock-pill"
          aria-expanded={tableOpen}
          onClick={() => setTableOpen((v) => !v)}
        >
          <IconList size={16} />
          <span>Vías ({filteredFeatures.length})</span>
        </button>
      </div>

      {/* Bottom-right Map Controls: Zoom Stack + Capas + Estilo Mapa */}
      <div className="map-dock-bottom-right" aria-label="Controles del mapa">
        <div className="map-action-row">
          <button
            ref={layersBtnRef}
            type="button"
            className="map-dock-pill"
            {...({ popoverTarget: layersPopoverId } as Record<string, string>)}
          >
            <IconLayers size={16} />
            <span>Capas</span>
          </button>

          <button
            ref={styleBtnRef}
            type="button"
            className="map-dock-pill"
            {...({ popoverTarget: stylePopoverId } as Record<string, string>)}
          >
            <IconMap size={16} />
            <span>Mapa</span>
          </button>
        </div>

        <div className="map-zoom-stack" role="group" aria-label="Zoom y enfoque del mapa">
          <button
            type="button"
            className="zoom-btn"
            aria-label="Acercar mapa"
            onClick={() =>
              mapInstanceRef.current?.zoomIn?.({ duration: prefersReducedMotion() ? 0 : 220 })
            }
          >
            <IconPlus size={17} />
          </button>
          <button
            type="button"
            className="zoom-btn"
            aria-label="Alejar mapa"
            onClick={() =>
              mapInstanceRef.current?.zoomOut?.({ duration: prefersReducedMotion() ? 0 : 220 })
            }
          >
            <IconMinus size={17} />
          </button>
          <button
            type="button"
            className="zoom-btn"
            aria-label="Centrar en territorio activo"
            onClick={handleRecenter}
          >
            <IconCrosshair size={16} />
          </button>
        </div>
      </div>

      {/* Layers Popover (Native Popover API + Fallback Positioning) */}
      <div
        ref={layersPopoverRef}
        id={layersPopoverId}
        className="map-popover anchored-popover"
        {...({ popover: "auto" } as Record<string, string>)}
      >
        <p className="popover-title">Capas visibles</p>
        <div className="popover-option-list">
          {(
            [
              ["corridors", "Red vial IVV"],
              ["hospitals", "Hospitales"],
              ["towns", "Centros poblados"],
              ["districts", "Distritos"],
            ] as const
          ).map(([key, label]) => {
            const active = layersVisible[key];
            return (
              <button
                key={key}
                type="button"
                className={`popover-choice-btn${active ? " is-active" : ""}`}
                aria-pressed={active}
                onClick={() =>
                  setLayersVisible((prev) => ({ ...prev, [key]: !prev[key] }))
                }
              >
                <span>{label}</span>
                {active && <IconCheck size={15} />}
              </button>
            );
          })}
        </div>
      </div>

      {/* Basemap Style Popover */}
      <div
        ref={stylePopoverRef}
        id={stylePopoverId}
        className="map-popover anchored-popover"
        {...({ popover: "auto" } as Record<string, string>)}
      >
        <p className="popover-title">Tipo de mapa</p>
        <div className="style-grid">
          <button
            type="button"
            className={`style-card${resolvedBasemap === "standard" ? " is-active" : ""}`}
            aria-pressed={resolvedBasemap === "standard"}
            onClick={() => setBasemapChoice("standard")}
          >
            <IconMap size={18} />
            <span>Estándar</span>
          </button>
          <button
            type="button"
            className={`style-card${resolvedBasemap === "satellite" ? " is-active" : ""}`}
            aria-pressed={resolvedBasemap === "satellite"}
            onClick={() => setBasemapChoice("satellite")}
          >
            <IconSatellite size={18} />
            <span>Satélite</span>
          </button>
          <button
            type="button"
            className={`style-card${resolvedBasemap === "terrain" ? " is-active" : ""}`}
            aria-pressed={resolvedBasemap === "terrain"}
            onClick={() => setBasemapChoice("terrain")}
          >
            <IconMountain size={18} />
            <span>Relieve</span>
          </button>
          <button
            type="button"
            className={`style-card${resolvedBasemap === "dark" ? " is-active" : ""}`}
            aria-pressed={resolvedBasemap === "dark"}
            onClick={() => setBasemapChoice("dark")}
          >
            <IconMoon size={18} />
            <span>Nocturno</span>
          </button>
        </div>
      </div>

      {/* Collapsible Bottom Table Sheet (hidden by default for clean map-first paint) */}
      {tableOpen && (
        <div
          className="table-sheet"
          role="region"
          aria-label="Listado detallado de tramos viales"
        >
          <div className="table-sheet-header">
            <div className="table-filter-bar">
              <div className="field field--compact" style={{ flex: "1 1 200px" }}>
                <label htmlFor={searchInputId} className="visually-hidden">
                  Buscar corredor vial
                </label>
                <input
                  id={searchInputId}
                  type="search"
                  placeholder="Filtrar vías (Carpish, Central, Ambo...)"
                  value={searchTerm}
                  onChange={(e) => {
                    setSearchTerm(e.target.value);
                    setPageIndex(0);
                  }}
                />
              </div>

              <div className="field field--compact" style={{ flex: "0 1 175px" }}>
                <label htmlFor={riskSelectId} className="visually-hidden">
                  Filtrar estado de riesgo
                </label>
                <select
                  id={riskSelectId}
                  className="brand-select"
                  value={riskFilter}
                  onChange={(e) => {
                    setRiskFilter(e.target.value);
                    setPageIndex(0);
                  }}
                >
                  <option value="ALL">Todos ({features.length})</option>
                  <option value="GREEN">Libre</option>
                  <option value="AMBER">Precaución</option>
                  <option value="RED">Crítico</option>
                  <option value="UNKNOWN">Sin clima vigente</option>
                </select>
              </div>
            </div>

            <button
              type="button"
              className="icon-btn"
              aria-label="Cerrar tabla de vías"
              onClick={() => setTableOpen(false)}
            >
              <IconX size={18} />
            </button>
          </div>

          <div
            tabIndex={0}
            role="region"
            aria-labelledby={tableCaptionId}
            className="table-wrapper"
          >
            <table className="corridor-table">
              <caption id={tableCaptionId}>
                Tramos viales — {regionLabel} ({filteredFeatures.length})
              </caption>
              <thead>
                <tr>
                  <th scope="col">Corredor</th>
                  <th scope="col">Distancia</th>
                  <th scope="col">Estado</th>
                  <th scope="col">Riesgo</th>
                  <th scope="col">Mapa</th>
                </tr>
              </thead>
              <tbody>
                {pageFeatures.length === 0 ? (
                  <tr>
                    <td colSpan={5}>
                      {features.length === 0
                        ? "Sin tramos cargados todavía."
                        : "Sin resultados para este filtro."}
                    </td>
                  </tr>
                ) : (
                  pageFeatures.map((f) => {
                    const p = f.properties;
                    const isSelected = selectedSegmentId === p.segment_id;
                    return (
                      <tr key={p.segment_id} className={isSelected ? "is-selected" : undefined}>
                        <th scope="row">
                          <strong>{p.corridor_name}</strong>
                          <div className="hint" style={{ marginBlockEnd: 0 }}>
                            {SURFACE_LABELS[p.surface_type]}
                          </div>
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
                            className={isSelected ? "btn btn--primary btn--sm" : "btn btn--sm"}
                            onClick={() => {
                              handleInspectSegment(f);
                              setTableOpen(false);
                            }}
                            aria-pressed={isSelected}
                          >
                            {isSelected ? "En vista" : "Ver"}
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
                className="btn btn--sm"
                disabled={safePageIndex === 0}
                onClick={() => setPageIndex((p) => Math.max(0, p - 1))}
              >
                Anterior
              </button>
              <span className="hint" style={{ marginBlockEnd: 0 }}>
                {safePageIndex + 1} / {totalPages}
              </span>
              <button
                type="button"
                className="btn btn--sm"
                disabled={safePageIndex >= totalPages - 1}
                onClick={() => setPageIndex((p) => Math.min(totalPages - 1, p + 1))}
              >
                Siguiente
              </button>
            </nav>
          )}
        </div>
      )}
    </section>
  );
}
