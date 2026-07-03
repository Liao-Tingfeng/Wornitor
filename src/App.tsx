import { useState, useEffect } from "react";
import "./App.css";
import { Timeline } from "./components/Timeline";
import { Report } from "./components/Report";
import { Settings } from "./components/Settings";
import { useScreenshotStore, setupEventListeners } from "./stores/useScreenshotStore";
import { useI18n } from "./i18n";
type Tab = "timeline" | "report" | "settings";

const TAB_KEYS: Record<Tab, string> = {
  timeline: "app.tabs.timeline",
  report: "app.tabs.report",
  settings: "app.tabs.settings",
};

function RecordingDot({ status }: { status: string }) {
  const dotClass =
    status === "recording"
      ? "dot-recording"
      : status === "paused"
        ? "dot-paused"
        : "dot-idle";
  return <span className={`recording-dot ${dotClass}`} />;
}

function App() {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<Tab>("timeline");
  const {
    recordingStatus,
    isRecording,
    refreshStatus,
  } = useScreenshotStore();
  const lastLlmCost = useScreenshotStore((s) => s.lastLlmCost);
  const segmentCount = useScreenshotStore((s) => s.todaySegments.length);

  useEffect(() => {
    refreshStatus();
    setupEventListeners();

    // Poll status every 10 seconds (兜底，确保即使事件丢失也能更新)
    const interval = setInterval(refreshStatus, 10000);
    return () => clearInterval(interval);
  }, [refreshStatus]);

  const statusLabel =
    recordingStatus === "recording"
      ? t('app.status.recording')
      : recordingStatus === "paused"
        ? t('app.status.paused')
        : t('app.status.idle');

  return (
    <div className="app">
      {/* Top status bar */}
      <header className="app-header">
        <div className="header-left">
          <RecordingDot status={recordingStatus} />
          <span className={`status-text ${recordingStatus}`}>{statusLabel}</span>
          {isRecording && (
            <span className="segment-count-badge">
              {t('app.status.segments', { count: segmentCount })}
            </span>
          )}
          {lastLlmCost && (
            <span className="cost-badge" title={`${lastLlmCost.model} | ${lastLlmCost.provider}`}>
              💰 ¥{lastLlmCost.cost.toFixed(4)}
            </span>
          )}
        </div>

        <div className="header-center">
          <nav className="tab-nav">
            {(Object.entries(TAB_KEYS) as [Tab, string][]).map(
              ([key, tKey]) => (
                <button
                  key={key}
                  className={`tab-btn ${activeTab === key ? "active" : ""}`}
                  onClick={() => setActiveTab(key)}
                >
                  {key === "timeline" && "📋 "}
                  {key === "report" && "📊 "}
                  {key === "settings" && "⚙️ "}
                  {t(tKey)}
                </button>
              ),
            )}
          </nav>
        </div>

        <div className="header-right">
          <button
            className="icon-btn"
            onClick={() => setActiveTab("settings")}
            title={t('app.settings.title')}
          >
            ⚙️
          </button>
        </div>
      </header>

      {/* Page content */}
      <main className="app-main">
        {activeTab === "timeline" && <Timeline />}
        {activeTab === "report" && <Report />}
        {activeTab === "settings" && <Settings />}
      </main>
    </div>
  );
}

export default App;
