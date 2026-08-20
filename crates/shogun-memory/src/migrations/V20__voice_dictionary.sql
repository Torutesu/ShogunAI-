-- User-confirmed vocabulary for deterministic dictation cleanup and bounded ASR keyterm hints.
-- No automatic-learning provenance exists: every persistent term is explicitly user managed.
--
-- Rollback: DROP both tables, then remove version 20 from refinery_schema_history. See
-- docs/migrations/V20-rollback.md. This loses only user-managed vocabulary data.

CREATE TABLE voice_terms (
    id            INTEGER PRIMARY KEY,
    canonical     TEXT NOT NULL CHECK (length(trim(canonical)) BETWEEN 1 AND 120),
    locale        TEXT,
    scope         TEXT NOT NULL CHECK (scope IN ('global', 'bundle', 'surface')),
    scope_ref     TEXT,
    priority      INTEGER NOT NULL DEFAULT 0,
    enabled       INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    provenance    TEXT NOT NULL CHECK (provenance IN ('user')),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    CHECK (
        (scope = 'global' AND scope_ref IS NULL)
        OR (scope IN ('bundle', 'surface') AND scope_ref IS NOT NULL AND length(trim(scope_ref)) BETWEEN 1 AND 255)
    )
) STRICT;

CREATE TABLE voice_term_aliases (
    id               INTEGER PRIMARY KEY,
    term_id          INTEGER NOT NULL REFERENCES voice_terms(id) ON DELETE CASCADE,
    alias            TEXT NOT NULL CHECK (length(trim(alias)) BETWEEN 1 AND 120),
    alias_normalized TEXT NOT NULL,
    UNIQUE (term_id, alias_normalized)
) STRICT;

CREATE INDEX idx_voice_terms_eligible ON voice_terms (enabled, scope, scope_ref, locale, priority DESC);
CREATE INDEX idx_voice_term_aliases_lookup ON voice_term_aliases (alias_normalized);
