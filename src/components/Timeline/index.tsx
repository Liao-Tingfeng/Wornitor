import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useScreenshotStore } from "../../stores/useScreenshotStore";
import type { ActivitySegment, ScreenshotThumbnail } from "../../types";
import { useI18n } from "../../i18n";

const CATEGORY_ICONS: Record<string, string> = {
  dev: "💻",
  meeting: "📋",
  communication: "💬",
  design: "🎨",
  documentation: "📝",
  browsing: "🌐",
  management: "📊",
  other: "📌",
};

function formatTime(iso: string): string {
  const m = iso.match(/(\d{2}):(\d{2})/);
  return m ? `${m[1]}:${m[2]}` : iso;
}

function formatDuration(secs: number, t: (key: string, params?: Record<string, string | number>) => string): string {
  if (secs < 60) return t('time.second', { s: secs });
  const m = Math.floor(secs / 60);
  if (m < 60) return t('time.minute', { m });
  const h = Math.floor(m / 60);
  return t('time.hourMinute', { h, m: m % 60 });
}

function getDateLabel(t: (key: string, params?: Record<string, string | number>) => string): string {
  const now = new Date();
  const dateStr = t('time.dateFormat', { year: now.getFullYear(), month: now.getMonth() + 1, day: now.getDate() });
  const dayStr = t(`weekday.${now.getDay()}`);
  return `${dateStr} · ${dayStr}`;
}


interface EditableSegmentProps {
  segment: ActivitySegment;
  onSave: (data: Partial<ActivitySegment>) => void;
  onCancel: () => void;
}

function EditableSegment({ segment, onSave, onCancel }: EditableSegmentProps) {
  const { t } = useI18n();
  const [label, setLabel] = useState(segment.user_label ?? segment.llm_summary ?? "");
  const [category, setCategory] = useState(segment.category);
  const [startTime, setStartTime] = useState(segment.start_time);
  const [endTime, setEndTime] = useState(segment.end_time);

  return (
    <div className="timeline-edit-form">
      <input
        type="text"
        value={label}
        onChange={(e) => setLabel(e.target.value)}
        placeholder={t('timeline.activity')}
        className="input"
      />
      <select value={category} onChange={(e) => setCategory(e.target.value)} className="select">
        <option value="dev">{t('category.dev')}</option>
        <option value="meeting">{t('category.meeting')}</option>
        <option value="communication">{t('category.communication')}</option>
        <option value="design">{t('category.design')}</option>
        <option value="documentation">{t('category.documentation')}</option>
        <option value="browsing">{t('category.browsing')}</option>
        <option value="management">{t('category.management')}</option>
        <option value="other">{t('category.other')}</option>
      </select>
      <div className="edit-time-row">
        <input
          type="time"
          value={startTime.slice(11, 16)}
          onChange={(e) =>
            setStartTime(segment.start_time.slice(0, 11) + e.target.value + ":00")
          }
          className="input time-input"
        />
        <span>→</span>
        <input
          type="time"
          value={endTime.slice(11, 16)}
          onChange={(e) =>
            setEndTime(segment.end_time.slice(0, 11) + e.target.value + ":00")
          }
          className="input time-input"
        />
      </div>
      <div className="edit-actions">
        <button
          className="btn btn-primary btn-sm"
          onClick={() =>
            onSave({
              user_label: label,
              category,
              start_time: startTime,
              end_time: endTime,
            })
          }
        >
          {t('timeline.save')}
        </button>
        <button className="btn btn-ghost btn-sm" onClick={onCancel}>
          {t('timeline.cancel')}
        </button>
      </div>
    </div>
  );
}

function ScreenshotThumb({ ss, onClick }: { ss: ScreenshotThumbnail; onClick?: () => void }) {
  const [failed, setFailed] = useState(false);

  if (failed) {
    return (
      <div className="screenshot-thumb screenshot-thumb-failed" onClick={onClick}>
        <span className="thumb-failed-icon">🖼️</span>
      </div>
    );
  }

  return (
    <img
      src={convertFileSrc(ss.file_path)}
      className="screenshot-thumb"
      alt={`screenshot at ${ss.captured_at}`}
      onClick={onClick}
      onError={() => setFailed(true)}
    />
  );
}

