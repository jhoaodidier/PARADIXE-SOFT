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

: "${DATABASE_URL:?DATABASE_URL debe estar configurado}"
BACKUP_DIR="${1:-${BACKUP_DIR:-${ROOT_DIR}/backups}}"
RETENTION_DAYS="${RETENTION_DAYS:-7}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

mkdir -p "$BACKUP_DIR"
DUMP_PATH="${BACKUP_DIR}/paradixe_postgis_${STAMP}.dump"
SUM_PATH="${DUMP_PATH}.sha256"

echo "[BACKUP] Ejecutando pg_dump comprimido hacia ${DUMP_PATH}..."
pg_dump "$DATABASE_URL" --format=custom --compress=9 --no-owner --no-privileges -f "$DUMP_PATH"

sha256sum "$DUMP_PATH" > "$SUM_PATH"
(cd "$BACKUP_DIR" && sha256sum -c "$(basename "$SUM_PATH")")

if [[ -n "${BACKUP_S3_BUCKET:-}" ]] && command -v aws >/dev/null 2>&1; then
  aws ${S3_ENDPOINT:+--endpoint-url "$S3_ENDPOINT"} s3 cp "$DUMP_PATH" "s3://${BACKUP_S3_BUCKET}/$(basename "$DUMP_PATH")"
  aws ${S3_ENDPOINT:+--endpoint-url "$S3_ENDPOINT"} s3 cp "$SUM_PATH" "s3://${BACKUP_S3_BUCKET}/$(basename "$SUM_PATH")"
  echo "[BACKUP] Respaldo replicado en s3://${BACKUP_S3_BUCKET}/"
fi

if [[ "$RETENTION_DAYS" -gt 0 ]]; then
  find "$BACKUP_DIR" -maxdepth 1 -type f \( -name 'paradixe_postgis_*.dump' -o -name 'paradixe_postgis_*.dump.sha256' \) -mtime "+${RETENTION_DAYS}" -delete
fi

ls -lh "$DUMP_PATH" "$SUM_PATH"
echo "[BACKUP] OK"
