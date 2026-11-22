import { PLATFORM_APP_URL } from "../data/productSpecs";

interface NavbarProps {
  compact: boolean;
}

const NAV_LINKS = [
  { href: "#vision", label: "Visión" },
  { href: "#capacidades", label: "Capacidades" },
  { href: "#metricas", label: "Cifras" },
  { href: "#simulador-ivv", label: "Simulador IVV" },
  { href: "#perfiles", label: "Perfiles" },
  { href: "#modulos", label: "Módulos" },
  { href: "#arquitectura", label: "Ingeniería" },
];

export function Navbar({ compact }: NavbarProps) {
  return (
    <>
      <div
        id="scroll-progress"
        className="scroll-progress-bar"
        aria-hidden="true"
      />
      <header className={`site-header${compact ? " is-compact" : ""}`}>
        <div className="site-header-inner">
          <a
            href="#top"
            className="brand-mark"
            aria-label="PARADIXE SOFT por TakumiSoft — Inicio"
          >
            <img
              src="/favicon.svg"
              alt=""
              width={26}
              height={26}
              className="brand-mark-icon"
            />
            <span className="brand-wordmark">
              <strong>PARADIXE SOFT</strong>
              <span className="brand-by">by TakumiSoft</span>
            </span>
          </a>

          <nav className="primary-nav" aria-label="Navegación principal">
            {NAV_LINKS.map((item) => (
              <a key={item.href} href={item.href} className="nav-link">
                {item.label}
              </a>
            ))}
          </nav>

          <div className="header-cta-group">
            <a href="#simulador-ivv" className="nav-pill nav-pill--ghost">
              Probar IVV
            </a>
            <a
              href={PLATFORM_APP_URL}
              className="nav-pill nav-pill--primary"
              target="_blank"
              rel="noopener noreferrer"
            >
              Abrir Plataforma
            </a>
          </div>
        </div>
      </header>
    </>
  );
}
