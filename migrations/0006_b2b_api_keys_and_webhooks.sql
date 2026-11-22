-- ============================================================================
-- PARADIXE SOFT (TakumiSoft) — Plataforma B2B: API Keys y Webhooks Salientes
-- Migracion aditiva e idempotente para integracion de flotas, ERP/TMS y COER.
-- ============================================================================

CREATE TABLE IF NOT EXISTS b2b_api_keys (
    api_key_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_user_id UUID NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    organization_name VARCHAR(120) NOT NULL,
    key_prefix VARCHAR(24) NOT NULL,
    key_hash CHAR(64) NOT NULL UNIQUE,
    role_code VARCHAR(32) NOT NULL DEFAULT 'DRIVER'
        CHECK (role_code IN ('DRIVER', 'HEALTH_OPERATOR', 'CIVIL_DEFENSE')),
    rate_limit_rpm INTEGER NOT NULL DEFAULT 120 CHECK (rate_limit_rpm BETWEEN 10 AND 6000),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_used_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_b2b_api_keys_owner_active
    ON b2b_api_keys(owner_user_id, is_active);

CREATE TABLE IF NOT EXISTS b2b_webhook_endpoints (
    webhook_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_user_id UUID NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    organization_name VARCHAR(120) NOT NULL,
    target_url VARCHAR(512) NOT NULL,
    signing_secret VARCHAR(128) NOT NULL,
    region_iso VARCHAR(16) NULL,
    min_risk_level VARCHAR(16) NOT NULL DEFAULT 'RED'
        CHECK (min_risk_level IN ('AMBER', 'RED', 'BLOCKED')),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_delivery_status SMALLINT NULL,
    last_delivered_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_b2b_webhooks_active_risk
    ON b2b_webhook_endpoints(is_active, min_risk_level);
