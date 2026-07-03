import { create } from "zustand";
import type { ScreenshotConfig, LlmConfig, PrivacyRule } from "../types";
import { api } from "../lib/tauri";

interface ConfigStore {
  screenshotConfig: ScreenshotConfig;
  llmConfigs: LlmConfig[];
  activeLlmConfig: number | null;
  privacyRules: PrivacyRule[];
  isLoading: boolean;
  error: string | null;

  loadConfig: () => Promise<void>;
  updateScreenshotConfig: (config: Partial<ScreenshotConfig>) => Promise<void>;
  saveLlmConfig: (config: LlmConfig) => Promise<void>;
  deleteLlmConfig: (name: string) => Promise<void>;
  testLlmConnection: (config: LlmConfig) => Promise<boolean>;
  addPrivacyRule: (rule: Omit<PrivacyRule, "id">) => Promise<void>;
  deletePrivacyRule: (id: number) => Promise<void>;
}

const defaultScreenshotConfig: ScreenshotConfig = {
  interval_secs: 30,
  analysis_interval_secs: 300,
  jpeg_quality: 85,
  max_width: 1200,
  include_cursor: true,
  dedup_threshold: 5,
  retention_days: 30,
  capture_all_displays: false,
};

export const useConfigStore = create<ConfigStore>((set, get) => ({
  screenshotConfig: { ...defaultScreenshotConfig },
  llmConfigs: [],
  activeLlmConfig: null,
  privacyRules: [],
  isLoading: false,
  error: null,

  loadConfig: async () => {
    set({ isLoading: true, error: null });
    try {
      // Load from Tauri config command
      const config = await api.getConfig();
      const raw = config as unknown as Record<string, unknown>;
      set({
        screenshotConfig: {
          interval_secs: (raw.screenshot_interval ?? defaultScreenshotConfig.interval_secs) as number,
          analysis_interval_secs: (raw.analysis_interval_secs ?? defaultScreenshotConfig.analysis_interval_secs) as number,
          jpeg_quality: (raw.jpeg_quality ?? defaultScreenshotConfig.jpeg_quality) as number,
          max_width: (raw.max_width ?? defaultScreenshotConfig.max_width) as number,
          include_cursor: (raw.include_cursor ?? defaultScreenshotConfig.include_cursor) as boolean,
          dedup_threshold: (raw.dedup_threshold ?? defaultScreenshotConfig.dedup_threshold) as number,
          capture_all_displays: (raw.capture_all_displays ?? defaultScreenshotConfig.capture_all_displays) as boolean,
          retention_days: (raw.retention_days ?? defaultScreenshotConfig.retention_days) as number,
        },
        isLoading: false,
      });

      // Load LLM configs
      try {
        const llmConfigs = await api.listLlmConfigs();
        const activeIdx = llmConfigs.findIndex((c) => c.is_active);
        set({
          llmConfigs,
          activeLlmConfig: activeIdx >= 0 ? activeIdx : null,
        });
      } catch {
        // LLM configs may not be in DB yet
      }

      // Load privacy rules (blocked apps)
      try {
        const rules = await api.getActivePrivacyRules("app_block");
        set({ privacyRules: rules });
      } catch {
        // rules may be empty
      }
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  updateScreenshotConfig: async (partial) => {
    try {
      const current = get().screenshotConfig;
      const updated = { ...current, ...partial };
      // Persist via update_config Tauri command
      // NOTE: Tauri v2 converts #[tauri::command] snake_case params to camelCase on JS side
      await api.updateConfig({
        screenshotInterval: updated.interval_secs,
        analysisIntervalSecs: updated.analysis_interval_secs,
        jpegQuality: updated.jpeg_quality,
        maxWidth: updated.max_width,
        includeCursor: updated.include_cursor,
        dedupThreshold: updated.dedup_threshold,
        captureAllDisplays: updated.capture_all_displays,
        retentionDays: updated.retention_days,
      });
      set({ screenshotConfig: updated });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveLlmConfig: async (config) => {
    try {
      await api.saveLlmConfig(config);
      const configs = await api.listLlmConfigs();
      const activeIdx = configs.findIndex((c) => c.is_active);
      set({ llmConfigs: configs, activeLlmConfig: activeIdx >= 0 ? activeIdx : null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  deleteLlmConfig: async (name) => {
    try {
      await api.deleteLlmConfig(name);
      const configs = await api.listLlmConfigs();
      set({ llmConfigs: configs });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  testLlmConnection: async (config) => {
    try {
      return await api.testLlmConnection(config);
    } catch (e) {
      set({ error: String(e) });
      return false;
    }
  },

  addPrivacyRule: async (rule) => {
    // TODO: Implement via Tauri command (not yet in Rust backend)
    set((state) => ({
      privacyRules: [
        ...state.privacyRules,
        { ...rule, id: Date.now() } as PrivacyRule,
      ],
    }));
  },

  deletePrivacyRule: async (id) => {
    // TODO: Implement via Tauri command
    set((state) => ({
      privacyRules: state.privacyRules.filter((r) => r.id !== id),
    }));
  },
}));
