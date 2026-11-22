#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || "${2}" != "--confirm-restore" ]]; then
  echo "Uso: $0 <ruta_al_archivo.dump> --confirm-restore" >&2
  exit 1
fi

DUMP_PATH="$1"
SUM_PATH="${DUMP_PATH}.sha256"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -f "${ROOT_DIR}/.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/.env"
  set +a
fi

: "${DATABASE_URL:?DATABASE_URL debe estar configurado}"
[[ -f "$DUMP_PATH" ]] || { echo "No existe dump: $DUMP_PATH" >&2; exit 1; }
[[ -f "$SUM_PATH" ]] || { echo "Falta checksum SHA-256: $SUM_PATH" >&2; exit 1; }

(cd "$(dirname "$DUMP_PATH")" && sha256sum -c "$(basename "$SUM_PATH")")

echo "[RESTORE] Restaurando $DUMP_PATH en PostGIS..."
pg_restore --clean --if-exists --no-owner --no-privileges --dbname="$DATABASE_URL" "$DUMP_PATH"
echo "[RESTORE] Restauración finalizada."