function ScreenshotLightbox({
  screenshots,
  initialIndex,
  onClose,
  t,
}: {
  screenshots: ScreenshotThumbnail[];
  initialIndex: number;
  onClose: () => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  const [index, setIndex] = useState(initialIndex);
  const [lightboxError, setLightboxError] = useState(false);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "ArrowLeft") setIndex((i) => Math.max(0, i - 1));
      if (e.key === "ArrowRight") setIndex((i) => Math.min(screenshots.length - 1, i + 1));
    },
    [onClose, screenshots.length],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (screenshots.length === 0) return null;

  const current = screenshots[index];

  return (
    <div className="screenshot-lightbox-overlay" onClick={onClose}>
      <div className="screenshot-lightbox" onClick={(e) => e.stopPropagation()}>
        <button className="lightbox-close" onClick={onClose}>
          ✕
        </button>
        {screenshots.length > 1 && (
          <>
            <button
              className="lightbox-nav lightbox-prev"
              onClick={() => { setIndex((i) => Math.max(0, i - 1)); setLightboxError(false); }}
              disabled={index === 0}
            >
              ‹
            </button>
            <button
              className="lightbox-nav lightbox-next"
              onClick={() => { setIndex((i) => Math.min(screenshots.length - 1, i + 1)); setLightboxError(false); }}
              disabled={index === screenshots.length - 1}
            >
              ›
            </button>
          </>
        )}
        {lightboxError ? (
          <div className="lightbox-fallback">
            <p>{t('settings.imageLoadError')}</p>
            <p className="text-muted">{current.file_path}</p>
          </div>
        ) : (
          <img
            src={convertFileSrc(current.file_path)}
            alt={`screenshot ${index + 1}`}
            className="lightbox-image"
            onError={() => setLightboxError(true)}
          />
        )}
        <div className="lightbox-counter">
          {index + 1} / {screenshots.length}
        </div>
      </div>
    </div>
  );
}

