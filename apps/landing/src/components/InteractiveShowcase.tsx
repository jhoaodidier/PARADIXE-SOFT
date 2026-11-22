import { useMemo, useState } from "react";
import { computeDeterministicIvv, PROFILE_TABS } from "../data/productSpecs";

const RISK_LABEL: Record<"GREEN" | "AMBER" | "RED", string> = {
  GREEN: "VERDE · Tránsito libre",
  AMBER: "ÁMBAR · Precaución activa",
  RED: "ROJO · Alto riesgo geotécnico",
};

export function InteractiveShowcase() {
  // Preset calibrado con el caso real del túnel/corte Carpish (IVV = 84.00 RED)
  const [precip24hMm, setPrecip24hMm] = useState(85);
  const [precip72hMm, setPrecip72hMm] = useState(190);
  const [slopePct, setSlopePct] = useState(27);
  const [ravineDistM, setRavineDistM] = useState(200);
  const [soilIdx, setSoilIdx] = useState(0.8);
  const [isHotspot, setIsHotspot] = useState(true);

  const [activeProfileId, setActiveProfileId] = useState<
    (typeof PROFILE_TABS)[number]["id"]
  >("fleets");

  const ivv = useMemo(
    () =>
      computeDeterministicIvv({
        precip24hMm,
        precip72hMm,
        slopePct,
        ravineDistM,
        soilIdx,
        isHotspot,
      }),
    [precip24hMm, precip72hMm, slopePct, ravineDistM, soilIdx, isHotspot],
  );

  const activeProfile =
    PROFILE_TABS.find((p) => p.id === activeProfileId) ?? PROFILE_TABS[0];

  function applyPreset(preset: "nominal" | "carpish" | "extreme") {
    if (preset === "nominal") {
      setPrecip24hMm(6);
      setPrecip72hMm(18);
      setSlopePct(8);
      setRavineDistM(420);
      setSoilIdx(0.25);
      setIsHotspot(false);
    } else if (preset === "carpish") {
      setPrecip24hMm(85);
      setPrecip72hMm(190);
      setSlopePct(27);
      setRavineDistM(200);
      setSoilIdx(0.8);
      setIsHotspot(true);
    } else {
      setPrecip24hMm(80);
      setPrecip72hMm(180);
      setSlopePct(45);
      setRavineDistM(0);
      setSoilIdx(1.0);
      setIsHotspot(true);
    }
  }

  return (
    <section
      id="simulador-ivv"
      className="interactive-section"
      aria-labelledby="ivv-sim-heading"
    >
      <div className="section-shell">
        {/* Bloque 1: Calculadora Interactiva Exacta de la Fórmula IVV */}
        <header className="section-intro" data-reveal>
          <p className="eyebrow">Laboratorio Interactivo · crates/paradixe-ivv</p>
          <h2 id="ivv-sim-heading" className="display-title">
            Prueba el motor IVV real en vivo.
          </h2>
          <p className="lead-copy">
            Ajusta lluvia acumulada, pendiente topográfica, distancia a quebrada
            y susceptibilidad del suelo para observar exactamente el cálculo
            determinista que ejecuta el núcleo en Rust.
          </p>
        </header>

        <div className="ivv-lab-card" data-reveal>
          <div className="ivv-controls-col">
            <div className="ivv-presets-row">
              <span>Escenarios calibrados:</span>
              <button
                type="button"
                className="preset-chip"
                onClick={() => applyPreset("nominal")}
              >
                Valle seco (Verde)
              </button>
              <button
                type="button"
                className="preset-chip"
                onClick={() => applyPreset("carpish")}
              >
                Corte Carpish (84.00)
              </button>
              <button
                type="button"
                className="preset-chip"
                onClick={() => applyPreset("extreme")}
              >
                Límite crítico (100.00)
              </button>
            </div>

            <div className="slider-grid">
              <label className="slider-field">
                <span>
                  Lluvia 24h <strong>{precip24hMm} mm</strong>
                </span>
                <input
                  type="range"
                  min={0}
                  max={100}
                  step={1}
                  value={precip24hMm}
                  onChange={(e) => {
                    const next24 = Number(e.target.value);
                    setPrecip24hMm(next24);
                    if (precip72hMm < next24) setPrecip72hMm(next24);
                  }}
                />
              </label>

              <label className="slider-field">
                <span>
                  Lluvia 72h <strong>{Math.max(precip72hMm, precip24hMm)} mm</strong>
                </span>
                <input
                  type="range"
                  min={0}
                  max={220}
                  step={5}
                  value={Math.max(precip72hMm, precip24hMm)}
                  onChange={(e) =>
                    setPrecip72hMm(Math.max(Number(e.target.value), precip24hMm))
                  }
                />
              </label>

              <label className="slider-field">
                <span>
                  Pendiente máxima <strong>{slopePct}%</strong>
                </span>
                <input
                  type="range"
                  min={0}
                  max={45}
                  step={1}
                  value={slopePct}
                  onChange={(e) => setSlopePct(Number(e.target.value))}
                />
              </label>

              <label className="slider-field">
                <span>
                  Distancia a quebrada <strong>{ravineDistM} m</strong>
                </span>
                <input
                  type="range"
                  min={0}
                  max={500}
                  step={10}
                  value={ravineDistM}
                  onChange={(e) => setRavineDistM(Number(e.target.value))}
                />
              </label>

              <label className="slider-field">
                <span>
                  Susceptibilidad del suelo <strong>{soilIdx.toFixed(2)}</strong>
                </span>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={soilIdx}
                  onChange={(e) => setSoilIdx(Number(e.target.value))}
                />
              </label>

              <label className="hotspot-toggle">
                <input
                  type="checkbox"
                  checked={isHotspot}
                  onChange={(e) => setIsHotspot(e.target.checked)}
                />
                <span>
                  Punto crítico histórico INGEMMET / Provías{" "}
                  <small>({isHotspot ? "H = 1.0" : "H = 0.2"})</small>
                </span>
              </label>
            </div>
          </div>

          <div className={`ivv-readout-col ivv-readout-col--${ivv.risk}`}>
            <span className="readout-eyebrow">ÍNDICE DINÁMICO IVV</span>
            <output
              className="readout-score"
              aria-live="polite"
              data-testid="ivv-score-output"
            >
              {ivv.score.toFixed(2)}
            </output>
            <span className={`readout-risk-pill readout-risk-pill--${ivv.risk}`}>
              {RISK_LABEL[ivv.risk]}
            </span>

            <p className="readout-formula">
              <code>IVV = 100 × (0.35·P + 0.30·S + 0.20·G + 0.15·H)</code>
            </p>

            <dl className="component-breakdown">
              <div>
                <dt>P (Lluvia)</dt>
                <dd>{ivv.p.toFixed(2)}</dd>
              </div>
              <div>
                <dt>S (Pendiente/Quebrada)</dt>
                <dd>{ivv.s.toFixed(2)}</dd>
              </div>
              <div>
                <dt>G (Suelo)</dt>
                <dd>{ivv.g.toFixed(2)}</dd>
              </div>
              <div>
                <dt>H (Hotspot)</dt>
                <dd>{ivv.h.toFixed(2)}</dd>
              </div>
            </dl>
            <small className="readout-note">
              Si pasan &gt;3 horas sin refresco climático, el backend marca{" "}
              <code>STALE_IVV / UNKNOWN</code> por seguridad.
            </small>
          </div>
        </div>

        {/* Bloque 2: Selector por Perfil Operativo y B2B */}
        <div id="perfiles" className="profiles-showcase" data-reveal>
          <header className="profiles-header">
            <p className="eyebrow">Diseñado para cada actor territorial</p>
            <h2 className="display-title display-title--sm">
              Cuatro experiencias, una sola fuente de verdad.
            </h2>
          </header>

          <div
            className="profile-tablist"
            role="tablist"
            aria-label="Perfiles de uso PARADIXE SOFT"
          >
            {PROFILE_TABS.map((tab) => {
              const selected = tab.id === activeProfile.id;
              return (
                <button
                  key={tab.id}
                  id={`tab-${tab.id}`}
                  type="button"
                  role="tab"
                  aria-selected={selected}
                  aria-controls={`panel-${tab.id}`}
                  className={`profile-tab${selected ? " is-active" : ""}`}
                  onClick={() => setActiveProfileId(tab.id)}
                >
                  {tab.label}
                </button>
              );
            })}
          </div>

          <article
            id={`panel-${activeProfile.id}`}
            role="tabpanel"
            aria-labelledby={`tab-${activeProfile.id}`}
            className="profile-panel"
          >
            <div className="profile-panel-copy">
              <span className="profile-tagline">{activeProfile.tagline}</span>
              <h3>{activeProfile.headline}</h3>
              <p>{activeProfile.description}</p>
              <ul className="scrolly-bullets">
                {activeProfile.bullets.map((b) => (
                  <li key={b}>{b}</li>
                ))}
              </ul>
              <div className="profile-kpi-box">
                <strong>{activeProfile.metricValue}</strong>
                <span>{activeProfile.metricLabel}</span>
              </div>
            </div>

            <div className="profile-code-card">
              <div className="code-card-header">
                <span className="code-dots" aria-hidden="true">
                  <i />
                  <i />
                  <i />
                </span>
                <span>{activeProfile.codeTitle}</span>
              </div>
              <pre>
                <code>{activeProfile.codeSnippet}</code>
              </pre>
            </div>
          </article>
        </div>
      </div>
    </section>
  );
}
