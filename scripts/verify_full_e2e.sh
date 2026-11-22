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

: "${DATABASE_URL:?DATABASE_URL must be configured in environment or .env}"
: "${WHATSAPP_VERIFY_TOKEN:=paradixe_verify_e2e}"
: "${WHATSAPP_WEBHOOK_SECRET:=paradixe_webhook_e2e}"
: "${S3_ENDPOINT:=http://127.0.0.1:9000}"
: "${S3_BUCKET_EVIDENCE:=paradixe-evidence}"
: "${S3_PUBLIC_EVIDENCE_BASE:=/evidence}"

API_PORT=18080
MSG_PORT=18090
API_BASE="http://127.0.0.1:${API_PORT}"
MSG_BASE="http://127.0.0.1:${MSG_PORT}"

API_BIN="${ROOT_DIR}/target/debug/paradixe-api"
MSG_BIN="${ROOT_DIR}/target/debug/paradixe-messaging"

if [[ ! -x "$API_BIN" || ! -x "$MSG_BIN" ]]; then
  echo "[E2E] Compilando binarios paradixe-api y paradixe-messaging..."
  cargo build -p paradixe-api --bin paradixe-api -p paradixe-messaging
fi

TMP_DIR="$(mktemp -d)"
API_PID=""
MSG_PID=""

cleanup() {
  local rc=$?
  if [[ -n "$MSG_PID" ]]; then
    kill "$MSG_PID" 2>/dev/null || true
    wait "$MSG_PID" 2>/dev/null || true
  fi
  if [[ -n "$API_PID" ]]; then
    kill "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
  fi
  rm -rf "$TMP_DIR"
  exit "$rc"
}
trap cleanup EXIT INT TERM

http_req() {
  local expected_re="$1"
  local url="$2"
  shift 2
  local resp_file="${TMP_DIR}/http_resp.txt"
  local code
  code="$(curl -sS -o "$resp_file" -w "%{http_code}" "$url" "$@")"
  if [[ ! "$code" =~ ^(${expected_re})$ ]]; then
    echo "[FAIL] HTTP ${code} en ${url}" >&2
    cat "$resp_file" >&2
    echo "" >&2
    return 1
  fi
  cat "$resp_file"
}

echo "[E2E] Iniciando paradixe-api en :${API_PORT} y paradixe-messaging en :${MSG_PORT}..."
API_BIND_ADDR="127.0.0.1:${API_PORT}" \
MESSAGING_INTERNAL_URL="$MSG_BASE" \
WHATSAPP_WEBHOOK_SECRET="$WHATSAPP_WEBHOOK_SECRET" \
S3_ENDPOINT="$S3_ENDPOINT" \
S3_BUCKET_EVIDENCE="$S3_BUCKET_EVIDENCE" \
S3_PUBLIC_EVIDENCE_BASE="$S3_PUBLIC_EVIDENCE_BASE" \
"$API_BIN" >"${TMP_DIR}/api.log" 2>&1 &
API_PID=$!

MESSAGING_BIND_ADDR="127.0.0.1:${MSG_PORT}" \
PARADIXE_API_URL="${API_BASE}/api/v1" \
WHATSAPP_VERIFY_TOKEN="$WHATSAPP_VERIFY_TOKEN" \
WHATSAPP_WEBHOOK_SECRET="$WHATSAPP_WEBHOOK_SECRET" \
"$MSG_BIN" serve >"${TMP_DIR}/msg.log" 2>&1 &
MSG_PID=$!

for _ in $(seq 1 40); do
  if curl -sf "${API_BASE}/health" >/dev/null && curl -sf "${MSG_BASE}/health" >/dev/null; then
    break
  fi
  sleep 0.25
done