export function Timeline() {
  const { t } = useI18n();
  const {
    todaySegments,
    recordingStatus,
    loadTodayTimeline,
    updateSegment,
    deleteSegment,
    mergeSegments,
    addManualSegment,
    refreshStatus,
    isRecording,
    isPaused,
    startRecording,
    stopRecording,
    pauseRecording,
    resumeRecording,
  } = useScreenshotStore();

  const lastLlmCost = useScreenshotStore((s) => s.lastLlmCost);
  const recentCostTime = useScreenshotStore((s) => s.recentCostTime);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [showAddForm, setShowAddForm] = useState(false);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [lightboxData, setLightboxData] = useState<{
    screenshots: ScreenshotThumbnail[];
    index: number;
  } | null>(null);
  const [costToast, setCostToast] = useState<{ cost: number; tokens: number; visible: boolean } | null>(null);

  useEffect(() => {
    loadTodayTimeline();
    refreshStatus();
  }, [loadTodayTimeline, refreshStatus]);

  useEffect(() => {
    if (lastLlmCost && Date.now() - recentCostTime < 5000) {
      setCostToast({ cost: lastLlmCost.cost, tokens: lastLlmCost.total_tokens, visible: true });
      const timer = setTimeout(() => setCostToast(null), 4000);
      return () => clearTimeout(timer);
    }
  }, [lastLlmCost, recentCostTime]);

  const totalDuration = todaySegments.reduce(
    (sum, s) => sum + s.duration_secs,
    0,
  );
  const totalHours = Math.floor(totalDuration / 3600);
  const totalMins = Math.floor((totalDuration % 3600) / 60);

  const handleMerge = async () => {
    if (selectedIds.size < 2) return;
    const label = prompt(t('timeline.mergeLabel'));
    if (!label) return;
    await mergeSegments(Array.from(selectedIds), label);
    setSelectedIds(new Set());
  };

  return (
    <div className="timeline">
      <div className="timeline-header">
        <h2>📅 {getDateLabel(t)}</h2>
        <div className="timeline-stats">
          <span className="stat-badge">
            ⏱ {t('settings.recorded')}: {totalHours}h {totalMins}min
          </span>
          <span className="stat-badge">
            ⚡ {t('settings.segments', { n: todaySegments.length })}
          </span>
          <span
            className={`recording-indicator ${recordingStatus}`}
            title={
              recordingStatus === "recording"
                ? t('timeline.recording')
                : recordingStatus === "paused"
                  ? t('timeline.paused')
                  : recordingStatus === "error"
                    ? t('timeline.error')
                    : t('timeline.idle')
            }
          >
            {recordingStatus === "recording" && t('timeline.recording')}
            {recordingStatus === "paused" && "⏸ " + t('timeline.paused')}
            {recordingStatus === "idle" && "⏹ " + t('timeline.idle')}
            {recordingStatus === "error" && "⚠️ " + t('timeline.error')}
          </span>
        </div>
      </div>

      {/* Recording controls */}
      <div className="recording-controls">
        {!isRecording && (
          <button className="btn btn-primary" onClick={startRecording}>
            {t('settings.btn.start')}
          </button>
        )}
        {isRecording && !isPaused && (
          <>
            <button className="btn btn-warning" onClick={pauseRecording}>
              {t('settings.btn.pause')}
            </button>
            <button className="btn btn-danger" onClick={stopRecording}>
              {t('settings.btn.stop')}
            </button>
          </>
        )}
        {isRecording && isPaused && (
          <>
            <button className="btn btn-primary" onClick={resumeRecording}>
              {t('settings.btn.resume')}
            </button>
            <button className="btn btn-danger" onClick={stopRecording}>
              {t('settings.btn.stop')}
            </button>
          </>
        )}
      </div>

      {/* LLM Cost Toast */}
      {costToast && (
        <div className="cost-toast">
          <span>💰 ¥{costToast.cost.toFixed(4)}</span>
          <span className="cost-detail">({costToast.tokens} tokens)</span>
        </div>
      )}

      {/* Timeline entries */}
      <div className="timeline-entries">
        {todaySegments.length === 0 && (
          <div className="empty-state">
            <p>{t('settings.noActivityToday')}</p>
            <p className="text-muted">
              {t('settings.startHint')}
            </p>
          </div>
        )}

        {todaySegments.slice().reverse().map((segment) => (
          <div
            key={segment.id}
            className={`timeline-card ${selectedIds.has(segment.id) ? "selected" : ""}`}
          >
            <div className="card-header">
              <input
                type="checkbox"
                checked={selectedIds.has(segment.id)}
                onChange={() => {
                  const next = new Set(selectedIds);
                  if (next.has(segment.id)) next.delete(segment.id);
                  else next.add(segment.id);
                  setSelectedIds(next);
                }}
                className="checkbox"
              />
              <span className="card-time">
                {formatTime(segment.start_time)} — {formatTime(segment.end_time)}
              </span>
              <span className="card-duration">
                {formatDuration(segment.duration_secs, t)}
              </span>
            </div>

            <div className="card-body">
              <span className="card-icon">
                {CATEGORY_ICONS[segment.category] ?? "📌"}
              </span>
              <span className="card-category-badge">{segment.category}</span>
              <span className="card-title">
                {segment.user_label ?? segment.llm_summary ?? segment.app_name ?? t('timeline.unlabeled')}
              </span>
            </div>

            {segment.app_name && (
              <div className="card-meta">
                💻 {segment.app_name}
                {segment.window_title && ` · ${segment.window_title}`}
              </div>
            )}

            {/* Screenshot thumbnails */}
            {segment.screenshots && segment.screenshots.length > 0 && (
              <div className="card-thumbnails">
                {segment.screenshots.map((ss, i) => (
                  <ScreenshotThumb
                    key={ss.id}
                    ss={ss}
                    onClick={() =>
                      setLightboxData({
                        screenshots: segment.screenshots!,
                        index: i,
                      })
                    }
                  />
                ))}
                <span className="thumbnails-count">
                  {t('settings.units.screenshots', { n: segment.screenshots.length })}
                </span>
              </div>
            )}

            {editingId === segment.id && (
              <EditableSegment
                segment={segment}
                onSave={async (data) => {
                  await updateSegment(segment.id, data);
                  setEditingId(null);
                }}
                onCancel={() => setEditingId(null)}
              />
            )}

            <div className="card-actions">
              <button
                className="btn btn-ghost btn-sm"
                onClick={() =>
                  setEditingId(editingId === segment.id ? null : segment.id)
                }
              >
                ✏️ {t('settings.edit')}
              </button>
              {deleteConfirmId === segment.id ? (
                <>
                  <span className="confirm-text">{t('timeline.confirmDelete')}</span>
                  <button
                    className="btn btn-danger btn-sm"
                    onClick={() => {
                      deleteSegment(segment.id);
                      setDeleteConfirmId(null);
                    }}
                  >
                    {t('timeline.confirm')}
                  </button>
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => setDeleteConfirmId(null)}
                  >
                    {t('timeline.cancel')}
                  </button>
                </>
              ) : (
                <button
                  className="btn btn-ghost btn-sm text-danger"
                  onClick={() => setDeleteConfirmId(segment.id)}
                >
                  🗑️ {t('timeline.delete')}
                </button>
              )}
            </div>
          </div>
        ))}
      </div>

      {/* Batch actions */}
      {selectedIds.size >= 2 && (
        <div className="batch-actions">
          <button className="btn btn-primary" onClick={handleMerge}>
            🔗 {t('settings.merge')} ({selectedIds.size})
          </button>
          <button
            className="btn btn-ghost"
            onClick={() => setSelectedIds(new Set())}
          >
            {t('settings.deselect')}
          </button>
        </div>
      )}

      {/* Manual add */}
      {showAddForm ? (
        <ManualAddForm
          onSubmit={async (segment) => {
            await addManualSegment(segment);
            setShowAddForm(false);
          }}
          onCancel={() => setShowAddForm(false)}
        />
      ) : (
        <button
          className="btn btn-ghost add-activity-btn"
          onClick={() => setShowAddForm(true)}
        >
          {t('settings.btn.addActivity')}
        </button>
      )}

      {/* Screenshot lightbox */}
      {lightboxData && (
        <ScreenshotLightbox
          screenshots={lightboxData.screenshots}
          initialIndex={lightboxData.index}
          onClose={() => setLightboxData(null)}
          t={t}
        />
      )}
    </div>
  );
}

