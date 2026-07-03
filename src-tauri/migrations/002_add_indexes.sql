CREATE INDEX IF NOT EXISTS idx_screenshots_captured_at ON screenshot_frames(captured_at);
CREATE INDEX IF NOT EXISTS idx_segments_start_time ON activity_segments(start_time);
CREATE INDEX IF NOT EXISTS idx_segments_end_time ON activity_segments(end_time);
CREATE INDEX IF NOT EXISTS idx_daily_summaries_date ON daily_summaries(date);
CREATE INDEX IF NOT EXISTS idx_period_summaries_type ON period_summaries(type);

CREATE INDEX IF NOT EXISTS idx_segments_category ON activity_segments(category);
CREATE INDEX IF NOT EXISTS idx_segments_app_name ON activity_segments(app_name);
