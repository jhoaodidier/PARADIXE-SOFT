import { PLATFORM_APP_URL } from "../data/productSpecs";

export function FooterCta() {
  return (
    <footer className="footer-section">
      <div className="section-shell">
        <div className="closing-cta-panel" data-reveal>
          <p className="eyebrow">Listo para despacho inmediato</p>
          <h2 className="display-title">
            Conduce, coordina e integra con certeza determinista.
          </h2>
          <p className="lead-copy">
            Accede ahora a la consola operativa en vivo o integra la API
            empresarial de PARADIXE SOFT en tu organización.
          </p>
          <div className="hero-actions">
            <a
              href={PLATFORM_APP_URL}
              className="cta-btn cta-btn--primary"
              target="_blank"
              rel="noopener noreferrer"
            >
              Abrir consola PARADIXE SOFT ↗
            </a>
            <a href="#top" className="cta-btn cta-btn--secondary">
              Volver arriba ↑
            </a>
          </div>
        </div>

        <div className="footer-meta-bar">
          <div className="footer-brand-col">
            <strong>PARADIXE SOFT</strong>
            <span>
              Desarrollado por <strong>TakumiSoft (TAKUMISOFT)</strong> ·
              Resiliencia territorial Offline-First
            </span>
          </div>
          <div className="footer-links-col">
            <a href="#capacidades">Motor IVV</a>
            <a href="#simulador-ivv">Simulador en vivo</a>
            <a href="#perfiles">B2B &amp; COER</a>
            <a href="#arquitectura">Arquitectura Rust</a>
          </div>
        </div>
      </div>
    </footer>
  );
}
