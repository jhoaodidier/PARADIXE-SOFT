#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

: "${DATABASE_URL:?DATABASE_URL must be set}"
: "${SIMULATOR_DATABASE_URL:?SIMULATOR_DATABASE_URL must be set}"

SCHEMA_EXISTS="$(psql "$DATABASE_URL" -tAc "SELECT to_regclass('public.roles') IS NOT NULL;")"
if [ "$SCHEMA_EXISTS" != "t" ]; then
  echo "Applying migrations/0001_initial_schema.sql..."
  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0001_initial_schema.sql
else
  echo "Initial schema already present; skipping 0001_initial_schema.sql."
fi

echo "Applying migrations/0002_seed_base_catalogs.sql (pass 1)..."
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0002_seed_base_catalogs.sql
echo "Applying migrations/0002_seed_base_catalogs.sql (pass 2 - idempotency check)..."
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0002_seed_base_catalogs.sql

echo "Applying migrations/0003_territorial_regions_and_osm_lineage.sql..."
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0003_territorial_regions_and_osm_lineage.sql
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0003_territorial_regions_and_osm_lineage.sql

echo "Applying migrations/0004_simulator_fixture_segment_isolation.sql..."
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0004_simulator_fixture_segment_isolation.sql
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0004_simulator_fixture_segment_isolation.sql

echo "Applying migrations/0005_production_ops_forecasts_and_fleet_alerts.sql..."
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0005_production_ops_forecasts_and_fleet_alerts.sql
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f migrations/0005_production_ops_forecasts_and_fleet_alerts.sql

DB_NAME="$(python3 -c 'import os, urllib.parse; u = urllib.parse.urlparse(os.environ["DATABASE_URL"]); print(u.path.lstrip("/") or "paradixe_soft")')"
SIM_USER="$(python3 -c 'import os, urllib.parse; u = urllib.parse.urlparse(os.environ["SIMULATOR_DATABASE_URL"]); print(u.username or "paradixe_sim_ro")')"
SIM_PASS="$(python3 -c 'import os, urllib.parse; u = urllib.parse.urlparse(os.environ["SIMULATOR_DATABASE_URL"]); print(urllib.parse.unquote(u.password or ""))')"

if [ -z "$SIM_PASS" ]; then
  echo "SIMULATOR_DATABASE_URL must include a password for $SIM_USER" >&2
  exit 1
fi

echo "Provisioning least-privilege role '$SIM_USER' on database '$DB_NAME'..."
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q -o /dev/null \
  -v sim_user="$SIM_USER" \
  -v sim_pass="$SIM_PASS" \
  -v db_name="$DB_NAME" <<'SQL'
SELECT set_config('paradixe.sim_user', :'sim_user', false);
SELECT set_config('paradixe.sim_pass', :'sim_pass', false);
SELECT set_config('paradixe.db_name', :'db_name', false);

DO $$
DECLARE
  v_user TEXT := current_setting('paradixe.sim_user');
  v_pass TEXT := current_setting('paradixe.sim_pass');
  v_db   TEXT := current_setting('paradixe.db_name');
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = v_user) THEN
    EXECUTE format(
      'CREATE ROLE %I WITH LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE',
      v_user, v_pass
    );
  ELSE
    EXECUTE format(
      'ALTER ROLE %I WITH LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE',
      v_user, v_pass
    );
  END IF;

  EXECUTE format('GRANT CONNECT ON DATABASE %I TO %I', v_db, v_user);
  EXECUTE format('GRANT USAGE ON SCHEMA public TO %I', v_user);
  EXECUTE format('REVOKE ALL ON ALL TABLES IN SCHEMA public FROM %I', v_user);
  EXECUTE format(
    'GRANT SELECT ON roles, users, vehicle_categories, road_nodes, road_segments, ' ||
    'transfer_corridors, transfer_corridor_capabilities, ivv_model_versions, ' ||
    'routing_config_versions, impact_parameter_versions, weather_observations, ' ||
    'ivv_assessments, incidents, humanitarian_windows, ' ||
    'territorial_units, hydrography_waterways, corridor_alert_subscriptions, weather_forecasts, ' ||
    'simulation_scenarios, sim_events, simulation_impact_logs TO %I',
    v_user
  );
  EXECUTE format(
    'GRANT INSERT, UPDATE, DELETE ON simulation_scenarios, sim_events, simulation_impact_logs TO %I',
    v_user
  );
END $$;
SQL

TABLE_COUNT="$(psql "$DATABASE_URL" -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name <> 'spatial_ref_sys';")"
ROLE_COUNT="$(psql "$DATABASE_URL" -tAc "SELECT count(*) FROM roles;")"
VEH_COUNT="$(psql "$DATABASE_URL" -tAc "SELECT count(*) FROM vehicle_categories;")"
SYNC_SEQ="$(psql "$DATABASE_URL" -tAc "SELECT last_seq FROM sync_control WHERE singleton_id = 1;")"
IVV_TAG="$(psql "$DATABASE_URL" -tAc "SELECT version_tag FROM ivv_model_versions WHERE is_active = TRUE;")"
RTE_TAG="$(psql "$DATABASE_URL" -tAc "SELECT version_tag FROM routing_config_versions WHERE is_active = TRUE;")"
IMP_COUNT="$(psql "$DATABASE_URL" -tAc "SELECT count(*) FROM impact_parameter_versions WHERE is_active = TRUE;")"

echo "Verified: tables=$TABLE_COUNT roles=$ROLE_COUNT vehicles=$VEH_COUNT sync_seq=$SYNC_SEQ ivv=$IVV_TAG routing=$RTE_TAG impact_active=$IMP_COUNT"

test "$TABLE_COUNT" -ge 31
test "$ROLE_COUNT" -eq 5
test "$VEH_COUNT" -eq 5
test "$SYNC_SEQ" -ge 0
test "$IVV_TAG" = "ivv-v1.0-initial"
test "$RTE_TAG" = "routing-v1.0"
test "$IMP_COUNT" -eq 5

SIM_VEH_COUNT="$(psql "$SIMULATOR_DATABASE_URL" -tAc "SELECT count(*) FROM vehicle_categories;")"
test "$SIM_VEH_COUNT" -eq 5

if psql "$SIMULATOR_DATABASE_URL" -v ON_ERROR_STOP=1 -c "DELETE FROM roles;" >/dev/null 2>&1; then
  echo "SECURITY ERROR: $SIM_USER was allowed to DELETE FROM roles!" >&2
  exit 1
fi

echo "Confirmed: '$SIM_USER' can read catalogs ($SIM_VEH_COUNT vehicles) and is denied writes on operational tables."
