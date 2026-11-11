-- LOLA OS migration 0002 — omni sessions (ADR-017, ADR-021, ADR-022).
--
-- Omni-capable sessions need: frozen memory snapshots at session start
-- (cache-stability law), media references on events (audio/video/image),
-- and compaction witness ids (Hermes absorbed-message pattern).

ALTER TABLE sessions ADD COLUMN memory_snapshot_hash TEXT;

ALTER TABLE session_events ADD COLUMN media_uri TEXT;
ALTER TABLE session_events ADD COLUMN absorbed_ids_json TEXT;

-- The session event tree (Pi model) walks parent links; index them.
CREATE INDEX idx_events_parent ON session_events(parent_event_id);
