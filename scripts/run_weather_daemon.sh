#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ -f ".env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source ".env"
  set +a
fi

REGION="${1:-PE-HUC}"
INTERVAL_HOURS="${2:-2}"

if [[ -x "./target/release/paradixe-ingestor" ]]; then
  exec ./target/release/paradixe-ingestor weather-sync --region "$REGION" --interval-hours "$INTERVAL_HOURS"
fi

export PATH="$HOME/.cargo/bin:$PATH"
exec cargo run --release -p paradixe-ingestor -- weather-sync --region "$REGION" --interval-hours "$INTERVAL_HOURS"
