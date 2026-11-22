-- ============================================================================
-- PARADIXE SOFT (TakumiSoft) — Extension Multi-Regional y Linaje OSM/Hidrografia
-- Migracion aditiva e idempotente para escalar a todo Huanuco, sus provincias/
-- distritos y cualquier departamento adicional del Peru sin romper las 27 tablas base.
-- ============================================================================

CREATE TABLE IF NOT EXISTS territorial_units (
    unit_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ubigeo_code VARCHAR(16) NOT NULL UNIQUE,
    iso_code VARCHAR(16) NULL,
    parent_unit_id UUID NULL REFERENCES territorial_units(unit_id),
    name VARCHAR(120) NOT NULL,
    admin_level SMALLINT NOT NULL CHECK (admin_level IN (4, 6, 8)),
    bbox_geom GEOMETRY(POLYGON, 4326) NOT NULL,
    centroid_geom GEOMETRY(POINT, 4326) NOT NULL,
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    pmtiles_uri VARCHAR(512) NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_territorial_units_bbox
    ON territorial_units USING GIST (bbox_geom);
CREATE INDEX IF NOT EXISTS idx_territorial_units_centroid_geog
    ON territorial_units USING GIST ((centroid_geom::geography));
CREATE INDEX IF NOT EXISTS idx_territorial_units_parent
    ON territorial_units(parent_unit_id, admin_level);

CREATE TABLE IF NOT EXISTS hydrography_waterways (
    waterway_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    external_source_id VARCHAR(64) NOT NULL UNIQUE,
    waterway_type VARCHAR(24) NOT NULL
        CHECK (waterway_type IN ('river', 'stream', 'ravine', 'canal')),
    name VARCHAR(120) NULL,
    geom GEOMETRY(LINESTRING, 4326) NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hydrography_waterways_geom
    ON hydrography_waterways USING GIST (geom);
CREATE INDEX IF NOT EXISTS idx_hydrography_waterways_geog
    ON hydrography_waterways USING GIST ((geom::geography));

ALTER TABLE road_nodes
    ADD COLUMN IF NOT EXISTS external_source_id VARCHAR(64);
CREATE UNIQUE INDEX IF NOT EXISTS uq_road_nodes_external_source
    ON road_nodes(external_source_id)
    WHERE external_source_id IS NOT NULL;

ALTER TABLE road_segments
    ADD COLUMN IF NOT EXISTS external_source_id VARCHAR(128);
CREATE UNIQUE INDEX IF NOT EXISTS uq_road_segments_external_source
    ON road_segments(external_source_id)
    WHERE external_source_id IS NOT NULL;

ALTER TABLE road_segments
    ADD COLUMN IF NOT EXISTS territorial_unit_id UUID NULL REFERENCES territorial_units(unit_id);
CREATE INDEX IF NOT EXISTS idx_road_segments_territorial_unit
    ON road_segments(territorial_unit_id);