function ManualAddForm({
  onSubmit,
  onCancel,
}: {
  onSubmit: (segment: ActivitySegment) => void;
  onCancel: () => void;
}) {
  const { t } = useI18n();
  const now = new Date();
  const nowISO = now.toISOString().slice(0, 16);
  const [start, setStart] = useState(nowISO);
  const [end, setEnd] = useState(nowISO);
  const [label, setLabel] = useState("");
  const [category, setCategory] = useState("other");

  const handleSubmit = () => {
    const startDate = new Date(start);
    const endDate = new Date(end);
    const durationSecs = Math.max(
      0,
      Math.floor((endDate.getTime() - startDate.getTime()) / 1000),
    );

    const segment: ActivitySegment = {
      id: crypto.randomUUID(),
      start_time: startDate.toISOString().slice(0, 19).replace("T", " "),
      end_time: endDate.toISOString().slice(0, 19).replace("T", " "),
      duration_secs: durationSecs,
      app_name: null,
      window_title: null,
      llm_summary: null,
      category,
      user_label: label || null,
      confidence: 1.0,
      is_manual: true,
      source_frame_ids: null,
    };
    onSubmit(segment);
  };

  return (
    <div className="manual-add-form">
      <h4>{t('settings.pageTitle')}</h4>
      <input
        type="text"
        value={label}
        onChange={(e) => setLabel(e.target.value)}
        placeholder={t('timeline.activityName')}
        className="input"
      />
      <select
        value={category}
        onChange={(e) => setCategory(e.target.value)}
        className="select"
      >
        <option value="dev">{t('category.dev')}</option>
        <option value="meeting">{t('category.meeting')}</option>
        <option value="communication">{t('category.communication')}</option>
        <option value="design">{t('category.design')}</option>
        <option value="documentation">{t('category.documentation')}</option>
        <option value="browsing">{t('category.browsing')}</option>
        <option value="management">{t('category.management')}</option>
        <option value="other">{t('category.other')}</option>
      </select>
      <div className="edit-time-row">
        <label>
          {t('settings.startLabel')}
          <input
            type="datetime-local"
            value={start}
            onChange={(e) => setStart(e.target.value)}
            className="input time-input"
          />
        </label>
        <label>
          {t('settings.endLabel')}
          <input
            type="datetime-local"
            value={end}
            onChange={(e) => setEnd(e.target.value)}
            className="input time-input"
          />
        </label>
      </div>
      <div className="edit-actions">
        <button className="btn btn-primary btn-sm" onClick={handleSubmit}>
          {t('settings.btn.add')}
        </button>
        <button className="btn btn-ghost btn-sm" onClick={onCancel}>
          {t('settings.btn.cancel')}
        </button>
      </div>
    </div>
  );
}
