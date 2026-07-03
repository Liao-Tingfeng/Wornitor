import { create } from "zustand";
import type { ReportData } from "../types";
import { api } from "../lib/tauri";

export type ReportView = "daily" | "weekly" | "monthly";

interface ReportStore {
  currentView: ReportView;
  // 数据部分（秒级加载，不调 LLM）
  reportData: ReportData | null;
  dataLoading: boolean;
  // AI 总结部分（用户点击后加载）
  aiSummary: string | null;
  summaryLoading: boolean;
  summaryError: string | null;
  // 已有的总结（来自 has_existing_summary）
  existingSummary: string | null;

  setView: (view: ReportView) => void;
  loadReportData: (view: ReportView, params: { date?: string; endDate?: string; year?: number; month?: number }) => Promise<void>;
  generateSummary: (view: ReportView, params: { date?: string; endDate?: string; year?: number; month?: number }) => Promise<void>;
  clearSummary: () => void;
}

export const useReportStore = create<ReportStore>((set) => ({
  currentView: "daily",
  reportData: null,
  dataLoading: false,
  aiSummary: null,
  summaryLoading: false,
  summaryError: null,
  existingSummary: null,

  setView: (view) => set({ currentView: view }),

  loadReportData: async (view, params) => {
    set({ dataLoading: true, reportData: null, existingSummary: null, aiSummary: null, summaryError: null, summaryLoading: false });
    try {
      const data = await api.getReportData(view, params);
      set({
        reportData: data,
        dataLoading: false,
        existingSummary: data.has_existing_summary ? data.markdown : null,
      });
    } catch (e) {
      set({ dataLoading: false, summaryError: String(e) });
    }
  },

  generateSummary: async (view, params) => {
    set({ summaryLoading: true, summaryError: null });
    try {
      const summary = await api.generateReportSummary(view, params);
      set({ aiSummary: summary, existingSummary: summary, summaryLoading: false });
    } catch (e) {
      set({ summaryError: String(e), summaryLoading: false });
    }
  },

  clearSummary: () => set({ aiSummary: null, existingSummary: null }),
}));