curl -sf "${API_BASE}/health" >/dev/null || { echo "[FAIL] paradixe-api no respondió /health"; cat "${TMP_DIR}/api.log"; exit 1; }
curl -sf "${MSG_BASE}/health" >/dev/null || { echo "[FAIL] paradixe-messaging no respondió /health"; cat "${TMP_DIR}/msg.log"; exit 1; }
echo "[PASS] Healthchecks API y Messaging OK"

# 1. Métricas Prometheus, regiones y analítica regional
http_req "200" "${API_BASE}/metrics" | grep -q "paradixe_routing_requests_total"
echo "[PASS] Prometheus /metrics expone contadores PARADIXE"

REGION_COUNT="$(http_req "200" "${API_BASE}/api/v1/regions" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["regions"]))')"
[[ "$REGION_COUNT" -ge 4 ]] || { echo "[FAIL] Se esperaban >=4 unidades territoriales, obtenido $REGION_COUNT"; exit 1; }
echo "[PASS] GET /api/v1/regions -> ${REGION_COUNT} unidades territoriales"

http_req "200" "${API_BASE}/api/v1/analytics/summary?region=PE-HUC" | python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["total_segments"] > 0, f"total_segments inesperado: {d}"
print("[PASS] GET /api/v1/analytics/summary -> region=%s, total_segments=%s, certified_segments=%s (%s%%), hospitals=%s" % (d["region"], d["total_segments"], d["certified_segments"], d["certified_pct"], d["hospitals_count"]))
'

# 2. Registro de conductor (DRIVER) con teléfono E.164 dinámico
RAND_SUFFIX="$(date +%s | tail -c 6)"
DRIVER_PHONE="+519629${RAND_SUFFIX}"
DRIVER_TOKEN="$(http_req "201" "${API_BASE}/api/v1/auth/register" \
  -X POST -H "Content-Type: application/json" \
  -d "{\"phone_e164\":\"${DRIVER_PHONE}\",\"full_name\":\"Conductor E2E ${RAND_SUFFIX}\",\"password\":\"ParadixeE2E2026!\",\"device_id\":\"e2e-driver-${RAND_SUFFIX}\"}" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["access_token"])')"
[[ -n "$DRIVER_TOKEN" ]]
echo "[PASS] POST /api/v1/auth/register (201 Created, DRIVER JWT emitido)"

# 3. Login COER_ADMIN y listado RBAC /admin/users
ADMIN_TOKEN="$(http_req "200" "${API_BASE}/api/v1/auth/login" \
  -X POST -H "Content-Type: application/json" \
  -d '{"phone_e164":"+51962000100","password":"ParadixeDev2026!","device_id":"e2e-coer"}' \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["access_token"])')"
[[ -n "$ADMIN_TOKEN" ]]

USER_COUNT="$(http_req "200" "${API_BASE}/api/v1/admin/users" -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["users"]))')"
[[ "$USER_COUNT" -ge 3 ]] || { echo "[FAIL] GET /api/v1/admin/users devolvió $USER_COUNT usuarios"; exit 1; }
echo "[PASS] POST /auth/login COER_ADMIN + GET /admin/users (${USER_COUNT} cuentas)"

# 4. Consultar nodos reales de PostGIS para ruteo certificado, ruteo Fail-Safe y corredor de transbordo
CERT_ROW="$(psql "$DATABASE_URL" -t -A -F '|' -c "
SELECT rs.segment_id, rs.origin_node_id, rs.destination_node_id,
       round(ST_Y(ST_LineInterpolatePoint(rs.geom, 0.5))::numeric, 6),
       round(ST_X(ST_LineInterpolatePoint(rs.geom, 0.5))::numeric, 6)
