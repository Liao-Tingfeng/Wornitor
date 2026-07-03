-- Migration v4: LLM usage tracking for cost estimation

CREATE TABLE IF NOT EXISTS llm_usage_logs (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    model             TEXT NOT NULL,
    provider          TEXT NOT NULL,
    prompt_tokens     INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,
    estimated_cost    REAL NOT NULL DEFAULT 0.0,
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for daily/monthly aggregation queries
CREATE INDEX IF NOT EXISTS idx_llm_usage_created_at ON llm_usage_logs(created_at);
CREATE INDEX IF NOT EXISTS idx_llm_usage_model ON llm_usage_logs(model);
