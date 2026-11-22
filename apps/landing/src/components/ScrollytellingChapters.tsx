import { useEffect, useState } from "react";
import { SCROLLY_CHAPTERS } from "../data/productSpecs";

export function ScrollytellingChapters() {
  const [activeIndex, setActiveIndex] = useState(0);

  useEffect(() => {
    const stepNodes = Array.from(
      document.querySelectorAll<HTMLElement>("[data-scrolly-step]"),
    );
    if (stepNodes.length === 0 || typeof IntersectionObserver === "undefined") {
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            const idx = Number(
              (entry.target as HTMLElement).dataset.scrollyIndex ?? "0",
            );
            if (!Number.isNaN(idx)) {
              setActiveIndex(idx);
            }
          }
        }
      },
      { threshold: 0.45, rootMargin: "-15% 0px -25% 0px" },
    );

    stepNodes.forEach((node) => observer.observe(node));
    return () => observer.disconnect();
  }, []);

  const currentChapter = SCROLLY_CHAPTERS[activeIndex] ?? SCROLLY_CHAPTERS[0];

  return (
    <section
      id="capacidades"
      className="scrolly-section"
      aria-labelledby="scrolly-heading"
    >
      <div className="section-shell">
        <header className="section-intro" data-reveal>
          <p className="eyebrow">Arquitectura en Cuatro Capas</p>
          <h2 id="scrolly-heading" className="display-title">
            Precisión matemática desde el talud hasta la flota.
          </h2>
          <p className="lead-copy">
            Sin heurísticas opacas ni estimaciones optimistas. Cada decisión nace
            de datos georreferenciados verificados en campo y reglas explícitas en Rust.
          </p>
        </header>

        <div className="scrolly-grid">
          <div className="scrolly-narrative">
            {SCROLLY_CHAPTERS.map((chapter, index) => {
              const isActive = index === activeIndex;
              return (
                <article
                  key={chapter.id}
                  data-scrolly-step
                  data-scrolly-index={index}
                  className={`scrolly-step scrolly-step--${chapter.accent}${
                    isActive ? " is-active" : ""
                  }`}
                >
                  <button
                    type="button"
                    className="scrolly-step-trigger"
                    onClick={() => setActiveIndex(index)}
                    aria-expanded={isActive}
                  >
                    <span className="scrolly-step-num">{chapter.step}</span>
                    <span className="scrolly-eyebrow">{chapter.eyebrow}</span>
                  </button>

                  <h3>{chapter.title}</h3>
                  <p className="scrolly-summary">{chapter.summary}</p>

                  <ul className="scrolly-bullets">
                    {chapter.highlights.map((bullet) => (
                      <li key={bullet}>{bullet}</li>
                    ))}
                  </ul>

                  <div className="scrolly-mobile-media">
                    <img
                      src={chapter.image.src}
                      alt={chapter.image.alt}
                      width={chapter.image.width}
                      height={chapter.image.height}
                      decoding="async"
                    />
                  </div>
                </article>
              );
            })}
          </div>

          <div className="scrolly-sticky-column" aria-live="polite">
            <div className="scrolly-stage-card">
              <div className="scrolly-stage-meta">
                <span className="stage-step-pill">
                  Capítulo {currentChapter.step} / 04
                </span>
                <span className="stage-badge">{currentChapter.badge}</span>
              </div>

              <div className="scrolly-visual-frame">
                {SCROLLY_CHAPTERS.map((chapter, index) => (
                  <img
                    key={chapter.id}
                    src={chapter.image.src}
                    alt={chapter.image.alt}
                    width={chapter.image.width}
                    height={chapter.image.height}
                    decoding="async"
                    className={`scrolly-stage-img${
                      index === activeIndex ? " is-visible" : ""
                    }`}
                  />
                ))}
              </div>

              <div className="scrolly-stage-footer">
                <strong>{currentChapter.title}</strong>
                <div className="stage-dots" role="tablist" aria-label="Selector de capítulo">
                  {SCROLLY_CHAPTERS.map((chapter, index) => (
                    <button
                      key={chapter.id}
                      type="button"
                      role="tab"
                      aria-selected={index === activeIndex}
                      aria-label={`Ver ${chapter.title}`}
                      className={`stage-dot${index === activeIndex ? " is-active" : ""}`}
                      onClick={() => setActiveIndex(index)}
                    />
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
