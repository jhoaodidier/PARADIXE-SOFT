-- ============================================================================
-- PARADIXE SOFT (TakumiSoft) — Semillas de Catalogos Base de Produccion
-- ============================================================================

-- 1. Coordinador unico de sincronizacion transaccional
INSERT INTO sync_control (singleton_id, last_seq)
VALUES (1, 0)
ON CONFLICT (singleton_id) DO NOTHING;

-- 2. Roles RBAC y pesos base de confianza comunitaria
INSERT INTO roles (code, base_trust_weight, has_authoritative_override)
VALUES
    ('DRIVER',          0.60, FALSE),
    ('HEALTH_OPERATOR', 0.70, FALSE),
    ('CIVIL_DEFENSE',   1.00, TRUE),
    ('COER_ADMIN',      1.00, TRUE),
    ('BOT_SERVICE',     0.10, FALSE)
ON CONFLICT (code) DO NOTHING;

-- 3. Catalogo vehicular multimodal andino-amazonico
INSERT INTO vehicle_categories (
    code, max_slope_pct, min_road_width_m, max_weight_tons, allows_dirt_track, priority_tier
)
VALUES
    ('AMBULANCE_4X4', 22.0, 2.50,  4.50, TRUE,  1),
    ('PICKUP_4X4',    24.0, 2.30,  3.50, TRUE,  2),
    ('LIGHT_TRUCK',   14.0, 3.20, 12.00, FALSE, 3),
    ('HEAVY_TRAILER', 10.0, 4.00, 36.00, FALSE, 4),
    ('MOTORCYCLE',    28.0, 1.00,  0.40, TRUE,  5)
ON CONFLICT (code) DO NOTHING;

-- 4. Version inicial de modelo IVV (parametros iniciales sujetos a calibracion empirica)
INSERT INTO ivv_model_versions (
    version_tag,
    w_precip, w_slope_ravine, w_soil, w_hotspot,
    precip_24h_cap_mm, precip_72h_cap_mm, slope_cap_pct, ravine_max_dist_m,
    amber_threshold, red_threshold, validity_hours, is_active
)
SELECT
    'ivv-v1.0-initial',
    0.350, 0.300, 0.200, 0.150,
    80.00, 180.00, 45.0, 500,
    40.00, 70.00, 3,
    NOT EXISTS (SELECT 1 FROM ivv_model_versions WHERE is_active = TRUE)
WHERE NOT EXISTS (
    SELECT 1 FROM ivv_model_versions WHERE version_tag = 'ivv-v1.0-initial'
);

-- 5. Version inicial de pesos del enrutador
INSERT INTO routing_config_versions (
    version_tag, alpha_ivv_weight, beta_suspected_penalty, is_active
)
SELECT
    'routing-v1.0',
    0.75, 0.50,
    NOT EXISTS (SELECT 1 FROM routing_config_versions WHERE is_active = TRUE)
WHERE NOT EXISTS (
    SELECT 1 FROM routing_config_versions WHERE version_tag = 'routing-v1.0'
);

-- 6. Parametros de impacto de referencia por categoria vehicular
INSERT INTO impact_parameter_versions (
    version_tag,
    vehicle_category_id,
    co2_idle_kg_per_hour,
    co2_drive_kg_per_km,
    agri_cargo_value_usd,
    perishability_loss_rate_per_hour,
    transfer_handling_loss_pct,
    spoilage_grace_hours,
    is_active
)
SELECT
    'impact-v1.0-ref',
    vc.category_id,
    v.co2_idle_kg_per_hour,
    v.co2_drive_kg_per_km,
    v.agri_cargo_value_usd,
    v.perishability_loss_rate_per_hour,
    v.transfer_handling_loss_pct,
    v.spoilage_grace_hours,
    NOT EXISTS (
        SELECT 1 FROM impact_parameter_versions ipv
        WHERE ipv.vehicle_category_id = vc.category_id AND ipv.is_active = TRUE
    )
FROM (
    VALUES
        ('AMBULANCE_4X4', 1.200, 0.330,    0.00, 0.0000, 0.000, 0.00),
        ('PICKUP_4X4',    1.000, 0.300, 2500.00, 0.0300, 0.015, 2.00),
        ('LIGHT_TRUCK',   1.800, 0.750, 4500.00, 0.0300, 0.015, 2.00),
        ('HEAVY_TRAILER', 3.500, 1.350, 9000.00, 0.0300, 0.015, 2.00),
        ('MOTORCYCLE',    0.150, 0.075,    0.00, 0.0000, 0.000, 0.00)
) AS v(
    code, co2_idle_kg_per_hour, co2_drive_kg_per_km,
    agri_cargo_value_usd, perishability_loss_rate_per_hour,
    transfer_handling_loss_pct, spoilage_grace_hours
)
JOIN vehicle_categories vc ON vc.code = v.code
ON CONFLICT (vehicle_category_id, version_tag) DO NOTHING;
