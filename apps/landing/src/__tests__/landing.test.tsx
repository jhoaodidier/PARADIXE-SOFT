import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { computeDeterministicIvv } from "../data/productSpecs";

describe("Landing Promocional PARADIXE SOFT by TakumiSoft", () => {
  it("renderiza estructura editorial con branding TakumiSoft, único h1 y cero términos heredados", () => {
    const { container } = render(<App />);

    const h1s = screen.getAllByRole("heading", { level: 1 });
    expect(h1s).toHaveLength(1);
    expect(h1s[0].textContent).toMatch(/Inteligencia vial determinista/i);

    const text = container.textContent ?? "";
    expect(text).toMatch(/TakumiSoft/i);
    expect(text).toMatch(/PARADIXE SOFT/i);
    expect(text).toMatch(/31,622/);
    expect(text).toMatch(/6,482/);
    expect(text).not.toMatch(/qulla|hackathon|wfeo/i);
  });

  it("replica exactamente la fórmula IVV determinista de crates/paradixe-ivv", () => {
    const carpish = computeDeterministicIvv({
      precip24hMm: 85,
      precip72hMm: 190,
      slopePct: 27,
      ravineDistM: 200,
      soilIdx: 0.8,
      isHotspot: true,
    });
    expect(carpish.score).toBe(84);
    expect(carpish.risk).toBe("RED");

    const minCase = computeDeterministicIvv({
      precip24hMm: 0,
      precip72hMm: 0,
      slopePct: 0,
      ravineDistM: 500,
      soilIdx: 0,
      isHotspot: false,
    });
    expect(minCase.score).toBe(3);
    expect(minCase.risk).toBe("GREEN");

    const maxCase = computeDeterministicIvv({
      precip24hMm: 80,
      precip72hMm: 180,
      slopePct: 45,
      ravineDistM: 0,
      soilIdx: 1,
      isHotspot: true,
    });
    expect(maxCase.score).toBe(100);
    expect(maxCase.risk).toBe("RED");
  });

  it("actualiza el simulador IVV interactivo al elegir presets calibrados", () => {
    render(<App />);

    const scoreOut = screen.getByTestId("ivv-score-output");
    expect(scoreOut.textContent).toBe("84.00");

    fireEvent.click(screen.getByRole("button", { name: /Valle seco/i }));
    expect(Number(scoreOut.textContent)).toBeLessThan(40);

    fireEvent.click(screen.getByRole("button", { name: /Límite crítico/i }));
    expect(scoreOut.textContent).toBe("100.00");
  });

  it("cambia pestañas por perfil operativo y muestra integración B2B X-API-Key", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("tab", { name: /Desarrolladores B2B/i }));
    const panel = screen.getByRole("tabpanel");
    expect(panel.textContent).toMatch(/X-API-Key/);
    expect(panel.textContent).toMatch(/X-Paradixe-Signature/);

    fireEvent.click(screen.getByRole("tab", { name: /COER y Defensa Civil/i }));
    expect(screen.getByRole("tabpanel").textContent).toMatch(
      /UNVERIFIED_PHYSICAL_ATTRIBUTES/,
    );
  });

  it("permite navegar capítulos scrollytelling y controles de carrusel", () => {
    render(<App />);

    const dotTwo = screen.getByRole("tab", {
      name: /Ver Solo rutas físicamente verificables/i,
    });
    fireEvent.click(dotTwo);
    expect(dotTwo).toHaveAttribute("aria-selected", "true");

    expect(
      screen.getByRole("button", { name: "Siguiente módulo" }),
    ).toBeInTheDocument();
  });
});