FROM road_segments rs
JOIN territorial_units tu ON ST_Intersects(rs.geom, tu.bbox_geom) AND tu.iso_code = 'PE-HUC'
JOIN LATERAL (
    SELECT ia.ivv_score
    FROM weather_observations wo
    JOIN ivv_assessments ia ON ia.observation_id = wo.observation_id
    WHERE wo.segment_id = rs.segment_id
    ORDER BY ia.calculated_at DESC LIMIT 1
) ivv ON TRUE
WHERE rs.surface_type = 'PAVED'
  AND rs.attributes_verified_at IS NOT NULL
  AND rs.is_topology_verified = TRUE
  AND ivv.ivv_score < 70
  AND NOT EXISTS (
      SELECT 1 FROM incidents i WHERE i.segment_id = rs.segment_id AND i.resolved_at IS NULL
  )
  AND rs.length_km BETWEEN 0.10 AND 1.50
ORDER BY rs.length_km DESC
LIMIT 1;
")"
IFS='|' read -r CERT_SEG_ID CERT_ORIG CERT_DEST CERT_LAT CERT_LON <<< "$CERT_ROW"
[[ -n "$CERT_SEG_ID" ]] || { echo "[FAIL] No se encontró segmento PAVED certificado en PE-HUC"; exit 1; }

UNVER_ROW="$(psql "$DATABASE_URL" -t -A -F '|' -c "
SELECT rs.origin_node_id, rs.destination_node_id
FROM road_segments rs
JOIN territorial_units tu ON ST_Intersects(rs.geom, tu.bbox_geom) AND tu.iso_code = 'PE-HUC'
WHERE rs.surface_type = 'DIRT_TRACK'
  AND rs.attributes_verified_at IS NULL
  AND rs.is_topology_verified = TRUE
  AND NOT EXISTS (
      SELECT 1 FROM road_segments r2
      WHERE (r2.origin_node_id = rs.destination_node_id OR r2.destination_node_id = rs.destination_node_id)
        AND r2.attributes_verified_at IS NOT NULL
  )
LIMIT 1;
")"
IFS='|' read -r UNVER_ORIG UNVER_DEST <<< "$UNVER_ROW"
[[ -n "$UNVER_ORIG" ]] || { echo "[FAIL] No se encontró tramo DIRT_TRACK aislado no verificado en PE-HUC"; exit 1; }

T0_NS="$(date +%s%N)"
http_req "200" "${API_BASE}/api/v1/routing/calculate" \
  -X POST \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{\"origin_node_id\":\"${CERT_ORIG}\",\"destination_node_id\":\"${CERT_DEST}\",\"vehicle_category_code\":\"PICKUP_4X4\",\"cargo_type\":\"PERISHABLE_AGRI\",\"cargo_payload_kg\":600}" \
  > "${TMP_DIR}/route_ok.json"
T1_NS="$(date +%s%N)"
ROUTE_MS="$(( (T1_NS - T0_NS) / 1000000 ))"

python3 -c '
import json, sys
d = json.load(sys.stdin)
ms = sys.argv[1]
assert d["resolution_type"] == "ALTERNATE_ROUTE", f"Esperado ALTERNATE_ROUTE, obtenido: {d}"
ei = d.get("economic_impact")
assert ei is not None, f"Falta economic_impact: {d}"
assert ei["reference_blockage_hours"] == 4.0
print("[PASS] POST /routing/calculate ALTERNATE_ROUTE en %s ms (dist=%s km, time=%s min, CO2=%s kg, ahorro=%s min, loss_avoided=$%s)" % (
    ms, d["total_distance_km"], d["estimated_time_min"], ei["estimated_co2_kg"],
    ei["time_saved_vs_blockage_min"], ei["estimated_cargo_loss_avoided_usd"]
))
' "${ROUTE_MS}" < "${TMP_DIR}/route_ok.json"

http_req "200" "${API_BASE}/api/v1/routing/calculate" \
  -X POST \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{\"origin_node_id\":\"${UNVER_ORIG}\",\"destination_node_id\":\"${UNVER_DEST}\",\"vehicle_category_code\":\"PICKUP_4X4\",\"cargo_type\":\"MEDICAL_SUPPLIES\",\"cargo_payload_kg\":400}" \
  | python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["resolution_type"] == "NO_SAFE_ROUTE", f"Esperado NO_SAFE_ROUTE, obtenido: {d}"
assert d["reason_code"] == "UNVERIFIED_PHYSICAL_ATTRIBUTES", f"Esperado UNVERIFIED_PHYSICAL_ATTRIBUTES, obtenido: {d}"
print("[PASS] POST /routing/calculate Fail-Safe -> %s (%s)" % (d["resolution_type"], d["reason_code"]))
'

# 5. Creación e inspección de corredor de transbordo sobre nodos reales
TC_ID="$(http_req "201" "${API_BASE}/api/v1/transfer-corridors" \
  -X POST \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{\"side_a_node_id\":\"${CERT_ORIG}\",\"side_b_node_id\":\"${CERT_DEST}\",\"length_m\":180,\"max_slope_pct\":6.5,\"transfer_mode\":\"PEDESTRIAN_SAFE_PATH\",\"max_safe_ivv\":69,\"inspection_valid_days\":30,\"capabilities\":[{\"cargo_type\":\"MEDICAL_SUPPLIES\",\"max_payload_kg\":1500,\"requires_medical_supervision\":false,\"estimated_transfer_time_min\":20}]}" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["corridor_id"])')"
[[ -n "$TC_ID" ]]

http_req "200" "${API_BASE}/api/v1/transfer-corridors/${TC_ID}/inspect" \
  -X PATCH \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"is_verified_safe":true,"inspection_valid_days":30,"notes":"E2E verified"}' >/dev/null

