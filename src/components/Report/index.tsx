import { useEffect, useState, useMemo, useCallback } from "react";
import {
  PieChart,
  Pie,
  Cell,
  Tooltip,
  ResponsiveContainer,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
} from "recharts";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useReportStore, type ReportView } from "../../stores/useReportStore";
import { api } from "../../lib/tauri";
import type { UsageSummary } from "../../types";
import { useI18n } from "../../i18n";

const COLORS = [
  "#4CAF50",
  "#2196F3",
  "#FF9800",
  "#9C27B0",
  "#00BCD4",
  "#E91E63",
  "#607D8B",
  "#795548",
];

function useViewLabels() {
  const { t } = useI18n();
  return {
    daily: t('report.daily'),
    weekly: t('report.weekly'),
    monthly: t('report.monthly'),
  } as Record<ReportView, string>;
}

function getTodayISO(): string {
  return new Date().toISOString().slice(0, 10);
}

/** Format a number with locale separators (e.g. 12,345) */
function fmtNum(n: number): string {
  return n.toLocaleString("zh-CN");
}

/** Usage / Cost card shown below the activity charts */
function UsageCostCard({
  dailyUsage,
  monthlyUsage,
  t,
}: {
  dailyUsage: UsageSummary | null;
  monthlyUsage: UsageSummary | null;
  t: (key: string) => string;
}) {
  const hasData = (dailyUsage && dailyUsage.call_count > 0) || (monthlyUsage && monthlyUsage.call_count > 0);

  if (!hasData) {
    return (
      <div className="usage-cost-card">
        <h4>{t('report.cost.title')}</h4>
        <p style={{ color: '#999', textAlign: 'center', padding: '20px 0' }}>{t('report.cost.noData')}</p>
      </div>
    );
  }

  return (
    <div className="usage-cost-card">
      <h4>{t('report.cost.title')}</h4>
      <div className="usage-grid">
        {/* Daily column */}
        <div className="usage-column">
          <h5>📅 {t('report.cost.today')}</h5>
          {dailyUsage && dailyUsage.call_count > 0 ? (
            <UsageStatRow summary={dailyUsage} t={t} />
          ) : (
            <p className="text-muted">{t('report.cost.noData')}</p>
          )}
        </div>

        {/* Monthly column */}
        <div className="usage-column">
          <h5>📆 {t('report.cost.month')}</h5>
          {monthlyUsage && monthlyUsage.call_count > 0 ? (
            <UsageStatRow summary={monthlyUsage} t={t} />
          ) : (
            <p className="text-muted">{t('report.cost.noData')}</p>
          )}
        </div>
      </div>
    </div>
  );
}

function UsageStatRow({ summary, t }: { summary: UsageSummary; t: (key: string) => string }) {
  return (
    <div className="usage-stats">
      <div className="usage-stat">
        <span className="usage-stat-label">{t('report.cost.callCount')}</span>
        <span className="usage-stat-value">{summary.call_count} {t('report.unit.time')}</span>
      </div>
      <div className="usage-stat">
        <span className="usage-stat-label">{t('report.cost.promptTokens')}</span>
        <span className="usage-stat-value">{fmtNum(summary.total_prompt_tokens)} tokens</span>
      </div>
      <div className="usage-stat">
        <span className="usage-stat-label">{t('report.cost.completionTokens')}</span>
        <span className="usage-stat-value">{fmtNum(summary.total_completion_tokens)} tokens</span>
      </div>
      <div className="usage-stat">
        <span className="usage-stat-label">{t('report.cost.totalTokens')}</span>
        <span className="usage-stat-value">{fmtNum(summary.total_tokens)}</span>
      </div>
      <div className="usage-stat usage-stat-cost">
        <span className="usage-stat-label">{t('report.cost.estimatedCost')}</span>
        <span className="usage-stat-value-cost">
          ¥{summary.total_cost.toFixed(4)}
        </span>
      </div>
    </div>
  );
}

/** Convert breakdown tuples to chart-friendly format (minutes) */
function breakdownToChart(
  breakdown: [string, number][],
): { name: string; value: number }[] {
  return breakdown.map(([name, secs]) => ({
    name,
    value: Math.round(secs / 60),
  }));
}

/** Format total seconds to "X小时Y分" */
function fmtDuration(totalSecs: number, t: (key: string, params?: Record<string, string | number>) => string): string {
  const hours = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  if (hours > 0) return t('report.time.format', { x: hours, y: mins });
  return t('report.time.minOnly', { m: mins });
}

