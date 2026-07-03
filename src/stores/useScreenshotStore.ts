import { create } from "zustand";
import type { ActivitySegment, ScreenshotThumbnail } from "../types";
import { TauriEvent } from "../types";
import { api } from "../lib/tauri";
import { listen } from "@tauri-apps/api/event";

import type { LlmCostEvent } from "../types";

export type RecordingStatusType = "idle" | "recording" | "paused" | "error";

interface ScreenshotStore {
  isRecording: boolean;
  isPaused: boolean;
  todaySegments: ActivitySegment[];
  recordingStatus: RecordingStatusType;
  error: string | null;
  lastLlmCost: LlmCostEvent | null;
  recentCostTime: number;

  startRecording: () => Promise<void>;
  stopRecording: () => Promise<void>;
  pauseRecording: () => Promise<void>;
  resumeRecording: () => Promise<void>;
  loadTodayTimeline: () => Promise<void>;
  updateSegment: (id: string, data: Partial<ActivitySegment>) => Promise<void>;
  deleteSegment: (id: string) => Promise<void>;
  mergeSegments: (ids: string[], label: string) => Promise<void>;
  addManualSegment: (segment: ActivitySegment) => Promise<void>;
  refreshStatus: () => Promise<void>;
}

export const useScreenshotStore = create<ScreenshotStore>((set, get) => ({
  isRecording: false,
  isPaused: false,
  todaySegments: [],
  recordingStatus: "idle",
  error: null,
  lastLlmCost: null,
  recentCostTime: 0,

  refreshStatus: async () => {
    try {
      const status = await api.getRecordingStatus();
      set({
        isRecording: status.is_recording,
        isPaused: status.is_paused,
        recordingStatus: status.is_recording
          ? status.is_paused
            ? "paused"
            : "recording"
          : "idle",
      });
    } catch (e) {
      console.error("Failed to refresh recording status", e);
    }
  },

  startRecording: async () => {
    try {
      await api.startRecording();
      set({
        isRecording: true,
        isPaused: false,
        recordingStatus: "recording",
        error: null,
      });
    } catch (e) {
      set({ error: String(e), recordingStatus: "error" });
    }
  },

  stopRecording: async () => {
    try {
      await api.stopRecording();
      set({
        isRecording: false,
        isPaused: false,
        recordingStatus: "idle",
        error: null,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  pauseRecording: async () => {
    try {
      await api.pauseRecording();
      set({ isPaused: true, recordingStatus: "paused" });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  resumeRecording: async () => {
    try {
      await api.resumeRecording();
      set({ isPaused: false, recordingStatus: "recording" });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  loadTodayTimeline: async () => {
    try {
      const segments = await api.getTodayTimeline();
      // Fetch screenshots for all segments
      const allFrameIds: string[] = [];
      for (const seg of segments) {
        if (seg.source_frame_ids) {
          // source_frame_ids can be a single UUID or comma-separated UUIDs (after merge)
          for (const id of seg.source_frame_ids.split(",")) {
            const trimmed = id.trim();
            if (trimmed && !allFrameIds.includes(trimmed)) {
              allFrameIds.push(trimmed);
            }
          }
        }
      }
      if (allFrameIds.length > 0) {
        const screenshots = await api.getScreenshotsByIds(allFrameIds);
        // Build lookup map: id -> ScreenshotThumbnail
        const ssMap = new Map<string, ScreenshotThumbnail>();
        for (const ss of screenshots) {
          ssMap.set(ss.id, ss);
        }
        // Attach screenshots to each segment
        for (const seg of segments) {
          if (seg.source_frame_ids) {
            const ids = seg.source_frame_ids
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean);
            seg.screenshots = ids
              .map((id) => ssMap.get(id))
              .filter((s): s is ScreenshotThumbnail => s !== undefined);
          }
        }
      }
      set({ todaySegments: segments, error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  updateSegment: async (id, data) => {
    const segments = get().todaySegments;
    const existing = segments.find((s) => s.id === id);
    if (!existing) return;

    const updated: ActivitySegment = { ...existing, ...data };
    try {
      await api.updateSegment(updated);
      set({
        todaySegments: segments.map((s) => (s.id === id ? updated : s)),
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  deleteSegment: async (id) => {
    try {
      await api.deleteSegment(id);
      set({
        todaySegments: get().todaySegments.filter((s) => s.id !== id),
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  mergeSegments: async (ids, label) => {
    try {
      await api.mergeSegments(ids, label);
      await get().loadTodayTimeline();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  addManualSegment: async (segment) => {
    try {
      await api.addManualSegment(segment);
      await get().loadTodayTimeline();
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));

// ── Tauri 事件监听 ──────────────────────────────────────────
// 在后端完成截图→LLM分析→写DB后自动刷新前端时间线

let _unlistenCleanup: (() => void)[] = [];

export async function setupEventListeners() {
  // 清理旧监听（防止 HMR 重复注册）
  teardownEventListeners();

  const store = useScreenshotStore;

  const unlisten1 = await listen(TauriEvent.NEW_SEGMENT, () => {
    store.getState().loadTodayTimeline();
    store.getState().refreshStatus();
  });
  _unlistenCleanup.push(unlisten1);

  const unlisten2 = await listen(TauriEvent.UPDATE_SEGMENT, () => {
    store.getState().loadTodayTimeline();
    store.getState().refreshStatus();
  });
  _unlistenCleanup.push(unlisten2);

  const unlisten3 = await listen<{
    is_recording?: boolean;
    is_paused?: boolean;
    segment_count?: number;
    total_seconds?: number;
  }>(TauriEvent.STATUS, (event) => {
    const { is_recording, is_paused } = event.payload;
    store.setState({
      ...(is_recording !== undefined && { isRecording: is_recording }),
      ...(is_paused !== undefined && { isPaused: is_paused }),
    });
    store.getState().refreshStatus();
  });
  _unlistenCleanup.push(unlisten3);

  const unlisten4 = await listen<LlmCostEvent>(TauriEvent.LLM_COST, (event) => {
    console.log('[COST] received:', event.payload);
    store.setState({
      lastLlmCost: event.payload,
      recentCostTime: Date.now(),
    });
  });
  _unlistenCleanup.push(unlisten4);
}

export function teardownEventListeners() {
  _unlistenCleanup.forEach((fn) => fn());
  _unlistenCleanup = [];
}
