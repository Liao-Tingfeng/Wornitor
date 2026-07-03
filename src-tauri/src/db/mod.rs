pub mod migrations;
pub mod models;

use models::*;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;

// ── Error type ──────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

// ── Database ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    /// Open (or create) the database at `path`, enable WAL mode, and run migrations.
    pub fn new(path: &str) -> Result<Self, DbError> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(5)
            .build(manager)
            .map_err(|e| DbError::Migration(format!("Failed to create connection pool: {e}")))?;

        // Enable WAL + foreign keys on an initial connection
        {
            let conn = pool.get()?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            conn.execute_batch("PRAGMA foreign_keys=ON;")?;
            migrations::run_migrations(&conn)?;
        }

        Ok(Database { pool })
    }

    /// Helper: get a connection from the pool with a consistent error mapping.
    fn get_conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, DbError> {
        self.pool
            .get()
            .map_err(|e| DbError::Migration(format!("Failed to acquire connection: {e}")))
    }

    /// Remove records older than `retention_days`. Returns count of deleted rows.
    /// Also cleans up screenshot files from disk.
    pub fn clean_old_data(&self, retention_days: i64) -> Result<i64, DbError> {
        let conn = self.get_conn()?;

        let cutoff = format!("-{} days", retention_days);
        let deleted = conn.execute(
            "DELETE FROM screenshot_frames WHERE captured_at < datetime('now', ?1)",
            params![cutoff],
        )? + conn.execute(
            "DELETE FROM activity_segments WHERE end_time < datetime('now', ?1)",
            params![cutoff],
        )?;

        // Also clean screenshot files from disk
        let base_dir = crate::dirs_db_path()
            .map(std::path::PathBuf::from)
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        if let Ok(count) = crate::screen::clean_screenshot_files(&base_dir, retention_days as u32) {
            eprintln!("[DB] Cleaned {} screenshot files from disk", count);
        }

        Ok(deleted as i64)
    }

    /// Clear all data from every table.
    pub fn clear_all(&self) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute_batch(
            "DELETE FROM screenshot_frames;
             DELETE FROM activity_segments;
             DELETE FROM daily_summaries;
             DELETE FROM period_summaries;
             DELETE FROM llm_configs;
             DELETE FROM privacy_rules;",
        )?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════
    //  Screenshot Frames
    // ═══════════════════════════════════════════════════════════════

    pub fn insert_screenshot(&self, frame: &ScreenshotFrame) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO screenshot_frames
             (id, captured_at, file_path, file_size, width, height, phash, app_name, window_title, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                frame.id,
                frame.captured_at,
                frame.file_path,
                frame.file_size,
                frame.width,
                frame.height,
                frame.phash,
                frame.app_name,
                frame.window_title,
                frame.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_screenshot(&self, id: &str) -> Result<Option<ScreenshotFrame>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, captured_at, file_path, file_size, width, height, phash,
                    app_name, window_title, created_at
             FROM screenshot_frames WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], Self::map_screenshot)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            _ => Ok(None),
        }
    }

    pub fn get_screenshots_in_range(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<ScreenshotFrame>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, captured_at, file_path, file_size, width, height, phash,
                    app_name, window_title, created_at
             FROM screenshot_frames
             WHERE captured_at >= ?1 AND captured_at <= ?2
             ORDER BY captured_at ASC",
        )?;
        let rows = stmt.query_map(params![from, to], Self::map_screenshot)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    /// Fetch multiple screenshot frames by their IDs in one round-trip.
    pub fn get_screenshots_by_ids(&self, ids: &[String]) -> Result<Vec<ScreenshotFrame>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.get_conn()?;

        // Build placeholders: ?1, ?2, ?3, ...
        let placeholders: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT id, captured_at, file_path, file_size, width, height, phash,
                    app_name, window_title, created_at
             FROM screenshot_frames
             WHERE id IN ({})
             ORDER BY captured_at ASC",
            placeholders.join(", ")
        );

        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt.query_map(params.as_slice(), Self::map_screenshot)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    pub fn get_recent_screenshots(&self, limit: i64) -> Result<Vec<ScreenshotFrame>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, captured_at, file_path, file_size, width, height, phash,
                    app_name, window_title, created_at
             FROM screenshot_frames
             ORDER BY captured_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], Self::map_screenshot)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    fn map_screenshot(row: &rusqlite::Row) -> rusqlite::Result<ScreenshotFrame> {
        Ok(ScreenshotFrame {
            id: row.get("id")?,
            captured_at: row.get("captured_at")?,
            file_path: row.get("file_path")?,
            file_size: row.get("file_size")?,
            width: row.get("width")?,
            height: row.get("height")?,
            phash: row.get("phash")?,
            app_name: row.get("app_name")?,
            window_title: row.get("window_title")?,
            created_at: row.get("created_at")?,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    //  Activity Segments
    // ═══════════════════════════════════════════════════════════════

    pub fn insert_segment(&self, seg: &ActivitySegment) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO activity_segments
             (id, start_time, end_time, duration_secs, app_name, window_title,
              llm_summary, category, user_label, confidence, source_frame_ids, is_manual, created_at,
              llm_cost, llm_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                seg.id,
                seg.start_time,
                seg.end_time,
                seg.duration_secs,
                seg.app_name,
                seg.window_title,
                seg.llm_summary,
                seg.category,
                seg.user_label,
                seg.confidence,
                seg.source_frame_ids,
                seg.is_manual as i64,
                seg.created_at,
                seg.llm_cost,
                seg.llm_tokens,
            ],
        )?;
        Ok(())
    }

    pub fn update_segment(&self, seg: &ActivitySegment) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        let affected = conn.execute(
            "UPDATE activity_segments SET
                start_time = ?2, end_time = ?3, duration_secs = ?4,
                app_name = ?5, window_title = ?6, llm_summary = ?7,
                category = ?8, user_label = ?9, confidence = ?10,
                source_frame_ids = ?11, is_manual = ?12
             WHERE id = ?1",
            params![
                seg.id,
                seg.start_time,
                seg.end_time,
                seg.duration_secs,
                seg.app_name,
                seg.window_title,
                seg.llm_summary,
                seg.category,
                seg.user_label,
                seg.confidence,
                seg.source_frame_ids,
                seg.is_manual as i64,
            ],
        )?;
        if affected == 0 {
            return Err(DbError::NotFound(format!(
                "ActivitySegment {} not found",
                seg.id
            )));
        }
        Ok(())
    }

    /// Update only the llm_cost and llm_tokens fields on an existing segment.
    pub fn update_segment_cost(
        &self,
        segment_id: &str,
        cost: f64,
        tokens: i64,
    ) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "UPDATE activity_segments SET llm_cost = ?1, llm_tokens = ?2 WHERE id = ?3",
            params![cost, tokens, segment_id],
        )?;
        Ok(())
    }

    pub fn delete_segment(&self, id: &str) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        let affected = conn.execute("DELETE FROM activity_segments WHERE id = ?1", params![id])?;
        if affected == 0 {
            return Err(DbError::NotFound(format!("ActivitySegment {id} not found")));
        }
        Ok(())
    }

    pub fn get_segment_by_id(&self, id: &str) -> Result<Option<ActivitySegment>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, start_time, end_time, duration_secs, app_name, window_title,
                    llm_summary, category, user_label, confidence, source_frame_ids,
                    is_manual, created_at, llm_cost, llm_tokens
             FROM activity_segments WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], Self::map_segment)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            _ => Ok(None),
        }
    }

    pub fn get_segments_by_date(&self, date: &str) -> Result<Vec<ActivitySegment>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, start_time, end_time, duration_secs, app_name, window_title,
                    llm_summary, category, user_label, confidence, source_frame_ids,
                    is_manual, created_at, llm_cost, llm_tokens
             FROM activity_segments
             WHERE date(start_time) = ?1
             ORDER BY start_time ASC",
        )?;
        let rows = stmt.query_map(params![date], Self::map_segment)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    pub fn get_segments_in_range(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<ActivitySegment>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, start_time, end_time, duration_secs, app_name, window_title,
                    llm_summary, category, user_label, confidence, source_frame_ids,
                    is_manual, created_at, llm_cost, llm_tokens
             FROM activity_segments
             WHERE start_time >= ?1 AND start_time <= ?2
             ORDER BY start_time ASC",
        )?;
        let rows = stmt.query_map(params![from, to], Self::map_segment)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    /// Merge multiple segments into one: delete originals, insert the merged segment.
    pub fn merge_segments(
        &self,
        segment_ids: &[&str],
        merged: &ActivitySegment,
    ) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        let tx = conn.unchecked_transaction()?;

        // Delete originals
        for id in segment_ids {
            tx.execute("DELETE FROM activity_segments WHERE id = ?1", params![id])?;
        }

        // Insert merged
        tx.execute(
            "INSERT INTO activity_segments
             (id, start_time, end_time, duration_secs, app_name, window_title,
              llm_summary, category, user_label, confidence, source_frame_ids, is_manual, created_at,
              llm_cost, llm_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                merged.id,
                merged.start_time,
                merged.end_time,
                merged.duration_secs,
                merged.app_name,
                merged.window_title,
                merged.llm_summary,
                merged.category,
                merged.user_label,
                merged.confidence,
                merged.source_frame_ids,
                merged.is_manual as i64,
                merged.created_at,
                merged.llm_cost,
                merged.llm_tokens,
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn map_segment(row: &rusqlite::Row) -> rusqlite::Result<ActivitySegment> {
        Ok(ActivitySegment {
            id: row.get("id")?,
            start_time: row.get("start_time")?,
            end_time: row.get("end_time")?,
            duration_secs: row.get("duration_secs")?,
            app_name: row.get("app_name")?,
            window_title: row.get("window_title")?,
            llm_summary: row.get("llm_summary")?,
            category: row.get("category")?,
            user_label: row.get("user_label")?,
            confidence: row.get("confidence")?,
            source_frame_ids: row.get("source_frame_ids")?,
            is_manual: row.get::<_, i64>("is_manual")? != 0,
            created_at: row.get("created_at")?,
            llm_cost: row.get("llm_cost")?,
            llm_tokens: row.get("llm_tokens")?,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    //  Daily Summaries
    // ═══════════════════════════════════════════════════════════════

    pub fn upsert_daily_summary(&self, summary: &DailySummary) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO daily_summaries
             (id, date, total_seconds, segment_count, activity_breakdown,
              llm_summary, user_notes, report_html, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(date) DO UPDATE SET
                total_seconds       = excluded.total_seconds,
                segment_count       = excluded.segment_count,
                activity_breakdown  = excluded.activity_breakdown,
                llm_summary         = excluded.llm_summary,
                user_notes          = excluded.user_notes,
                report_html         = excluded.report_html,
                updated_at          = excluded.updated_at",
            params![
                summary.id,
                summary.date,
                summary.total_seconds,
                summary.segment_count,
                summary.activity_breakdown,
                summary.llm_summary,
                summary.user_notes,
                summary.report_html,
                summary.created_at,
                summary.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_daily_summary(&self, date: &str) -> Result<Option<DailySummary>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, date, total_seconds, segment_count, activity_breakdown,
                    llm_summary, user_notes, report_html, created_at, updated_at
             FROM daily_summaries WHERE date = ?1",
        )?;
        let mut rows = stmt.query_map(params![date], Self::map_daily_summary)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            _ => Ok(None),
        }
    }

    pub fn get_daily_summaries_in_range(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<DailySummary>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, date, total_seconds, segment_count, activity_breakdown,
                    llm_summary, user_notes, report_html, created_at, updated_at
             FROM daily_summaries
             WHERE date >= ?1 AND date <= ?2
             ORDER BY date ASC",
        )?;
        let rows = stmt.query_map(params![from, to], Self::map_daily_summary)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    fn map_daily_summary(row: &rusqlite::Row) -> rusqlite::Result<DailySummary> {
        Ok(DailySummary {
            id: row.get("id")?,
            date: row.get("date")?,
            total_seconds: row.get("total_seconds")?,
            segment_count: row.get("segment_count")?,
            activity_breakdown: row.get("activity_breakdown")?,
            llm_summary: row.get("llm_summary")?,
            user_notes: row.get("user_notes")?,
            report_html: row.get("report_html")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    //  Period Summaries
    // ═══════════════════════════════════════════════════════════════

    pub fn insert_period_summary(&self, summary: &PeriodSummary) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO period_summaries
             (id, type, start_date, end_date, total_seconds, daily_trend,
              activity_breakdown, llm_summary, user_notes, report_html, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                summary.id,
                summary.r#type,
                summary.start_date,
                summary.end_date,
                summary.total_seconds,
                summary.daily_trend,
                summary.activity_breakdown,
                summary.llm_summary,
                summary.user_notes,
                summary.report_html,
                summary.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_period_summaries_by_type(
        &self,
        r#type: &str,
    ) -> Result<Vec<PeriodSummary>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, type, start_date, end_date, total_seconds, daily_trend,
                    activity_breakdown, llm_summary, user_notes, report_html, created_at
             FROM period_summaries
             WHERE type = ?1
             ORDER BY start_date DESC",
        )?;
        let rows = stmt.query_map(params![r#type], Self::map_period_summary)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    fn map_period_summary(row: &rusqlite::Row) -> rusqlite::Result<PeriodSummary> {
        Ok(PeriodSummary {
            id: row.get("id")?,
            r#type: row.get("type")?,
            start_date: row.get("start_date")?,
            end_date: row.get("end_date")?,
            total_seconds: row.get("total_seconds")?,
            daily_trend: row.get("daily_trend")?,
            activity_breakdown: row.get("activity_breakdown")?,
            llm_summary: row.get("llm_summary")?,
            user_notes: row.get("user_notes")?,
            report_html: row.get("report_html")?,
            created_at: row.get("created_at")?,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    //  LLM Configs
    // ═══════════════════════════════════════════════════════════════

    pub fn insert_llm_config(&self, config: &LlmConfig) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO llm_configs
             (name, provider, base_url, model, api_key, max_tokens, is_active, created_at, use_batch_api)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                config.name,
                config.provider,
                config.base_url,
                config.model,
                config.api_key,
                config.max_tokens,
                config.is_active as i64,
                config.created_at,
                config.use_batch_api,
            ],
        )?;
        Ok(())
    }

    /// Upsert an LLM config by name: insert if name does not exist, update if it does.
    pub fn upsert_llm_config(&self, config: &LlmConfig) -> Result<(), DbError> {
        let conn = self.get_conn()?;

        // 检查是否已存在同名的
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM llm_configs WHERE name = ?1",
                params![config.name],
                |row| row.get(0),
            )
            .ok();

        eprintln!(
            "[DB-DEBUG] upsert_llm_config: name={}, existing_id={:?}, is_active={}",
            config.name, existing, config.is_active
        );

        if let Some(id) = existing {
            // UPDATE
            eprintln!("[DB-DEBUG] UPDATE llm_configs SET ... WHERE id={}", id);
            conn.execute(
                "UPDATE llm_configs SET
                    provider     = ?2,
                    base_url     = ?3,
                    model        = ?4,
                    api_key      = ?5,
                    max_tokens   = ?6,
                    is_active    = ?7,
                    use_batch_api = ?8
                 WHERE name = ?1",
                params![
                    config.name,
                    config.provider,
                    config.base_url,
                    config.model,
                    config.api_key,
                    config.max_tokens,
                    config.is_active as i64,
                    config.use_batch_api,
                ],
            )?;
            eprintln!("[DB-DEBUG] UPDATE done, rows affected");
        } else {
            // INSERT
            eprintln!("[DB-DEBUG] INSERT INTO llm_configs ...");
            conn.execute(
                "INSERT INTO llm_configs
                 (name, provider, base_url, model, api_key, max_tokens, is_active, created_at, use_batch_api)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), ?8)",
                params![
                    config.name,
                    config.provider,
                    config.base_url,
                    config.model,
                    config.api_key,
                    config.max_tokens,
                    config.is_active as i64,
                    config.use_batch_api,
                ],
            )?;
            eprintln!("[DB-DEBUG] INSERT done");
        }

        // 验证
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM llm_configs WHERE is_active = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        eprintln!("[DB-DEBUG] Total active configs after upsert: {}", count);

        Ok(())
    }

    /// Delete an LLM config by name.
    pub fn delete_llm_config_by_name(&self, name: &str) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute("DELETE FROM llm_configs WHERE name = ?1", params![name])?;
        Ok(())
    }

    pub fn get_llm_configs(&self) -> Result<Vec<LlmConfig>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, provider, base_url, model, api_key,
                    max_tokens, is_active, created_at, use_batch_api
             FROM llm_configs
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], Self::map_llm_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    pub fn set_active_config(&self, id: i64) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        let tx = conn.unchecked_transaction()?;
        // Deactivate all
        tx.execute("UPDATE llm_configs SET is_active = 0", [])?;
        // Activate target
        let affected = tx.execute(
            "UPDATE llm_configs SET is_active = 1 WHERE id = ?1",
            params![id],
        )?;
        if affected == 0 {
            return Err(DbError::NotFound(format!("LlmConfig {id} not found")));
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_active_config(&self) -> Result<Option<LlmConfig>, DbError> {
        let conn = self.get_conn()?;
        // 首先尝试获取活跃(is_active=1)的配置
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, name, provider, base_url, model, api_key,
                    max_tokens, is_active, created_at, use_batch_api
             FROM llm_configs WHERE is_active = 1
             LIMIT 1",
        ) {
            if let Ok(mut rows) = stmt.query_map([], Self::map_llm_config) {
                if let Some(Ok(row)) = rows.next() {
                    return Ok(Some(row));
                }
            }
        }
        // 没有活跃配置时，回退到第一个可用配置（兼容升级前的存量数据）
        eprintln!("[DB] No active config, falling back to first available config");
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, name, provider, base_url, model, api_key,
                    max_tokens, is_active, created_at, use_batch_api
             FROM llm_configs
             LIMIT 1",
        ) {
            if let Ok(mut rows) = stmt.query_map([], Self::map_llm_config) {
                if let Some(Ok(row)) = rows.next() {
                    eprintln!(
                        "[DB] Fallback to: name={}, has_api_key={}",
                        row.name,
                        row.api_key.is_some()
                    );
                    return Ok(Some(row));
                }
            }
        }
        Ok(None)
    }

    fn map_llm_config(row: &rusqlite::Row) -> rusqlite::Result<LlmConfig> {
        Ok(LlmConfig {
            id: row.get("id")?,
            name: row.get("name")?,
            provider: row.get("provider")?,
            base_url: row.get("base_url")?,
            model: row.get("model")?,
            api_key: row.get("api_key")?,
            max_tokens: row.get("max_tokens")?,
            is_active: row.get::<_, i64>("is_active")? != 0,
            created_at: row.get("created_at")?,
            use_batch_api: row.get::<_, Option<i64>>("use_batch_api")?.map(|v| v != 0),
        })
    }

    // ═══════════════════════════════════════════════════════════════
    //  Privacy Rules
    // ═══════════════════════════════════════════════════════════════

    pub fn insert_privacy_rule(&self, rule: &PrivacyRule) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO privacy_rules (rule_type, pattern, is_active, blur_rect, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                rule.rule_type,
                rule.pattern,
                rule.is_active as i64,
                rule.blur_rect,
                rule.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_active_rules(&self, rule_type: &str) -> Result<Vec<PrivacyRule>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, rule_type, pattern, is_active, blur_rect, created_at
             FROM privacy_rules
             WHERE rule_type = ?1 AND is_active = 1
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![rule_type], Self::map_privacy_rule)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    pub fn toggle_rule(&self, id: i64, active: bool) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        let affected = conn.execute(
            "UPDATE privacy_rules SET is_active = ?1 WHERE id = ?2",
            params![active as i64, id],
        )?;
        if affected == 0 {
            return Err(DbError::NotFound(format!("PrivacyRule {id} not found")));
        }
        Ok(())
    }

    pub fn delete_rule(&self, id: i64) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        let affected = conn.execute("DELETE FROM privacy_rules WHERE id = ?1", params![id])?;
        if affected == 0 {
            return Err(DbError::NotFound(format!("PrivacyRule {id} not found")));
        }
        Ok(())
    }

    fn map_privacy_rule(row: &rusqlite::Row) -> rusqlite::Result<PrivacyRule> {
        Ok(PrivacyRule {
            id: row.get("id")?,
            rule_type: row.get("rule_type")?,
            pattern: row.get("pattern")?,
            is_active: row.get::<_, i64>("is_active")? != 0,
            blur_rect: row.get("blur_rect")?,
            created_at: row.get("created_at")?,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    //  LLM Usage Logs
    // ═══════════════════════════════════════════════════════════════

    pub fn insert_llm_usage_log(&self, log: &LlmUsageLog) -> Result<(), DbError> {
        let conn = self.get_conn()?;
        conn.execute(
            "INSERT INTO llm_usage_logs
             (model, provider, prompt_tokens, completion_tokens, total_tokens, estimated_cost, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                log.model,
                log.provider,
                log.prompt_tokens,
                log.completion_tokens,
                log.total_tokens,
                log.estimated_cost,
                log.created_at,
            ],
        )?;
        Ok(())
    }

    /// Get aggregated usage summary for a given date (format: 'YYYY-MM-DD').
    pub fn get_daily_usage_summary(&self, date: &str) -> Result<UsageSummary, DbError> {
        let conn = self.get_conn()?;
        let summary = conn.query_row(
            "SELECT
                 COALESCE(SUM(prompt_tokens), 0)     AS total_prompt_tokens,
                 COALESCE(SUM(completion_tokens), 0) AS total_completion_tokens,
                 COALESCE(SUM(total_tokens), 0)      AS total_tokens,
                 COALESCE(SUM(estimated_cost), 0.0)  AS total_cost,
                 COUNT(*)                             AS call_count
             FROM llm_usage_logs
             WHERE date(created_at) = ?1",
            params![date],
            |row| {
                Ok(UsageSummary {
                    total_prompt_tokens: row.get("total_prompt_tokens")?,
                    total_completion_tokens: row.get("total_completion_tokens")?,
                    total_tokens: row.get("total_tokens")?,
                    total_cost: row.get("total_cost")?,
                    call_count: row.get("call_count")?,
                })
            },
        )?;
        Ok(summary)
    }

    /// Get aggregated usage summary for a given month.
    pub fn get_monthly_usage_summary(
        &self,
        year: i32,
        month: u32,
    ) -> Result<UsageSummary, DbError> {
        let conn = self.get_conn()?;
        let date_prefix = format!("{year}-{month:02}");
        let summary = conn.query_row(
            "SELECT
                 COALESCE(SUM(prompt_tokens), 0)     AS total_prompt_tokens,
                 COALESCE(SUM(completion_tokens), 0) AS total_completion_tokens,
                 COALESCE(SUM(total_tokens), 0)      AS total_tokens,
                 COALESCE(SUM(estimated_cost), 0.0)  AS total_cost,
                 COUNT(*)                             AS call_count
             FROM llm_usage_logs
             WHERE created_at LIKE ?1 || '%'",
            params![date_prefix],
            |row| {
                Ok(UsageSummary {
                    total_prompt_tokens: row.get("total_prompt_tokens")?,
                    total_completion_tokens: row.get("total_completion_tokens")?,
                    total_tokens: row.get("total_tokens")?,
                    total_cost: row.get("total_cost")?,
                    call_count: row.get("call_count")?,
                })
            },
        )?;
        Ok(summary)
    }

    // ═══════════════════════════════════════════════════════════════
    //  Aggregation queries
    // ═══════════════════════════════════════════════════════════════

    /// Returns `(category, total_seconds)` for segments in the given time range.
    pub fn get_category_breakdown(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, i64)>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT category, SUM(duration_secs) AS total
             FROM activity_segments
             WHERE start_time >= ?1 AND start_time <= ?2
             GROUP BY category
             ORDER BY total DESC",
        )?;
        let rows = stmt.query_map(params![from, to], |row| {
            Ok((
                row.get::<_, String>("category")?,
                row.get::<_, i64>("total")?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    /// Returns `(date, total_seconds)` for segments in the given time range.
    pub fn get_daily_trend(&self, from: &str, to: &str) -> Result<Vec<(String, i64)>, DbError> {
        let conn = self.get_conn()?;
        let mut stmt = conn.prepare(
            "SELECT date(start_time) AS day, SUM(duration_secs) AS total
             FROM activity_segments
             WHERE start_time >= ?1 AND start_time <= ?2
             GROUP BY day
             ORDER BY day ASC",
        )?;
        let rows = stmt.query_map(params![from, to], |row| {
            Ok((row.get::<_, String>("day")?, row.get::<_, i64>("total")?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }
}