export function Report() {
  const { t } = useI18n();
  const viewLabels = useViewLabels();
  const {
    currentView,
    reportData,
    dataLoading,
    aiSummary,
    summaryLoading,
    summaryError,
    existingSummary,
    setView,
    loadReportData,
    generateSummary,
    clearSummary,
  } = useReportStore();

  const [selectedDate, setSelectedDate] = useState(getTodayISO());
  const [dailyUsage, setDailyUsage] = useState<UsageSummary | null>(null);
  const [monthlyUsage, setMonthlyUsage] = useState<UsageSummary | null>(null);
  const [usageLoading, setUsageLoading] = useState(false);

  // Load report data (no LLM) when date or view changes
  useEffect(() => {
    clearSummary();
    if (currentView === "daily") {
      loadReportData("daily", { date: selectedDate });
    } else if (currentView === "weekly") {
      loadReportData("weekly", { date: selectedDate, endDate: selectedDate });
    } else if (currentView === "monthly") {
      const now = new Date();
      loadReportData("monthly", { year: now.getFullYear(), month: now.getMonth() + 1 });
    }
  }, [selectedDate, currentView, loadReportData, clearSummary]);

  // Fetch LLM usage when date changes
  useEffect(() => {
    const fetchUsage = async () => {
      setUsageLoading(true);
      try {
        const [daily, monthly] = await Promise.all([
          api.getDailyUsage(selectedDate),
          (() => {
            const now = new Date();
            return api.getMonthlyUsage(now.getFullYear(), now.getMonth() + 1);
          })(),
        ]);
        setDailyUsage(daily);
        setMonthlyUsage(monthly);
      } catch {
        setDailyUsage(null);
        setMonthlyUsage(null);
      } finally {
        setUsageLoading(false);
      }
    };
    fetchUsage();
  }, [selectedDate]);

  const handleViewChange = useCallback(
    (view: ReportView) => {
      setView(view);
    },
    [setView],
  );

  const handleGenerateSummary = useCallback(() => {
    if (currentView === "daily") {
      generateSummary("daily", { date: selectedDate });
    } else if (currentView === "weekly") {
      generateSummary("weekly", { date: selectedDate, endDate: selectedDate });
    } else if (currentView === "monthly") {
      const now = new Date();
      generateSummary("monthly", { year: now.getFullYear(), month: now.getMonth() + 1 });
    }
  }, [currentView, selectedDate, generateSummary]);

  const breakdownData = useMemo(
    () => (reportData ? breakdownToChart(reportData.breakdown) : []),
    [reportData],
  );

  const totalStr = useMemo(
    () => (reportData ? fmtDuration(reportData.total_seconds, t) : ""),
    [reportData, t],
  );

  const hasSegments = reportData && reportData.segments.length > 0;

  return (
    <div className="report-page">
      <div className="report-header">
        <div className="report-tabs">
          {(Object.entries(viewLabels) as [ReportView, string][]).map(
            ([key, label]) => (
              <button
                key={key}
                className={`tab-btn ${currentView === key ? "active" : ""}`}
                onClick={() => handleViewChange(key)}
              >
                {label}
              </button>
            ),
          )}
        </div>

        {currentView === "daily" && (
          <input
            type="date"
            value={selectedDate}
            onChange={(e) => setSelectedDate(e.target.value)}
            className="input date-input"
          />
        )}
      </div>

      {/* Data loading (only for the initial data fetch — very fast) */}
      {dataLoading && (
        <div className="loading-state">
          <div className="spinner" />
          <p>{t('report.noData')}</p>
        </div>
      )}

      {summaryError && dataLoading === false && (
        <div className="error-banner">{summaryError}</div>
      )}

      {/* Data loaded: show charts + segments */}
      {!dataLoading && hasSegments && (
        <div className="report-content">
          {totalStr && (
            <div className="report-total">
              📄 {selectedDate} · {t('report.header.working')}{viewLabels[currentView]}
              <span className="total-hours">{t('report.stats.totalHours')}: {totalStr}</span>
            </div>
          )}

          {/* Category breakdown charts */}
          {breakdownData.length > 0 && (
            <div className="charts-row">
              <div className="chart-container">
                <h4>{t('report.chart.timeAlloc')}</h4>
                <ResponsiveContainer width="100%" height={220}>
                  <PieChart>
                    <Pie
                      data={breakdownData}
                      cx="50%"
                      cy="50%"
                      innerRadius={50}
                      outerRadius={80}
                      dataKey="value"
                      label={({ name, percent }: { name?: string; percent?: number }) =>
                        `${name ?? ''} ${((percent ?? 0) * 100).toFixed(0)}%`
                      }
                    >
                      {breakdownData.map((_, idx) => (
                        <Cell
                          key={idx}
                          fill={COLORS[idx % COLORS.length]}
                        />
                      ))}
                    </Pie>
                    <Tooltip />
                  </PieChart>
                </ResponsiveContainer>
              </div>

              <div className="chart-container">
                <h4>{t('report.chart.categoryDuration')}</h4>
                <ResponsiveContainer width="100%" height={220}>
                  <BarChart data={breakdownData}>
                    <CartesianGrid strokeDasharray="3 3" />
                    <XAxis dataKey="name" />
                    <YAxis unit=" min" />
                    <Tooltip />
                    <Bar dataKey="value" fill="#2196F3" radius={[4, 4, 0, 0]}>
                      {breakdownData.map((_, idx) => (
                        <Cell
                          key={idx}
                          fill={COLORS[idx % COLORS.length]}
                        />
                      ))}
                    </Bar>
                  </BarChart>
                </ResponsiveContainer>
              </div>
            </div>
          )}

          {/* LLM Usage / Cost Card */}
          {!usageLoading && (
            <UsageCostCard
              dailyUsage={dailyUsage}
              monthlyUsage={monthlyUsage}
              t={t}
            />
          )}

          {/* Report markdown (from DB data, not LLM) */}
          {reportData.markdown && (
            <div className="report-md">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  h2: ({ children, ...props }) => (
                    <h2 className="md-heading" {...props}>
                      {children}
                    </h2>
                  ),
                  table: ({ children, ...props }) => (
                    <div className="table-wrapper">
                      <table className="md-table" {...props}>
                        {children}
                      </table>
                    </div>
                  ),
                }}
              >
                {reportData.markdown}
              </ReactMarkdown>
            </div>
          )}

          {/* ── AI 总结区域 ── */}
          <div className="summary-section">
            {/* 已有总结 */}
            {existingSummary && !aiSummary && (
              <div className="report-md summary-existing">
                <h4>{t('report.summary.title')}</h4>
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {existingSummary}
                </ReactMarkdown>
              </div>
            )}

            {/* AI 正在生成 */}
            {summaryLoading && (
              <div className="loading-state">
                <div className="spinner" />
                <p>{t('report.summary.generating')}</p>
              </div>
            )}

            {/* AI 总结生成完毕 */}
            {aiSummary && (
              <div className="report-md summary-existing">
                <h4>{t('report.summary.title')}</h4>
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {aiSummary}
                </ReactMarkdown>
              </div>
            )}

            {/* 生成按钮：没有总结且没有正在加载时显示 */}
            {!existingSummary && !summaryLoading && !aiSummary && (
              <button
                className="generate-summary-btn"
                onClick={handleGenerateSummary}
              >
                {t('report.summary.generate')}
              </button>
            )}
          </div>

          {/* Notes area */}
          <div className="notes-section">
            <h4>📝 {t('report.notes.save')}</h4>
            <textarea
              placeholder={t('report.notes.placeholder')}
              className="textarea"
              rows={4}
            />
            <button
              className="btn btn-primary"
              onClick={() => {
                // TODO: replace alert() with Toast component
                alert(t('report.notes.saved'));
              }}
            >
              💾 {t('report.notes.save')}
            </button>
          </div>

          {/* Action buttons */}
          <div className="report-actions">
            <button
              className="btn btn-primary"
              onClick={() => {
                const content = reportData.markdown || "";
                const blob = new Blob([content], {
                  type: "text/markdown",
                });
                const url = URL.createObjectURL(blob);
                const a = document.createElement("a");
                a.href = url;
                a.download = `work-report-${selectedDate}.md`;
                a.click();
                URL.revokeObjectURL(url);
              }}
            >
              📥 {t('report.export.markdown')}
            </button>
            <button
              className="btn btn-ghost"
              onClick={() => {
                const content = reportData.markdown || "";
                navigator.clipboard.writeText(content);
                // TODO: replace alert() with Toast component
                alert(t('report.export.copied'));
              }}
            >
              📋 {t('report.export.copy')}
            </button>
          </div>
        </div>
      )}

      {/* Empty state: no data and no loading */}
      {!dataLoading && !hasSegments && !summaryError && (
        <div className="empty-state">
          <p>
            {t('report.noActivity')}
          </p>
          <p className="text-muted">
            {t('report.startRecording')}
          </p>
        </div>
      )}
    </div>
  );
}
