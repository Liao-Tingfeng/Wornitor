// ===== Activity / Timeline Types =====

export interface ScreenshotThumbnail {
  id: string;
  file_path: string;
  captured_at: string;
}

export interface ActivitySegment {
  id: string;
  start_time: string;
  end_time: string;
  duration_secs: number;
  app_name: string | null;
  window_title: string | null;
  llm_summary: string | null;
  category: string;
  user_label: string | null;
  confidence: number;
  is_manual: boolean;
  source_frame_ids: string | null;
  llm_cost?: number;
  llm_tokens?: number;
  /** Parsed screenshot thumbnails, populated on frontend from source_frame_ids */
  screenshots?: ScreenshotThumbnail[];
}

export interface RecordingStatus {
  is_recording: boolean;
  is_paused: boolean;
  segment_count: number;
  total_seconds: number;
}

// ===== Report Types =====

export interface DailySummary {
  id: string;
  date: string;
  total_seconds: number;
  segment_count: number;
  activity_breakdown: Record<string, number> | null;
  llm_summary: string | null;
  user_notes: string | null;
  report_html: string | null;
}

export interface PeriodSummary {
  id: string;
  type: 'week' | 'month' | 'custom';
  start_date: string;
  end_date: string;
  total_seconds: number;
  daily_trend: Record<string, number> | null;
  activity_breakdown: Record<string, number> | null;
  llm_summary: string | null;
  report_html: string | null;
}

// ===== LLM Usage Summary =====

export interface UsageSummary {
  total_prompt_tokens: number;
  total_completion_tokens: number;
  total_tokens: number;
  total_cost: number;  // 元
  call_count: number;
}

// ===== LLM Cost Event =====
export interface LlmCostEvent {
  model: string;
  provider: string;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  cost: number;
  batch_size?: number;
  timestamp: string;
}

// ===== Config Types =====

export interface ScreenshotConfig {
  interval_secs: number;
  analysis_interval_secs: number;
  jpeg_quality: number;
  max_width: number;
  include_cursor: boolean;
  dedup_threshold: number;
  retention_days: number;
  capture_all_displays?: boolean;
}

export interface LlmConfig {
  id?: number;
  name: string;
  provider: string;
  base_url: string;
  model: string;
  api_key?: string;
  max_tokens: number;
  is_active: boolean;
  use_batch_api?: boolean;
}

export interface PrivacyRule {
  id?: number;
  rule_type: string;
  pattern: string;
  is_active: boolean;
  blur_rect?: string;
}

export interface AppConfig {
  screenshot: ScreenshotConfig;
  llm: LlmConfig;
  privacy: {
    blur_sensitive: boolean;
    blocked_apps: string[];
    idle_pause_minutes: number;
  };
}

// ===== ReportData (from new backend commands, no LLM) =====

export interface ReportData {
  title: string;
  date_label: string;
  total_seconds: number;
  segments: ActivitySegment[];
  breakdown: [string, number][];
  html: string;
  markdown: string;
  has_existing_summary: boolean;
}

// ===== Legacy / Screenshot Types (preserved) =====

export interface Screenshot {
  id: string;
  path: string;
  thumbnail: string;
  timestamp: number;
  application?: string;
  windowTitle?: string;
}

export interface ScreenshotBatch {
  screenshots: Screenshot[];
  startTime: number;
  endTime: number;
}

export interface WorkReport {
  id: string;
  date: string;
  summary: string;
  tasks: TaskItem[];
  metrics: WorkMetrics;
  screenshots: Screenshot[];
  rawLlmOutput?: string;
}

export interface TaskItem {
  title: string;
  duration: number;
  category: string;
  description: string;
}

export interface WorkMetrics {
  totalActiveMinutes: number;
  appDistribution: Record<string, number>;
  categoryBreakdown: Record<string, number>;
}

export interface TimelineEntry {
  time: number;
  application: string;
  windowTitle: string;
  duration: number;
}

export interface AnalysisResult {
  screenshots: Screenshot[];
  timeline: TimelineEntry[];
  report: WorkReport;
}

export interface LLMRequest {
  model: string;
  messages: LLMMessage[];
  max_tokens?: number;
}

export interface LLMMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface LLMResponse {
  content: string;
  model: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

export interface TauriEventPayload {
  type: string;
  data: unknown;
}

export const TauriEvent = {
  NEW_SEGMENT: 'analysis:new-segment',
  UPDATE_SEGMENT: 'analysis:update-segment',
  STATUS: 'analysis:status',
  LLM_COST: 'analysis:llm-cost',
  BATCH_SUBMITTED: 'analysis:batch-submitted',
  BATCH_COMPLETED: 'analysis:batch-completed',
  BATCH_FAILED: 'analysis:batch-failed',
  ERROR: 'analysis:error',
} as const;
export type TauriEventType = (typeof TauriEvent)[keyof typeof TauriEvent];
