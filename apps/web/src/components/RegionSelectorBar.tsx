import { useId } from "react";
import type { CorridorSegmentFeature, TerritorialUnit } from "../api/client.ts";

export interface RegionSelectorBarProps {
  units: TerritorialUnit[];
  selectedRegionCode: string;
  selectedProvinceUnitId: string;
  selectedDistrictUnitId: string;
  onSelectRegion: (isoOrUbigeo: string) => void;
  onSelectProvince: (unitId: string) => void;
  onSelectDistrict: (unitId: string) => void;
  features: CorridorSegmentFeature[];
  loading: boolean;
  pmtilesActiveUrl: string | null;
}

export function RegionSelectorBar({
  units,
  selectedRegionCode,
  selectedProvinceUnitId,
  selectedDistrictUnitId,
  onSelectRegion,
  onSelectProvince,
  onSelectDistrict,
  features,
  loading,
  pmtilesActiveUrl,
}: RegionSelectorBarProps) {
  const headingId = useId();
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
      return u.parent_unit_id === activeRegion.unit_id || (u.parent_unit_id ? provinceIds.has(u.parent_unit_id) : false);
    }
    return true;
  });

  let greenCount = 0;
  let amberCount = 0;
  let redCount = 0;
  let unknownCount = 0;
  for (const f of features) {
    if (f.properties.is_stale || f.properties.risk_level === "UNKNOWN") {
      unknownCount += 1;
    } else if (f.properties.risk_level === "GREEN") {
      greenCount += 1;
    } else if (f.properties.risk_level === "AMBER") {
      amberCount += 1;
    } else if (f.properties.risk_level === "RED") {
      redCount += 1;
    }
  }

  return (
    <section className="card region-toolbar-wrap" aria-labelledby={headingId}>
      <div className="region-toolbar-header">
        <div>
          <h2 id={headingId} style={{ marginBlockEnd: "0.25rem" }}>
            Selector Territorial y Cobertura PMTiles
          </h2>
          <p className="hint" style={{ marginBlockEnd: 0 }}>
            {activeRegion
              ? `${activeRegion.name} · ${provinces.length} provincias · ${districts.length} distritos/centros`
              : "Cobertura territorial regional"}
            {pmtilesActiveUrl ? ` · Vectorial: ${pmtilesActiveUrl}` : ""}
          </p>
        </div>
        {loading && (
          <span className="risk-badge risk-badge--UNKNOWN" role="status">
            Sincronizando tramos...
          </span>
        )}
      </div>

      <div className="region-controls-grid">
        <div className="field" style={{ marginBlockEnd: 0 }}>
          <label htmlFor={regionSelectId}>Departamento / Región</label>
          <select
            id={regionSelectId}
            value={selectedRegionCode}
            onChange={(e) => onSelectRegion(e.target.value)}
          >
            {regions.length === 0 ? (
              <option value="PE-HUC">Departamento de Huánuco (PE-HUC)</option>
            ) : (
              regions.map((r) => {
                const code = r.iso_code ?? r.ubigeo_code;
                return (
                  <option key={r.unit_id} value={code}>
                    {r.name} ({code}) — {r.segment_count} tramos
                  </option>
                );
              })
            )}
          </select>
        </div>

        <div className="field" style={{ marginBlockEnd: 0 }}>
          <label htmlFor={provinceSelectId}>Provincia ({provinces.length})</label>
          <select
            id={provinceSelectId}
            value={selectedProvinceUnitId}
            onChange={(e) => onSelectProvince(e.target.value)}
            disabled={provinces.length === 0}
          >
            <option value="">Todas las provincias</option>
            {provinces.map((p) => (
              <option key={p.unit_id} value={p.unit_id}>
                {p.name} ({p.ubigeo_code}) — {p.segment_count}
              </option>
            ))}
          </select>
        </div>

        <div className="field" style={{ marginBlockEnd: 0 }}>
          <label htmlFor={districtSelectId}>Distrito / Centro ({districts.length})</label>
          <select
            id={districtSelectId}
            value={selectedDistrictUnitId}
            onChange={(e) => onSelectDistrict(e.target.value)}
            disabled={districts.length === 0}
          >
            <option value="">Todos los distritos</option>
            {districts.map((d) => (
              <option key={d.unit_id} value={d.unit_id}>
                {d.name} ({d.segment_count})
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="kpi-strip" aria-label="Resumen de riesgo vial en el ámbito seleccionado">
        <span className="kpi-chip">
          Tramos en vista: <strong>{features.length}</strong>
        </span>
        <span className="risk-badge risk-badge--GREEN">VERDE: {greenCount}</span>
        <span className="risk-badge risk-badge--AMBER">ÁMBAR: {amberCount}</span>
        <span className="risk-badge risk-badge--RED">ROJO: {redCount}</span>
        <span className="risk-badge risk-badge--UNKNOWN">DESCONOCIDO / VENCIDO: {unknownCount}</span>
      </div>
    </section>
  );
}