http_req "200" "${API_BASE}/api/v1/operations/bridges" | python3 -c '
import json, sys
d = json.load(sys.stdin)
tid = sys.argv[1]
assert len(d["transfer_corridors"]) >= 1
assert any(tc["corridor_id"] == tid and tc["is_verified_safe"] for tc in d["transfer_corridors"])
print("[PASS] POST/PATCH /transfer-corridors + GET /operations/bridges -> corridor_id=%s verificado (%s total)" % (tid, len(d["transfer_corridors"])))
' "${TC_ID}"

# 6. Carga binaria de evidencia fotográfica en RustFS + presign URL
PNG_FILE="${TMP_DIR}/pixel.png"
python3 -c '
import base64, sys
png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII="
open(sys.argv[1], "wb").write(base64.b64decode(png_b64))
' "$PNG_FILE"

EVID_KEY="$(http_req "201" "${API_BASE}/api/v1/uploads/evidence" \
  -X POST \
  -H "Authorization: Bearer ${DRIVER_TOKEN}" \
  --data-binary "@${PNG_FILE}" \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["content_type"]=="image/png"; assert d["photo_uri"].endswith(d["object_key"]); print(d["object_key"])')"
EVID_HTTP="$(curl -s -o /dev/null -w "%{http_code}" "${S3_ENDPOINT}/${S3_BUCKET_EVIDENCE}/${EVID_KEY}")"
[[ "$EVID_HTTP" == "200" ]] || { echo "[FAIL] RustFS GET ${EVID_KEY} devolvió HTTP $EVID_HTTP"; exit 1; }

http_req "200" "${API_BASE}/api/v1/uploads/presign" \
  -X POST \
  -H "Authorization: Bearer ${DRIVER_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"content_type":"image/jpeg","size_bytes":1024}' \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); assert "presigned_put_url" in d and "X-Amz-Signature=" in d["presigned_put_url"], d'
echo "[PASS] POST /uploads/evidence (RustFS HTTP 200 object=${EVID_KEY}) + /uploads/presign SigV4 OK"

# 7. Suscripciones de alertas de flota (crear -> listar -> eliminar)
SUB_ID="$(http_req "201" "${API_BASE}/api/v1/subscriptions" \
  -X POST \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"label":"E2E Flota Central","region_iso":"PE-HUC","corridor_name":"Carretera Central","min_risk_level":"RED","channel":"WHATSAPP_BOT"}' \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["subscription_id"])')"

