-- Allow isolated contingency drill fixtures (stored in simulation_scenarios.fixture_payload_json)
-- to inject sim_events against synthetic fixture segment UUIDs without inserting synthetic rows
-- into operational road_segments. Application-level validation in paradixe-api enforces that
-- segment_id belongs to either the scenario fixture or road_segments.
ALTER TABLE sim_events
    DROP CONSTRAINT IF EXISTS sim_events_segment_id_fkey;
