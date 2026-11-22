import type { components } from "../api/schema.d.ts";

type RiskLevel = components["schemas"]["RiskLevel"];

const RISK_META: Record<RiskLevel, { icon: string; label: string }> = {
  GREEN: { icon: "●", label: "Bajo (Verde)" },
  AMBER: { icon: "▲", label: "Precaución (Ámbar)" },
  RED: { icon: "■", label: "Crítico (Rojo)" },
  UNKNOWN: { icon: "?", label: "Sin verificación vigente" },
};

export interface RiskBadgeProps {
  riskLevel: RiskLevel;
  ivvScore: number | null;
  isStale: boolean;
}

export function RiskBadge({ riskLevel, ivvScore, isStale }: RiskBadgeProps) {
  const effectiveLevel: RiskLevel = isStale ? "UNKNOWN" : riskLevel;
  const meta = RISK_META[effectiveLevel];
  const scoreLabel =
    ivvScore !== null && !isStale ? ` · IVV ${ivvScore.toFixed(2)}` : isStale ? " · Vencido" : "";

  return (
    <span className={`risk-badge risk-badge--${effectiveLevel}`}>
      <span aria-hidden="true">{meta.icon}</span>
      <span>
        {meta.label}
        {scoreLabel}
      </span>
    </span>
  );
}
