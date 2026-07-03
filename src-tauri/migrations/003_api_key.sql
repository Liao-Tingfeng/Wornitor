-- Migrate llm_configs from keychain-based api_key_ref to direct api_key storage.
-- 1. Add new api_key column
-- 2. Copy any existing values (api_key_ref was used as a keychain account name, not the key itself,
--    so existing data won't have real keys — this is just for schema compatibility)
-- 3. Drop the old api_key_ref column

ALTER TABLE llm_configs ADD COLUMN api_key TEXT;

-- Copy api_key_ref into api_key (if any) — api_key_ref was a keychain reference,
-- so it won't be a real API key, but we preserve it for forward compatibility.
UPDATE llm_configs SET api_key = api_key_ref;

-- Drop the old column (SQLite requires recreating the table to drop a column)
-- For maximum compatibility, we use the standard approach: create new, copy, drop old.
CREATE TABLE llm_configs_new (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    provider        TEXT NOT NULL,
    base_url        TEXT NOT NULL,
    model           TEXT NOT NULL,
    api_key         TEXT,
    max_tokens      INTEGER DEFAULT 1024,
    temperature     REAL DEFAULT 0.3,
    is_active       INTEGER DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO llm_configs_new
    (id, name, provider, base_url, model, api_key, max_tokens, temperature, is_active, created_at)
SELECT
    id, name, provider, base_url, model, api_key, max_tokens, temperature, is_active, created_at
FROM llm_configs;

DROP TABLE llm_configs;

ALTER TABLE llm_configs_new RENAME TO llm_configs;
