import { useId, useMemo } from "react";
import type { CorridorSegmentFeature, TerritorialUnit } from "../api/client.ts";
import { IconPin } from "./Icons.tsx";
import { RiskBadge } from "./RiskBadge.tsx";

export interface RegionSelectorBarProps {
  units: TerritorialUnit[];
  selectedRegionCode: string;
  selectedProvinceUnitId: string;
  selectedDistrictUnitId: string;
  onSelectRegion: (isoOrUbigeo: string) => void;
  onSelectProvince: (unitId: string) => void;
  onSelectDistrict: (unitId: string) => void;
  features: CorridorSegmentFeature[];
  selectedSegmentId: string | null;
  onSelectFeature: (feature: CorridorSegmentFeature) => void;
  loading: boolean;
}

const SURFACE_SHORT: Record<
  CorridorSegmentFeature["properties"]["surface_type"],
  string
> = {
  PAVED: "Asfaltado",
  GRAVEL: "Afirmado",
  DIRT_TRACK: "Trocha",
};

export function RegionSelectorBar({
  units,
  selectedRegionCode,
  selectedProvinceUnitId,
  selectedDistrictUnitId,
  onSelectRegion,
  onSelectProvince,
  onSelectDistrict,
  features,
  selectedSegmentId,
  onSelectFeature,
  loading,
}: RegionSelectorBarProps) {
  const regionSelectId = useId();
  const provinceSelectId = useId();
  const districtSelectId = useId();

  const regions = units.filter((u) => u.admin_level === 4);
  const activeRegion =
    regions.find((u) => (u.iso_code ?? u.ubigeo_code) === selectedRegionCode) ?? regions[0] ?? null;

  const provinces = units.filter(
    (u) => u.admin_level === 6 && (!activeRegion || u.parent_unit_id === activeRegion.unit_id)
  );
  const provinceIds = new Set(provinces.map((p) => p.unit_id));

  const districts = units.filter((u) => {
    if (u.admin_level !== 8) return false;
    if (selectedProvinceUnitId) {
      return u.parent_unit_id === selectedProvinceUnitId;
    }
    if (activeRegion) {
      return (
        u.parent_unit_id === activeRegion.unit_id ||
        (u.parent_unit_id ? provinceIds.has(u.parent_unit_id) : false)
      );
    }
    return true;
  });

  const stats = useMemo(() => {
    let green = 0;
    let amber = 0;
    let red = 0;
    let unknown = 0;
    for (const f of features) {
      if (f.properties.is_stale || f.properties.risk_level === "UNKNOWN") {
        unknown += 1;
      } else if (f.properties.risk_level === "GREEN") {
        green += 1;
      } else if (f.properties.risk_level === "AMBER") {
        amber += 1;
      } else if (f.properties.risk_level === "RED") {
        red += 1;
      }
    }
    return { green, amber, red, unknown };
  }, [features]);

  const highlightedCorridors = useMemo(() => {
    const sorted = [...features].sort((a, b) => {
      const aNamed =
        !a.properties.corridor_name.startsWith("Acceso ") &&
        !a.properties.corridor_name.startsWith("Red Vial ");
      const bNamed =
        !b.properties.corridor_name.startsWith("Acceso ") &&
        !b.properties.corridor_name.startsWith("Red Vial ");
      if (aNamed !== bNamed) return aNamed ? -1 : 1;
      const aPaved = a.properties.surface_type === "PAVED" ? 0 : 1;
      const bPaved = b.properties.surface_type === "PAVED" ? 0 : 1;
      if (aPaved !== bPaved) return aPaved - bPaved;
      return b.properties.length_km - a.properties.length_km;
    });

    const seenNames = new Set<string>();
    const unique: CorridorSegmentFeature[] = [];
    for (const f of sorted) {
      const key = f.properties.corridor_name.trim().toLowerCase();
      if (!seenNames.has(key)) {
        seenNames.add(key);
        unique.push(f);
      }
      if (unique.length >= 8) break;
    }
    return unique;
  }, [features]);

  return (
    <div className="panel-stack">
      <div className="panel-heading-row">
        <div>
          <h2 className="panel-title">{activeRegion?.name ?? "Red Vial Regional"}</h2>
          <p className="panel-subtitle">
            {provinces.length} provincias · {districts.length} distritos
          </p>
        </div>
        {loading && <span className="pill-spinner">Cargando...</span>}
      </div>

      <div className="territory-fields">
        <div className="field field--tight">
          <label htmlFor={regionSelectId}>Departamento / Región</label>
          <select
            id={regionSelectId}
            className="brand-select"
            value={selectedRegionCode}
            onChange={(e) => onSelectRegion(e.target.value)}
          >
            {regions.length === 0 ? (
              <option value="PE-HUC">Departamento de Huánuco</option>
            ) : (
              regions.map((r) => {
                const code = r.iso_code ?? r.ubigeo_code;
                return (
                  <option key={r.unit_id} value={code}>
                    {r.name}
                  </option>
                );
              })
            )}
          </select>
        </div>

        <div className="two-col-inline">
          <div className="field field--tight">
            <label htmlFor={provinceSelectId}>Provincia ({provinces.length})</label>
            <select
              id={provinceSelectId}
              className="brand-select"
              value={selectedProvinceUnitId}
              onChange={(e) => onSelectProvince(e.target.value)}
              disabled={provinces.length === 0}
            >
              <option value="">Todas</option>
              {provinces.map((p) => (
                <option key={p.unit_id} value={p.unit_id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>

          <div className="field field--tight">
            <label htmlFor={districtSelectId}>Distrito ({districts.length})</label>
            <select
              id={districtSelectId}
              className="brand-select"
              value={selectedDistrictUnitId}
              onChange={(e) => onSelectDistrict(e.target.value)}
              disabled={districts.length === 0}
            >
              <option value="">Todos</option>
              {districts.map((d) => (
                <option key={d.unit_id} value={d.unit_id}>
                  {d.name}
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>

      <div className="stat-pill-grid" aria-label="Resumen de estado vial">
        <div className="stat-mini stat-mini--green">
          <strong>{stats.green}</strong>
          <span>Libres</span>
        </div>
        <div className="stat-mini stat-mini--amber">
          <strong>{stats.amber}</strong>
          <span>Alerta</span>
        </div>
        <div className="stat-mini stat-mini--red">
          <strong>{stats.red}</strong>
          <span>Críticos</span>
        </div>
        <div className="stat-mini stat-mini--muted">
          <strong>{stats.unknown}</strong>
          <span>Sin clima</span>
        </div>
      </div>

      {highlightedCorridors.length > 0 && (
        <div className="corridor-quick-section">
          <span className="section-eyebrow">Vías principales</span>
          <ul className="quick-corridor-list">
            {highlightedCorridors.map((f) => {
              const p = f.properties;
              const isSelected = p.segment_id === selectedSegmentId;
              return (
                <li key={p.segment_id}>
                  <button
                    type="button"
                    className={`corridor-quick-btn${isSelected ? " is-selected" : ""}`}
                    onClick={() => onSelectFeature(f)}
                  >
                    <span className="corridor-quick-main">
                      <IconPin size={15} />
                      <span>
                        <strong>{p.corridor_name}</strong>
                        <small>
                          {p.length_km.toFixed(1)} km · {SURFACE_SHORT[p.surface_type]}
                        </small>
                      </span>
                    </span>
                    <RiskBadge
                      riskLevel={p.risk_level}
                      ivvScore={p.ivv_score}
                      isStale={p.is_stale}
                    />
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
