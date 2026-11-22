import { useRef } from "react";
import { MODULE_SLIDES } from "../data/productSpecs";

export function CarouselShowcase() {
  const scrollerRef = useRef<HTMLUListElement>(null);

  function scrollBySlide(direction: -1 | 1) {
    const el = scrollerRef.current;
    if (!el) return;
    const step = Math.max(300, Math.round(el.clientWidth * 0.72));
    el.scrollBy({ left: direction * step, behavior: "smooth" });
  }

  return (
    <section
      id="modulos"
      className="light-section carousel-section"
      aria-labelledby="carousel-heading"
    >
      <div className="section-shell">
        <div className="carousel-header-row" data-reveal>
          <div>
            <p className="eyebrow eyebrow--light">Ecosistema Completo</p>
            <h2 id="carousel-heading" className="display-title display-title--light">
              Cada módulo construido para resistir el mundo real.
            </h2>
          </div>

          <div className="carousel-controls">
            <button
              type="button"
              className="carousel-nav-btn"
              onClick={() => scrollBySlide(-1)}
              aria-label="Módulo anterior"
            >
              ‹
            </button>
            <button
              type="button"
              className="carousel-nav-btn"
              onClick={() => scrollBySlide(1)}
              aria-label="Siguiente módulo"
            >
              ›
            </button>
          </div>
        </div>
      </div>

      <ul
        ref={scrollerRef}
        className="carousel-track"
        tabIndex={0}
        aria-label="Módulos operativos de PARADIXE SOFT"
      >
        {MODULE_SLIDES.map((slide) => (
          <li key={slide.id} className="carousel-slide">
            <div className="slide-media-wrap">
              <img
                src={slide.image.src}
                alt={slide.image.alt}
                width={slide.image.width}
                height={slide.image.height}
                decoding="async"
                className="slide-img"
              />
            </div>
            <div className="slide-body">
              <span className="slide-eyebrow">{slide.eyebrow}</span>
              <h3>{slide.title}</h3>
              <p>{slide.summary}</p>
              <ul className="slide-spec-chips">
                {slide.specs.map((spec) => (
                  <li key={spec}>{spec}</li>
                ))}
              </ul>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
