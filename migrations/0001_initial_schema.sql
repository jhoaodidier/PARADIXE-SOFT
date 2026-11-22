-- ============================================================================
-- PARADIXE SOFT (TakumiSoft) — Esquema Inicial de Produccion (PostgreSQL 16 + PostGIS 3.4)
-- 3NF Operativa Estricta + Auditoria Inmutable + Sincronizacion Transaccional
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS postgis;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ----------------------------------------------------------------------------
-- 1. Identidad, Autenticacion, Seguridad y Auditoria
-- ----------------------------------------------------------------------------

CREATE TABLE roles (
    role_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code VARCHAR(32) NOT NULL UNIQUE
        CHECK (code IN ('DRIVER', 'HEALTH_OPERATOR', 'CIVIL_DEFENSE', 'COER_ADMIN', 'BOT_SERVICE')),
    base_trust_weight DECIMAL(3,2) NOT NULL
        CHECK (base_trust_weight BETWEEN 0.10 AND 1.00),
    has_authoritative_override BOOLEAN NOT NULL
);

CREATE TABLE users (
    user_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role_id UUID NOT NULL REFERENCES roles(role_id),
    phone_e164 VARCHAR(20) NOT NULL UNIQUE,
    full_name VARCHAR(120) NOT NULL,
    password_hash VARCHAR(255) NULL,
    reputation_score DECIMAL(4,2) NOT NULL DEFAULT 0.50
        CHECK (reputation_score BETWEEN 0.10 AND 1.00),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_users_role_id ON users(role_id);

CREATE TABLE auth_refresh_tokens (
    token_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_family_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users(user_id),
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    device_id VARCHAR(128) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_auth_refresh_tokens_family ON auth_refresh_tokens(token_family_id);
CREATE INDEX idx_auth_refresh_tokens_user ON auth_refresh_tokens(user_id);

CREATE TABLE otp_challenges (
    challenge_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    phone_e164 VARCHAR(20) NOT NULL,
    code_hash VARCHAR(64) NOT NULL,
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts >= 0 AND attempts <= 5),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ NULL
);

CREATE INDEX idx_otp_challenges_phone_expires ON otp_challenges(phone_e164, expires_at DESC);

CREATE TABLE audit_logs (
    audit_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_user_id UUID NULL REFERENCES users(user_id),
    action VARCHAR(64) NOT NULL,
    entity_type VARCHAR(48) NOT NULL,
    entity_id UUID NOT NULL,
    before_state_json JSONB NULL,
    after_state_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_logs_entity ON audit_logs(entity_type, entity_id, created_at DESC);
CREATE INDEX idx_audit_logs_actor ON audit_logs(actor_user_id, created_at DESC);

-- ----------------------------------------------------------------------------
-- 2. Catalogo Vehicular, Topologia Espacial y Sincronizacion Global
-- ----------------------------------------------------------------------------

CREATE TABLE vehicle_categories (
    category_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code VARCHAR(32) NOT NULL UNIQUE
        CHECK (code IN ('AMBULANCE_4X4', 'PICKUP_4X4', 'LIGHT_TRUCK', 'HEAVY_TRAILER', 'MOTORCYCLE')),
    max_slope_pct DECIMAL(4,1) NOT NULL CHECK (max_slope_pct > 0),
    min_road_width_m DECIMAL(4,2) NOT NULL CHECK (min_road_width_m > 0),
    max_weight_tons DECIMAL(5,2) NOT NULL CHECK (max_weight_tons > 0),
    allows_dirt_track BOOLEAN NOT NULL,
    priority_tier SMALLINT NOT NULL CHECK (priority_tier BETWEEN 1 AND 5)
);

CREATE TABLE road_nodes (
    node_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(120) NOT NULL,
    node_type VARCHAR(32) NOT NULL
        CHECK (node_type IN ('INTERSECTION', 'TOWN_CENTER', 'HOSPITAL', 'SAFE_TRANSFER_ZONE', 'TUNNEL_PORTAL')),
    location_geom GEOMETRY(POINT, 4326) NOT NULL,
    elevation_m INTEGER NOT NULL,
    is_geotechnically_stable BOOLEAN NOT NULL,
    urban_radius_m INTEGER NOT NULL DEFAULT 0 CHECK (urban_radius_m >= 0)
);

CREATE INDEX idx_road_nodes_location_geom ON road_nodes USING GIST (location_geom);
CREATE INDEX idx_road_nodes_location_geog ON road_nodes USING GIST ((location_geom::geography));

CREATE TABLE road_segments (
    segment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    origin_node_id UUID NOT NULL REFERENCES road_nodes(node_id),
    destination_node_id UUID NOT NULL REFERENCES road_nodes(node_id),
    corridor_name VARCHAR(100) NOT NULL,
    geom GEOMETRY(LINESTRING, 4326) NOT NULL,
    direction_type VARCHAR(16) NOT NULL CHECK (direction_type IN ('ONE_WAY', 'BIDIRECTIONAL')),
    length_km DECIMAL(6,2) NOT NULL CHECK (length_km > 0),
    avg_slope_pct DECIMAL(4,1) NOT NULL CHECK (avg_slope_pct >= 0),
    max_slope_forward_pct DECIMAL(4,1) NOT NULL CHECK (max_slope_forward_pct >= 0),
    max_slope_reverse_pct DECIMAL(4,1) NULL CHECK (max_slope_reverse_pct IS NULL OR max_slope_reverse_pct >= 0),
    road_width_m DECIMAL(4,2) NOT NULL CHECK (road_width_m > 0),
    max_supported_tons DECIMAL(5,2) NOT NULL CHECK (max_supported_tons > 0),
    surface_type VARCHAR(20) NOT NULL CHECK (surface_type IN ('PAVED', 'GRAVEL', 'DIRT_TRACK')),
    soil_susceptibility_idx DECIMAL(3,2) NOT NULL CHECK (soil_susceptibility_idx BETWEEN 0.00 AND 1.00),
    distance_to_ravine_m INTEGER NOT NULL CHECK (distance_to_ravine_m >= 0),
    is_historical_hotspot BOOLEAN NOT NULL,
    base_travel_time_min DECIMAL(6,2) NOT NULL CHECK (base_travel_time_min > 0),
    is_topology_verified BOOLEAN NOT NULL DEFAULT FALSE,
    geometry_source VARCHAR(64) NOT NULL CHECK (geometry_source IN ('OSM_CURATED', 'OFFICIAL', 'FIELD_SURVEY')),
    attributes_verified_at TIMESTAMPTZ NULL,
    CONSTRAINT chk_road_segments_distinct_nodes CHECK (origin_node_id <> destination_node_id),
    CONSTRAINT chk_road_segments_direction_slope CHECK (
        (direction_type = 'BIDIRECTIONAL' AND max_slope_reverse_pct IS NOT NULL)
        OR (direction_type = 'ONE_WAY' AND max_slope_reverse_pct IS NULL)
    )
);

CREATE INDEX idx_road_segments_geom ON road_segments USING GIST (geom);
CREATE INDEX idx_road_segments_geog ON road_segments USING GIST ((geom::geography));
CREATE INDEX idx_road_segments_nodes ON road_segments(origin_node_id, destination_node_id);

CREATE TABLE transfer_corridors (
    corridor_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    side_a_node_id UUID NOT NULL REFERENCES road_nodes(node_id),
    side_b_node_id UUID NOT NULL REFERENCES road_nodes(node_id),
    path_geom GEOMETRY(LINESTRING, 4326) NOT NULL,
    length_m INTEGER NOT NULL CHECK (length_m > 0),
    max_slope_pct DECIMAL(4,1) NOT NULL CHECK (max_slope_pct >= 0),
    transfer_mode VARCHAR(32) NOT NULL CHECK (transfer_mode IN ('PEDESTRIAN_SAFE_PATH', 'SHUTTLE_RELAY')),
    max_safe_ivv DECIMAL(5,2) NOT NULL CHECK (max_safe_ivv BETWEEN 0.00 AND 100.00),
    is_verified_safe BOOLEAN NOT NULL,
    last_inspected_at TIMESTAMPTZ NOT NULL,
    inspection_valid_days SMALLINT NOT NULL DEFAULT 30 CHECK (inspection_valid_days BETWEEN 1 AND 90),
    requires_revalidation BOOLEAN NOT NULL DEFAULT FALSE,
    verified_by_user_id UUID NULL REFERENCES users(user_id),
    CONSTRAINT chk_transfer_corridors_distinct_sides CHECK (side_a_node_id <> side_b_node_id),
    CONSTRAINT chk_transfer_corridors_verifier CHECK (
        NOT is_verified_safe OR verified_by_user_id IS NOT NULL
    )
);

CREATE INDEX idx_transfer_corridors_path_geom ON transfer_corridors USING GIST (path_geom);
CREATE INDEX idx_transfer_corridors_path_geog ON transfer_corridors USING GIST ((path_geom::geography));

CREATE TABLE transfer_corridor_capabilities (
    corridor_id UUID NOT NULL REFERENCES transfer_corridors(corridor_id) ON DELETE CASCADE,
    cargo_type VARCHAR(32) NOT NULL CHECK (cargo_type IN ('MEDICAL_PATIENT', 'MEDICAL_SUPPLIES', 'PERISHABLE_AGRI')),
    max_payload_kg DECIMAL(8,2) NOT NULL CHECK (max_payload_kg > 0),
    requires_medical_supervision BOOLEAN NOT NULL,
    estimated_transfer_time_min DECIMAL(7,2) NOT NULL CHECK (estimated_transfer_time_min > 0),
    PRIMARY KEY (corridor_id, cargo_type)
);

CREATE TABLE sync_control (
    singleton_id SMALLINT PRIMARY KEY CHECK (singleton_id = 1),
    last_seq BIGINT NOT NULL DEFAULT 0 CHECK (last_seq >= 0)
);

CREATE TABLE sync_change_log (
    change_seq BIGINT PRIMARY KEY CHECK (change_seq > 0),
    entity_type VARCHAR(32) NOT NULL CHECK (
        entity_type IN (
            'ROAD_NODE', 'ROAD_SEGMENT', 'TRANSFER_CORRIDOR', 'VEHICLE_CATEGORY',
            'IVV_ASSESSMENT', 'INCIDENT', 'LOGISTIC_BRIDGE', 'HUMANITARIAN_WINDOW'
        )
    ),
    entity_id UUID NOT NULL,
    operation VARCHAR(12) NOT NULL CHECK (operation IN ('UPSERT', 'DELETE')),
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload_json JSONB NULL,
    CONSTRAINT chk_sync_change_log_payload CHECK (
        (operation = 'DELETE' AND payload_json IS NULL)
        OR (operation = 'UPSERT' AND payload_json IS NOT NULL)
    )
);

CREATE INDEX idx_sync_change_log_entity ON sync_change_log(entity_type, entity_id);

-- ----------------------------------------------------------------------------
-- 3. Capa 1: Clima, Modelo IVV y Configuracion de Enrutamiento Versionada
-- ----------------------------------------------------------------------------

CREATE TABLE ivv_model_versions (
    model_version_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version_tag VARCHAR(32) NOT NULL UNIQUE,
    w_precip DECIMAL(4,3) NOT NULL CHECK (w_precip >= 0),
    w_slope_ravine DECIMAL(4,3) NOT NULL CHECK (w_slope_ravine >= 0),
    w_soil DECIMAL(4,3) NOT NULL CHECK (w_soil >= 0),
    w_hotspot DECIMAL(4,3) NOT NULL CHECK (w_hotspot >= 0),
    precip_24h_cap_mm DECIMAL(6,2) NOT NULL CHECK (precip_24h_cap_mm > 0),
    precip_72h_cap_mm DECIMAL(6,2) NOT NULL CHECK (precip_72h_cap_mm > 0),
    slope_cap_pct DECIMAL(4,1) NOT NULL CHECK (slope_cap_pct > 0),
    ravine_max_dist_m INTEGER NOT NULL CHECK (ravine_max_dist_m > 0),
    amber_threshold DECIMAL(5,2) NOT NULL CHECK (amber_threshold >= 0),
    red_threshold DECIMAL(5,2) NOT NULL CHECK (red_threshold > amber_threshold AND red_threshold <= 100.00),
    validity_hours SMALLINT NOT NULL CHECK (validity_hours > 0),
    is_active BOOLEAN NOT NULL,
    CONSTRAINT chk_ivv_weights_sum_one CHECK (w_precip + w_slope_ravine + w_soil + w_hotspot = 1.000)
);

CREATE UNIQUE INDEX uq_ivv_model_versions_active
    ON ivv_model_versions(is_active)
    WHERE is_active = TRUE;

CREATE TABLE routing_config_versions (
    routing_config_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version_tag VARCHAR(32) NOT NULL UNIQUE,
    alpha_ivv_weight DECIMAL(4,2) NOT NULL DEFAULT 0.75 CHECK (alpha_ivv_weight BETWEEN 0.00 AND 3.00),
    beta_suspected_penalty DECIMAL(4,2) NOT NULL DEFAULT 0.50 CHECK (beta_suspected_penalty BETWEEN 0.00 AND 2.00),
    is_active BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uq_routing_config_versions_active
    ON routing_config_versions(is_active)
    WHERE is_active = TRUE;

CREATE TABLE weather_observations (
    observation_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    segment_id UUID NOT NULL REFERENCES road_segments(segment_id),
    source_name VARCHAR(32) NOT NULL
        CHECK (source_name IN ('SENAMHI', 'NASA_GPM', 'OPEN_METEO', 'LOCAL_STATION')),
    precip_24h_mm DECIMAL(6,2) NOT NULL CHECK (precip_24h_mm >= 0),
    precip_72h_mm DECIMAL(6,2) NOT NULL CHECK (precip_72h_mm >= precip_24h_mm),
    observed_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_weather_observations_segment_time
    ON weather_observations(segment_id, observed_at DESC);

-- 3NF Estricta: Sin segment_id redundante (se resuelve via weather_observations)
CREATE TABLE ivv_assessments (
    assessment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    observation_id UUID NOT NULL REFERENCES weather_observations(observation_id),
    model_version_id UUID NOT NULL REFERENCES ivv_model_versions(model_version_id),
    ivv_score DECIMAL(5,2) NOT NULL CHECK (ivv_score BETWEEN 0.00 AND 100.00),
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ivv_assessments_observation_calc
    ON ivv_assessments(observation_id, calculated_at DESC);

-- ----------------------------------------------------------------------------
-- 4. Capa 2: Telemetria Seudonima, Reportes e Incidentes Reales
-- ----------------------------------------------------------------------------

CREATE TABLE telemetry_daily_salts (
    salt_date_utc DATE PRIMARY KEY,
    key_ref VARCHAR(128) NOT NULL
);

CREATE TABLE telemetry_pings (
    ping_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_ping_uuid UUID NOT NULL UNIQUE,
    anonymous_device_hash VARCHAR(64) NOT NULL,
    segment_id UUID NOT NULL REFERENCES road_segments(segment_id),
    location_geom GEOMETRY(POINT, 4326) NOT NULL,
    speed_kmh DECIMAL(5,2) NOT NULL CHECK (speed_kmh >= 0),
    recorded_at TIMESTAMPTZ NOT NULL,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_telemetry_pings_location_geom ON telemetry_pings USING GIST (location_geom);
CREATE INDEX idx_telemetry_pings_location_geog ON telemetry_pings USING GIST ((location_geom::geography));
CREATE INDEX idx_telemetry_pings_segment_recorded ON telemetry_pings(segment_id, recorded_at DESC);
CREATE INDEX idx_telemetry_pings_synced_at ON telemetry_pings(synced_at);

CREATE TABLE field_reports (
    report_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    client_uuid UUID NOT NULL UNIQUE,
    reporter_user_id UUID NOT NULL REFERENCES users(user_id),
    segment_id UUID NOT NULL REFERENCES road_segments(segment_id),
    proposed_passability VARCHAR(24) NOT NULL CHECK (proposed_passability IN ('BLOCKED', 'PARTIAL_ONE_LANE', 'OPEN')),
    location_geom GEOMETRY(POINT, 4326) NOT NULL,
    photo_uri VARCHAR(512) NULL,
    channel VARCHAR(20) NOT NULL CHECK (channel IN ('MOBILE_APP', 'WHATSAPP_BOT', 'SMS')),
    validation_status VARCHAR(24) NOT NULL
        CHECK (validation_status IN ('PENDING', 'VERIFIED', 'REJECTED_DISTANCE', 'REJECTED_CONTRADICTED', 'STALE_ARCHIVED')),
    captured_at_client TIMESTAMPTZ NOT NULL,
    clock_skew_flagged BOOLEAN NOT NULL DEFAULT FALSE,
    synced_at_server TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_field_reports_location_geom ON field_reports USING GIST (location_geom);
CREATE INDEX idx_field_reports_location_geog ON field_reports USING GIST ((location_geom::geography));
CREATE INDEX idx_field_reports_segment_synced ON field_reports(segment_id, synced_at_server DESC);

CREATE TABLE incidents (
    incident_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    segment_id UUID NOT NULL REFERENCES road_segments(segment_id),
    trigger_source VARCHAR(32) NOT NULL
        CHECK (trigger_source IN ('TELEMETRY_CLUSTER', 'FIELD_CONSENSUS', 'AUTHORITY_DIRECT')),
    verification_state VARCHAR(24) NOT NULL
        CHECK (verification_state IN ('SUSPECTED', 'CONFIRMED', 'DISMISSED', 'RESOLVED')),
    impact_passability VARCHAR(24) NOT NULL
        CHECK (impact_passability IN ('CAUTION_SUSPECTED', 'PARTIAL_ONE_LANE', 'BLOCKED')),
    hazard_location_geom GEOMETRY(POINT, 4326) NOT NULL,
    hazard_radius_m INTEGER NOT NULL DEFAULT 150 CHECK (hazard_radius_m > 0),
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    confirmed_at TIMESTAMPTZ NULL,
    resolved_at TIMESTAMPTZ NULL,
    CONSTRAINT chk_incidents_resolution_timestamps CHECK (
        (verification_state IN ('RESOLVED', 'DISMISSED') AND resolved_at IS NOT NULL)
        OR (verification_state IN ('SUSPECTED', 'CONFIRMED') AND resolved_at IS NULL)
    )
);

CREATE INDEX idx_incidents_hazard_geom ON incidents USING GIST (hazard_location_geom);
CREATE INDEX idx_incidents_hazard_geog ON incidents USING GIST ((hazard_location_geom::geography));
CREATE INDEX idx_incidents_segment_state ON incidents(segment_id, verification_state);

CREATE TABLE incident_report_links (
    incident_id UUID NOT NULL REFERENCES incidents(incident_id) ON DELETE CASCADE,
    report_id UUID NOT NULL REFERENCES field_reports(report_id) ON DELETE CASCADE,
    PRIMARY KEY (incident_id, report_id)
);

-- ----------------------------------------------------------------------------
-- 5. Capa 3: Puente Logistico y Corredor Humanitario
-- ----------------------------------------------------------------------------

CREATE TABLE logistic_bridges (
    bridge_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    incident_id UUID NOT NULL REFERENCES incidents(incident_id),
    corridor_id UUID NOT NULL REFERENCES transfer_corridors(corridor_id),
    cargo_type VARCHAR(32) NOT NULL CHECK (cargo_type IN ('MEDICAL_PATIENT', 'MEDICAL_SUPPLIES', 'PERISHABLE_AGRI')),
    status VARCHAR(24) NOT NULL CHECK (status IN ('PROPOSED', 'ACTIVE', 'SUSPENDED_UNSAFE', 'COMPLETED')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_logistic_bridges_incident ON logistic_bridges(incident_id, status);

CREATE TABLE humanitarian_windows (
    window_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    incident_id UUID NOT NULL REFERENCES incidents(incident_id),
    authorized_by_user_id UUID NOT NULL REFERENCES users(user_id),
    vehicle_category_id UUID NOT NULL REFERENCES vehicle_categories(category_id),
    direction VARCHAR(16) NOT NULL CHECK (direction IN ('ORIGIN_TO_DEST', 'DEST_TO_ORIGIN')),
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    status VARCHAR(20) NOT NULL CHECK (status IN ('SCHEDULED', 'ACTIVE', 'CLOSED')),
    CONSTRAINT chk_humanitarian_windows_range CHECK (ends_at > starts_at)
);

CREATE INDEX idx_humanitarian_windows_incident_dir
    ON humanitarian_windows(incident_id, direction, starts_at, ends_at);

-- ----------------------------------------------------------------------------
-- 6. Modulo de Simulacros y Contingencia Aislado (What-If / Drills)
-- ----------------------------------------------------------------------------

CREATE TABLE simulation_scenarios (
    scenario_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(120) NOT NULL,
    created_by_user_id UUID NOT NULL REFERENCES users(user_id),
    status VARCHAR(20) NOT NULL CHECK (status IN ('DRAFT', 'RUNNING', 'COMPLETED', 'ARCHIVED')),
    revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    fixture_tag VARCHAR(48) NOT NULL,
    fixture_payload_json JSONB NOT NULL,
    fixture_sha256 CHAR(64) NOT NULL CHECK (char_length(fixture_sha256) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE sim_events (
    sim_event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scenario_id UUID NOT NULL REFERENCES simulation_scenarios(scenario_id) ON DELETE CASCADE,
    segment_id UUID NOT NULL REFERENCES road_segments(segment_id),
    event_type VARCHAR(24) NOT NULL CHECK (event_type IN ('WEATHER_OVERRIDE', 'BLOCKAGE_OVERRIDE')),
    precip_24h_mm DECIMAL(6,2) NULL,
    precip_72h_mm DECIMAL(6,2) NULL,
    simulated_passability VARCHAR(24) NULL,
    hazard_location_geom GEOMETRY(POINT, 4326) NULL,
    hazard_radius_m INTEGER NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_revision BIGINT NOT NULL CHECK (applied_revision > 0),
    CONSTRAINT chk_sim_events_discriminated_payload CHECK (
        (
            event_type = 'WEATHER_OVERRIDE'
            AND precip_24h_mm IS NOT NULL AND precip_24h_mm >= 0
            AND precip_72h_mm IS NOT NULL AND precip_72h_mm >= precip_24h_mm
            AND simulated_passability IS NULL
            AND hazard_location_geom IS NULL
            AND hazard_radius_m IS NULL
        )
        OR
        (
            event_type = 'BLOCKAGE_OVERRIDE'
            AND simulated_passability IN ('BLOCKED', 'PARTIAL_ONE_LANE')
            AND hazard_location_geom IS NOT NULL
            AND hazard_radius_m IS NOT NULL AND hazard_radius_m > 0
            AND precip_24h_mm IS NULL
            AND precip_72h_mm IS NULL
        )
    )
);

CREATE INDEX idx_sim_events_hazard_geom ON sim_events USING GIST (hazard_location_geom);
CREATE INDEX idx_sim_events_hazard_geog ON sim_events USING GIST ((hazard_location_geom::geography));
CREATE INDEX idx_sim_events_scenario_rev ON sim_events(scenario_id, applied_revision);

CREATE TABLE impact_parameter_versions (
    param_version_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    version_tag VARCHAR(32) NOT NULL,
    vehicle_category_id UUID NOT NULL REFERENCES vehicle_categories(category_id),
    co2_idle_kg_per_hour DECIMAL(6,3) NOT NULL CHECK (co2_idle_kg_per_hour > 0),
    co2_drive_kg_per_km DECIMAL(6,3) NOT NULL CHECK (co2_drive_kg_per_km > 0),
    agri_cargo_value_usd DECIMAL(10,2) NOT NULL CHECK (agri_cargo_value_usd >= 0),
    perishability_loss_rate_per_hour DECIMAL(5,4) NOT NULL CHECK (perishability_loss_rate_per_hour BETWEEN 0.0000 AND 1.0000),
    transfer_handling_loss_pct DECIMAL(4,3) NOT NULL CHECK (transfer_handling_loss_pct BETWEEN 0.000 AND 1.000),
    spoilage_grace_hours DECIMAL(5,2) NOT NULL DEFAULT 0 CHECK (spoilage_grace_hours >= 0),
    is_active BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_impact_param_version_vehicle UNIQUE (vehicle_category_id, version_tag)
);

CREATE UNIQUE INDEX uq_impact_param_active_per_vehicle
    ON impact_parameter_versions(vehicle_category_id)
    WHERE is_active = TRUE;

CREATE TABLE simulation_impact_logs (
    sim_log_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scenario_id UUID NOT NULL REFERENCES simulation_scenarios(scenario_id) ON DELETE CASCADE,
    param_version_id UUID NOT NULL REFERENCES impact_parameter_versions(param_version_id),
    routing_config_id UUID NOT NULL REFERENCES routing_config_versions(routing_config_id),
    ivv_model_version_id UUID NOT NULL REFERENCES ivv_model_versions(model_version_id),
    scenario_revision BIGINT NOT NULL CHECK (scenario_revision >= 0),
    request_snapshot_json JSONB NOT NULL,
    resolution_type VARCHAR(32) NOT NULL
        CHECK (resolution_type IN ('ALTERNATE_ROUTE', 'LOGISTIC_BRIDGE_REQUIRED', 'NO_SAFE_ROUTE')),
    baseline_travel_min DECIMAL(7,2) NOT NULL CHECK (baseline_travel_min > 0),
    mitigated_travel_min DECIMAL(7,2) NULL CHECK (mitigated_travel_min IS NULL OR mitigated_travel_min > 0),
    baseline_co2_kg DECIMAL(8,2) NOT NULL CHECK (baseline_co2_kg >= 0),
    mitigated_co2_kg DECIMAL(8,2) NULL CHECK (mitigated_co2_kg IS NULL OR mitigated_co2_kg >= 0),
    baseline_cargo_loss_usd DECIMAL(10,2) NOT NULL CHECK (baseline_cargo_loss_usd >= 0),
    mitigated_cargo_loss_usd DECIMAL(10,2) NULL CHECK (mitigated_cargo_loss_usd IS NULL OR mitigated_cargo_loss_usd >= 0),
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_simulation_impact_logs_scenario_log UNIQUE (scenario_id, sim_log_id),
    CONSTRAINT chk_simulation_impact_logs_no_safe_route CHECK (
        (
            resolution_type = 'NO_SAFE_ROUTE'
            AND mitigated_travel_min IS NULL
            AND mitigated_co2_kg IS NULL
            AND mitigated_cargo_loss_usd IS NULL
        )
        OR
        (
            resolution_type <> 'NO_SAFE_ROUTE'
            AND mitigated_travel_min IS NOT NULL
            AND mitigated_co2_kg IS NOT NULL
            AND mitigated_cargo_loss_usd IS NOT NULL
        )
    )
);

CREATE INDEX idx_simulation_impact_logs_scenario ON simulation_impact_logs(scenario_id, computed_at DESC);
