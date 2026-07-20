-- Warm-layer embeddings (FR-MEM-01/03, ADR-001). A sqlite-vec vec0 virtual table holding one
-- 384-dim (e5-small) vector per event, addressed by rowid = event_log.id. Vector search is a
-- brute-force scan and is intended for the Warm set only (FR-MEM-03: "vector search targets
-- Warm because sqlite-vec is exhaustive"). Populated asynchronously off the write path
-- (FR-MEM-22); an event with no row here is still found via FTS.
--
-- Additive over V1 (FR-MEM-31): no existing column is touched.
CREATE VIRTUAL TABLE event_vec USING vec0(embedding float[384]);
