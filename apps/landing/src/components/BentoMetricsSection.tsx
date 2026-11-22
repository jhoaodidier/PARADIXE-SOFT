import { BENTO_METRICS } from "../data/productSpecs";

export function BentoMetricsSection() {
  return (
    <section
      id="metricas"
      className="light-section bento-section"
      aria-labelledby="bento-heading"
    >
      <div className="section-shell">
        <header className="section-intro section-intro--light" data-reveal>
          <p className="eyebrow eyebrow--light">Escala Verificada en Producción</p>
          <h2 id="bento-heading" className="display-title display-title--light">
            Cada número respaldado por datos reales y tests vivos.
          </h2>
          <p className="lead-copy lead-copy--light">
            Desplegado y auditado sobre la Macro-Región Centro del Perú con
            certificación física completa del 100% de ejes pavimentados.
          </p>
        </header>

        <div className="bento-grid">
          {BENTO_METRICS.map((card) => (
            <article
              key={card.id}
              data-reveal
              className={`bento-card bento-card--${card.span}${
                card.accent ? ` bento-card--${card.accent}` : ""
              }`}
            >
              <strong className="bento-value">{card.value}</strong>
              <h3 className="bento-label">{card.label}</h3>
              <p className="bento-detail">{card.detail}</p>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}
