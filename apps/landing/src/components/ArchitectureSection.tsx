import { REGION_COVERAGE, RUST_CRATES } from "../data/productSpecs";

const SECURITY_PILLARS = [
  {
    title: "Autenticación Dual y Limpieza Activa",
    detail:
      "JWT auto-renovable cada 15 min con detección de reuso de familia, Argon2id y cabecera Clear-Site-Data al cerrar sesión.",
  },
  {
    title: "Webhooks B2B Anti-SSRF",
    detail:
      "Validación estricta de esquema HTTPS y bloqueo de IPs privadas, loopback o metadatos cloud (HTTP 422), firmados HMAC-SHA256.",
  },
  {
    title: "Hardening Nginx + TLS 1.3",
    detail:
      "Política CSP estricta con cero scripts inline, cabeceras COOP/CORP/Permissions-Policy y limitación de tasa en autenticación y API.",
  },
  {
    title: "Respaldos PostGIS SHA-256 y Observabilidad",
    detail:
      "Scripts automáticos pg_dump comprimidos validados con SHA-256 junto a métricas Prometheus (/metrics) y tablero Grafana COER.",
  },
];

export function ArchitectureSection() {
  return (
    <section
      id="arquitectura"
      className="arch-section"
      aria-labelledby="arch-heading"
    >
      <div className="section-shell">
        <header className="section-intro" data-reveal>
          <p className="eyebrow">Ingeniería TakumiSoft</p>
          <h2 id="arch-heading" className="display-title">
            Ocho crates en Rust. Cero bloques unsafe.
          </h2>
          <p className="lead-copy">
            Cada motor sigue el principio Library-First & CLI con contratos
            tipados OpenAPI 3.1.0, PostGIS 3.4 geodésico y pruebas automatizadas
            sobre infraestructura viva.
          </p>
        </header>

        <div className="crates-grid" data-reveal>
          {RUST_CRATES.map((crate) => (
            <article key={crate.name} className="crate-card">
              <div className="crate-top">
                <code className="crate-name">{crate.name}</code>
                <span className="crate-tag">Rust</span>
              </div>
              <p className="crate-desc">{crate.responsibility}</p>
              <span className="crate-guarantee">{crate.guarantee}</span>
            </article>
          ))}
        </div>

        <div className="arch-split-row" data-reveal>
          <div className="coverage-card">
            <div className="card-head-row">
              <h3>Cobertura Macro-Región Centro Certificada</h3>
              <span className="certified-pill">6,482 / 6,482 pavimentados (100%)</span>
            </div>
            <div className="table-scroll">
              <table className="coverage-table">
                <thead>
                  <tr>
                    <th scope="col">ISO</th>
                    <th scope="col">Departamento</th>
                    <th scope="col">Tramos Totales</th>
                    <th scope="col">Ejes Pavimentados</th>
                    <th scope="col">Puntos Críticos</th>
                  </tr>
                </thead>
                <tbody>
                  {REGION_COVERAGE.map((r) => (
                    <tr key={r.iso}>
                      <td>
                        <code>{r.iso}</code>
                      </td>
                      <th scope="row">{r.name}</th>
                      <td>{r.segments}</td>
                      <td className="certified-cell">{r.pavedCertified}</td>
                      <td>{r.hotspots}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          <div className="security-card">
            <h3>Blindaje de Seguridad y Producción</h3>
            <ul className="security-list">
              {SECURITY_PILLARS.map((item) => (
                <li key={item.title}>
                  <strong>{item.title}</strong>
                  <span>{item.detail}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
    </section>
  );
}
