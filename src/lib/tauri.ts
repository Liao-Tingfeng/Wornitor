import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  ActivitySegment,
  RecordingStatus,
  DailySummary,
  ReportData,
  AppConfig,
  LlmConfig,
  PrivacyRule,
  ScreenshotThumbnail,
  UsageSummary,
} from "../types";

// ===== Timeout utility =====

export async function invokeWithTimeout<T>(
  cmd: string,
  args: Record<string, unknown>,
  timeoutMs = 30000
): Promise<T> {
  const result = await Promise.race([
    invoke(cmd, args) as Promise<T>,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(`Command ${cmd} timed out after ${timeoutMs}ms`)), timeoutMs)
    ),
  ]);
  return result;
}

// ===== Typed API wrapper =====

export const api = {
  // Recording control
  startRecording: () => invoke<void>("start_recording"),
  stopRecording: () => invoke<void>("stop_recording"),
  pauseRecording: () => invoke<void>("pause_recording"),
  resumeRecording: () => invoke<void>("resume_recording"),
  getRecordingStatus: () => invoke<RecordingStatus>("get_recording_status"),

  // Timeline / Segments
  getTodayTimeline: () => invoke<ActivitySegment[]>("get_today_timeline"),
  updateSegment: (segment: ActivitySegment) =>
    invoke<void>("update_segment", { segment }),
  deleteSegment: (segmentId: string) =>
    invoke<void>("delete_segment", { segmentId }),
  mergeSegments: (segmentIds: string[], mergedLabel: string) =>
    invoke<void>("merge_segments", { segmentIds, mergedLabel }),
  addManualSegment: (segment: ActivitySegment) =>
    invoke<void>("add_manual_segment", { segment }),

  // Reports (with timeout protection)
  generateDailyReport: (date: string) =>
    invokeWithTimeout<string>("generate_daily_report", { date }, 45000),
  generateWeeklyReport: (endDate: string) =>
    invokeWithTimeout<string>("generate_weekly_report", { endDate }, 45000),
  generateMonthlyReport: (year: number, month: number) =>
    invokeWithTimeout<string>("generate_monthly_report", { year, month }, 45000),
  getDailySummary: (date: string) =>
    invoke<DailySummary | null>("get_daily_summary", { date }),

  // Config
  setLocale: (locale: string) => invoke<void>("set_locale", { locale }),
  getConfig: () => invoke<AppConfig>("get_config"),
  updateConfig: (config: Record<string, unknown>) =>
    invoke<void>("update_config", { ...config }),
  getLlmConfigPresets: () => invoke<LlmConfig[]>("get_llm_config_presets"),
  saveLlmConfig: (config: LlmConfig) =>
    invoke<void>("save_llm_config", { config }),
  deleteLlmConfig: (name: string) =>
    invoke<void>("delete_llm_config", { name }),
  testLlmConnection: (config: LlmConfig) =>
    invoke<boolean>("test_llm_connection", { config }),

  // Privacy rules
  getActivePrivacyRules: (ruleType: string) =>
    invoke<PrivacyRule[]>("get_active_privacy_rules", { rule_type: ruleType }),

  // DB queries
  getSegmentsByDate: (date: string) =>
    invoke<ActivitySegment[]>("get_segments_by_date", { date }),
  getSegmentsInRange: (from: string, to: string) =>
    invoke<ActivitySegment[]>("get_segments_in_range", { from, to }),
  getCategoryBreakdown: (from: string, to: string) =>
    invoke<[string, number][]>("get_category_breakdown", { from, to }),
  getDailyTrend: (from: string, to: string) =>
    invoke<[string, number][]>("get_daily_trend", { from, to }),
  listLlmConfigs: () => invoke<LlmConfig[]>("list_llm_configs"),
  getActiveLlmConfig: () => invoke<LlmConfig | null>("get_active_llm_config"),

  // Screenshot
  takeScreenshot: () => invoke<void>("take_screenshot"),
  getScreenshotsByIds: (ids: string[]) =>
    invoke<ScreenshotThumbnail[]>("get_screenshots_by_ids", { ids }),

  // Analysis
  triggerAnalysis: () => invoke<void>("trigger_analysis"),
  getAnalysisPrompt: () => invoke<string>("get_analysis_prompt"),

  // LLM Usage / Cost
  getDailyUsage: (date: string) =>
    invoke<UsageSummary>("get_daily_usage", { date }),
  getMonthlyUsage: (year: number, month: number) =>
    invoke<UsageSummary>("get_monthly_usage", { year, month }),

  // New report commands: data loads instantly, summary triggers LLM
  getReportData: (type: string, params: { date?: string; endDate?: string; year?: number; month?: number }) =>
    invokeWithTimeout<ReportData>("get_report_data", { reportType: type, ...params }),
  generateReportSummary: (type: string, params: { date?: string; endDate?: string; year?: number; month?: number }) =>
    invokeWithTimeout<string>("generate_report_summary", { reportType: type, ...params }, 45000),
};

// ===== Event listeners =====

export function onScreenshotCaptured(
  callback: (payload: { path: string; timestamp: number }) => void,
): Promise<UnlistenFn> {
  return listen<{ path: string; timestamp: number }>(
    "screenshot-captured",
    (event) => {
      callback(event.payload);
    },
  );
}

export function onCaptureError(
  callback: (payload: { message: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ message: string }>("capture-error", (event) => {
    callback(event.payload);
  });
}

export function onAnalysisComplete(
  callback: (payload: { reportId: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ reportId: string }>("analysis-complete", (event) => {
    callback(event.payload);
  });
}

export function onCaptureStatusChanged(
  callback: (payload: { running: boolean }) => void,
): Promise<UnlistenFn> {
  return listen<{ running: boolean }>("capture-status-changed", (event) => {
    callback(event.payload);
  });
}

export function onTrayPauseChanged(
  callback: (paused: boolean) => void,
): Promise<UnlistenFn> {
  return listen<boolean>("tray-pause-changed", (event) => {
    callback(event.payload);
  });
}

export function onAnalysisStatus(
  callback: (payload: Record<string, unknown>) => void,
): Promise<UnlistenFn> {
  return listen<Record<string, unknown>>("analysis:status", (event) => {
    callback(event.payload);
  });
}

export function onAnalysisNewSegment(
  callback: (segment: Record<string, unknown>) => void,
): Promise<UnlistenFn> {
  return listen<Record<string, unknown>>("analysis:new-segment", (event) => {
    callback(event.payload);
  });
}
