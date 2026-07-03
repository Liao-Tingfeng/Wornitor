-- Add cost and tokens fields to activity_segments
ALTER TABLE activity_segments ADD COLUMN llm_cost REAL DEFAULT 0;
ALTER TABLE activity_segments ADD COLUMN llm_tokens INTEGER DEFAULT 0;
