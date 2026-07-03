CREATE TABLE IF NOT EXISTS screenshot_frames (
    id              TEXT PRIMARY KEY,
    captured_at     TEXT NOT NULL,
    file_path       TEXT NOT NULL,
    file_size       INTEGER NOT NULL,
    width           INTEGER NOT NULL,
    height          INTEGER NOT NULL,
    phash           TEXT NOT NULL,
    app_name        TEXT,
    window_title    TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS activity_segments (
    id              TEXT PRIMARY KEY,
    start_time      TEXT NOT NULL,
    end_time        TEXT NOT NULL,
    duration_secs   INTEGER NOT NULL,
    app_name        TEXT,
    window_title    TEXT,
    llm_summary     TEXT,
    category        TEXT DEFAULT 'other',
    user_label      TEXT,
    confidence      REAL DEFAULT 0.0,
    source_frame_ids TEXT,
    is_manual       INTEGER DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS daily_summaries (
    id              TEXT PRIMARY KEY,
    date            TEXT NOT NULL UNIQUE,
    total_seconds   INTEGER NOT NULL DEFAULT 0,
    segment_count   INTEGER NOT NULL DEFAULT 0,
    activity_breakdown TEXT,
    llm_summary     TEXT,
    user_notes      TEXT,
    report_html     TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS period_summaries (
    id              TEXT PRIMARY KEY,
    type            TEXT NOT NULL,
    start_date      TEXT NOT NULL,
    end_date        TEXT NOT NULL,
    total_seconds   INTEGER NOT NULL DEFAULT 0,
    daily_trend     TEXT,
    activity_breakdown TEXT,
    llm_summary     TEXT,
    user_notes      TEXT,
    report_html     TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS llm_configs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    provider        TEXT NOT NULL,
    base_url        TEXT NOT NULL,
    model           TEXT NOT NULL,
    api_key_ref     TEXT,
    max_tokens      INTEGER DEFAULT 1024,
    temperature     REAL DEFAULT 0.3,
    is_active       INTEGER DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS privacy_rules (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_type       TEXT NOT NULL,
    pattern         TEXT NOT NULL,
    is_active       INTEGER DEFAULT 1,
    blur_rect       TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS _migrations (
    version         INTEGER PRIMARY KEY,
    applied_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
