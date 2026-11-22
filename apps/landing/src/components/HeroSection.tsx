import { HERO_MEDIA, PLATFORM_APP_URL } from "../data/productSpecs";

const HERO_PILLS = [
  "Núcleo Rust #![forbid(unsafe_code)]",
  "Offline-First PMTiles v3",
  "PostGIS 3.4 ::geography",
  "Macro-Región Centro Activa",
];

export function HeroSection() {
  return (
    <section id="top" className="hero-section">
      <div className="hero-ambient-glow" aria-hidden="true" />
      <div className="section-shell hero-shell">
        <div className="hero-copy">
          <p className="hero-kicker">
            <span className="kicker-dot" aria-hidden="true" />
            UN PRODUCTO DE TAKUMISOFT
          </p>

          <h1 className="hero-headline">
            Inteligencia vial determinista para los Andes.
          </h1>

          <p className="hero-subhead">
            PARADIXE SOFT anticipa deslizamientos hasta <strong>+72 horas</strong>,
            calcula rutas <em>Fail-Safe</em> por vehículo y opera sin cobertura
            celular para salvar vidas y carga agrícola crítica.
          </p>

          <div className="hero-actions">
            <a
              href={PLATFORM_APP_URL}
              className="cta-btn cta-btn--primary"
              target="_blank"
              rel="noopener noreferrer"
            >
              Abrir consola en vivo
              <span aria-hidden="true">↗</span>
            </a>
            <a href="#capacidades" className="cta-btn cta-btn--secondary">
              Explorar ingeniería
            </a>
          </div>

          <ul className="hero-spec-strip" aria-label="Garantías técnicas principales">
            {HERO_PILLS.map((pill) => (
              <li key={pill}>{pill}</li>
            ))}
          </ul>
        </div>

        <div id="vision" className="hero-stage-wrapper">
          <figure className="hero-stage-frame">
            <img
              src={HERO_MEDIA.src}
              alt={HERO_MEDIA.alt}
              width={HERO_MEDIA.width}
              height={HERO_MEDIA.height}
              fetchPriority="high"
              decoding="async"
              className="hero-stage-img"
            />
            <div className="hero-stage-scrim" aria-hidden="true" />
            <figcaption className="hero-floating-caption">
              <span className="caption-badge">Carpish · Huánuco · Pasco · Junín · Ucayali</span>
              <span>31,622 tramos reales · 6,482 ejes pavimentados certificados</span>
            </figcaption>
          </figure>
        </div>
      </div>
    </section>
  );
}
