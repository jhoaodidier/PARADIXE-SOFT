-- ============================================================================
-- PARADIXE SOFT (TakumiSoft) — Migracion 0005: Pronosticos IVV 24/48/72h y Alertas de Flota
-- ============================================================================

CREATE TABLE IF NOT EXISTS corridor_alert_subscriptions (
    subscription_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    label VARCHAR(120) NOT NULL,
    region_iso VARCHAR(16) NULL,
    corridor_name VARCHAR(100) NULL,
    min_risk_level VARCHAR(16) NOT NULL CHECK (min_risk_level IN ('AMBER', 'RED', 'BLOCKED')),
    channel VARCHAR(20) NOT NULL CHECK (channel IN ('WHATSAPP_BOT', 'SMS')),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_corridor_alert_subs_user
    ON corridor_alert_subscriptions(user_id, is_active);

CREATE INDEX IF NOT EXISTS idx_corridor_alert_subs_active
    ON corridor_alert_subscriptions(is_active, region_iso);

CREATE TABLE IF NOT EXISTS weather_forecasts (
    forecast_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    segment_id UUID NOT NULL REFERENCES road_segments(segment_id) ON DELETE CASCADE,
    horizon_hours SMALLINT NOT NULL CHECK (horizon_hours IN (24, 48, 72)),
    precip_24h_mm DECIMAL(6,2) NOT NULL CHECK (precip_24h_mm >= 0),
    precip_72h_mm DECIMAL(6,2) NOT NULL CHECK (precip_72h_mm >= precip_24h_mm),
    predicted_ivv_score DECIMAL(5,2) NOT NULL CHECK (predicted_ivv_score BETWEEN 0.00 AND 100.00),
    predicted_risk_level VARCHAR(16) NOT NULL CHECK (predicted_risk_level IN ('GREEN', 'AMBER', 'RED')),
    source_name VARCHAR(32) NOT NULL DEFAULT 'OPEN_METEO_FORECAST',
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (segment_id, horizon_hours)
);

CREATE INDEX IF NOT EXISTS idx_weather_forecasts_horizon_risk
    ON weather_forecasts(horizon_hours, predicted_risk_level);

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'paradixe_sim_ro') THEN
        EXECUTE 'GRANT SELECT ON corridor_alert_subscriptions, weather_forecasts TO paradixe_sim_ro';
    END IF;
END $$;