http_req "200" "${API_BASE}/api/v1/subscriptions" -H "Authorization: Bearer ${ADMIN_TOKEN}" | python3 -c '
import json, sys
d = json.load(sys.stdin)
sid = sys.argv[1]
assert any(s["subscription_id"] == sid for s in d["subscriptions"]), d
' "${SUB_ID}"
http_req "200|204" "${API_BASE}/api/v1/subscriptions/${SUB_ID}" -X DELETE -H "Authorization: Bearer ${ADMIN_TOKEN}" >/dev/null
echo "[PASS] CRUD /api/v1/subscriptions (create -> list -> delete)"

# 8. Pronóstico determinista +24h
http_req "200" "${API_BASE}/api/v1/corridors/forecast?region=PE-HUC&horizon_hours=24" | python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["horizon_hours"] == 24
assert len(d["items"]) > 0, "Sin pronósticos +24h en PE-HUC"
print("[PASS] GET /api/v1/corridors/forecast -> horizon=24h, items=%s" % len(d["items"]))
'

# 9. Stream SSE en tiempo real
SSE_OUT="$(curl -sN --max-time 2 -D - "${API_BASE}/api/v1/events/stream" || true)"
printf '%s' "$SSE_OUT" | grep -qi "content-type: text/event-stream"
printf '%s' "$SSE_OUT" | grep -q "event: connected"
echo "[PASS] GET /api/v1/events/stream -> text/event-stream + event: connected"

# 10. Pasarela paradixe-messaging (WhatsApp handshake, SMS HMAC <=160, simulate-webhook)
CHALLENGE_RESP="$(http_req "200" "${MSG_BASE}/webhooks/whatsapp?hub.mode=subscribe&hub.verify_token=${WHATSAPP_VERIFY_TOKEN}&hub.challenge=PARADIXE_OK")"
[[ "$CHALLENGE_RESP" == "PARADIXE_OK" ]] || { echo "[FAIL] WhatsApp verify devolvió '$CHALLENGE_RESP'"; exit 1; }

SMS_PAYLOAD='{"sender_phone_e164":"+51962000100","body":"ESTADO Carretera Central"}'
SMS_SIG="$(printf '%s' "$SMS_PAYLOAD" | HMAC_SECRET="$WHATSAPP_WEBHOOK_SECRET" python3 -c '
import hashlib, hmac, os, sys
raw = sys.stdin.read().encode()
print("sha256=" + hmac.new(os.environ["HMAC_SECRET"].encode(), raw, hashlib.sha256).hexdigest())
')"

http_req "200" "${MSG_BASE}/webhooks/sms" \
  -X POST \
  -H "Content-Type: application/json" \
  -H "X-Webhook-Signature: ${SMS_SIG}" \
  -d "$SMS_PAYLOAD" \
  | python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["accepted"] is True, d
n = len(d["reply_sms"])
assert n <= 160, "SMS excede 160 chars: %s" % n
print("[PASS] POST /webhooks/sms HMAC OK (%s/160 chars)" % n)
'

PARADIXE_API_URL="${API_BASE}/api/v1" WHATSAPP_WEBHOOK_SECRET="$WHATSAPP_WEBHOOK_SECRET" \
  "$MSG_BIN" simulate-webhook \
  --channel WHATSAPP_BOT \
  --phone "+51962000100" \
  --segment-id "$CERT_SEG_ID" \
  --passability OPEN \
  --lat "$CERT_LAT" \
  --lon "$CERT_LON" \
  | python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["accepted"] is True, d
print("[PASS] paradixe-messaging simulate-webhook OK (accepted=%s, reply=%s)" % (d["accepted"], d["reply_preview"]))
'

echo "[E2E] TODAS LAS VERIFICACIONES E2E PASARON CORRECTAMENTE"
