import { useEffect, useState } from "react";

/**
 * Progressive enhancement para efectos ligados al scroll.
 * - Cuando el navegador soporta CSS Scroll-Driven Animations (`animation-timeline`),
 *   el desplazamiento y las revelaciones corren nativamente en CSS sobre compositor.
 * - Cuando NO los soporta y el usuario no prefiere movimiento reducido, aplica
 *   fallbacks livianos con `IntersectionObserver` y listener pasivo de `scroll`.
 * - Jamás usa `scroll-timeline-polyfill`.
 */
export function useScrollEnhancements() {
  const [isHeaderCompact, setIsHeaderCompact] = useState(false);

  useEffect(() => {
    const prefersNoReducedMotion =
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: no-preference)").matches;

    const onScrollCompact = () => {
      setIsHeaderCompact(window.scrollY > 36);
    };
    onScrollCompact();
    window.addEventListener("scroll", onScrollCompact, { passive: true });

    const supportsScroll =
      typeof CSS !== "undefined" &&
      typeof CSS.supports === "function" &&
      CSS.supports(
        "(animation-timeline: scroll()) and (animation-range: 0% 100%)",
      );
    const supportsView =
      typeof CSS !== "undefined" &&
      typeof CSS.supports === "function" &&
      CSS.supports("(animation-timeline: view()) and (animation-range: entry)");

    const cleanups: Array<() => void> = [
      () => window.removeEventListener("scroll", onScrollCompact),
    ];

    if (!supportsScroll && prefersNoReducedMotion) {
      const progressBar = document.getElementById("scroll-progress");
      const updateProgress = () => {
        if (!progressBar) return;
        const maxScroll =
          document.documentElement.scrollHeight - window.innerHeight;
        const ratio = maxScroll > 0 ? Math.min(1, window.scrollY / maxScroll) : 0;
        progressBar.style.transform = `scaleX(${ratio})`;
      };
      updateProgress();
      window.addEventListener("scroll", updateProgress, { passive: true });
      cleanups.push(() =>
        window.removeEventListener("scroll", updateProgress),
      );
    }

    if (
      !supportsView &&
      prefersNoReducedMotion &&
      typeof IntersectionObserver !== "undefined"
    ) {
      const revealEls = Array.from(
        document.querySelectorAll<HTMLElement>("[data-reveal]"),
      );
      revealEls.forEach((el) => el.classList.add("reveal-pending"));

      const io = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (entry.isIntersecting) {
              const target = entry.target as HTMLElement;
              target.classList.add("is-revealed");
              target.classList.remove("reveal-pending");
              io.unobserve(target);
            }
          }
        },
        { threshold: 0.14, rootMargin: "0px 0px -40px 0px" },
      );

      revealEls.forEach((el) => io.observe(el));
      cleanups.push(() => io.disconnect());
    }

    return () => {
      for (const fn of cleanups) fn();
    };
  }, []);

  return { isHeaderCompact };
}
