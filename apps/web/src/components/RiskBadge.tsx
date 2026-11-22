import type { components } from "../api/schema.d.ts";

type RiskLevel = components["schemas"]["RiskLevel"];

const RISK_META: Record<RiskLevel, { dotColor: string; label: string }> = {
  GREEN: { dotColor: "green", label: "Libre" },
  AMBER: { dotColor: "amber", label: "Precaución" },
  RED: { dotColor: "red", label: "Crítico" },
  UNKNOWN: { dotColor: "unknown", label: "Sin clima" },
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
    ivvScore !== null && !isStale ? ` · ${ivvScore.toFixed(0)}` : "";

  return (
    <span className={`risk-badge risk-badge--${effectiveLevel}`}>
      <span className="risk-dot" aria-hidden="true" />
      <span>
        {meta.label}
        {scoreLabel}
      </span>
    </span>
  );
}
