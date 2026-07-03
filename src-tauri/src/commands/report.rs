//! Tauri IPC commands for report generation.
//!
//! Provides commands to generate structured daily / weekly / monthly reports,
//! query daily summaries, and get activity category breakdowns.
//!
//! Report generation flow:
//! 1. Query activity segments in the date range
//! 2. Merge adjacent same-category segments
//! 3. Compute per-category duration totals
//! 4. Assemble structured data
//! 5. Optionally call the LLM for a text summary
//! 6. Persist the report to `daily_summaries` / `period_summaries`
//! 7. Return the report as HTML / Markdown

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::Serialize;
use tauri::{Emitter, State};

use crate::db::models::{ActivitySegment, DailySummary, PeriodSummary};

// ---------------------------------------------------------------------------
// ReportData — 不含 AI 总结的结构化报表数据
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ReportData {
    pub title: String,
    pub date_label: String,
    pub total_seconds: i64,
    pub segments: Vec<ActivitySegment>,
    pub breakdown: Vec<(String, i64)>,
    pub html: String,
    pub markdown: String,
    pub has_existing_summary: bool,
}

// ---------------------------------------------------------------------------
// Helper: merge adjacent same-category segments
// ---------------------------------------------------------------------------

fn merge_adjacent(segments: &[ActivitySegment]) -> Vec<ActivitySegment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut merged: Vec<ActivitySegment> = Vec::new();
    let mut cur = segments[0].clone();

    for s in &segments[1..] {
        if s.category == cur.category {
            // Same category — extend current segment
            cur.end_time = s.end_time.clone();
            cur.duration_secs += s.duration_secs;
            // Concatenate summaries
            if let Some(ref summary) = s.llm_summary {
                let prev = cur.llm_summary.unwrap_or_default();
                cur.llm_summary = Some(if prev.is_empty() {
                    summary.clone()
                } else {
                    format!("{prev}; {summary}")
                });
            }
            // Merge frame IDs
            let existing = cur.source_frame_ids.unwrap_or_default();
            let add = s.source_frame_ids.as_deref().unwrap_or("");
            cur.source_frame_ids = Some(if existing.is_empty() {
                add.to_string()
            } else if add.is_empty() {
                existing
            } else {
                format!("{existing},{add}")
            });
        } else {
            // Different category — push current, start new
            merged.push(cur);
            cur = s.clone();
        }
    }
    merged.push(cur);
    merged
}

// ---------------------------------------------------------------------------
// Helper: compute category breakdown from segments
// ---------------------------------------------------------------------------

fn category_breakdown(segments: &[ActivitySegment]) -> Vec<(String, i64)> {
    let mut map: HashMap<String, i64> = HashMap::new();
    for s in segments {
        *map.entry(s.category.clone()).or_default() += s.duration_secs;
    }
    let mut result: Vec<(String, i64)> = map.into_iter().collect();
    result.sort_by(|a, b| b.1.cmp(&a.1)); // descending by duration
    result
}

// ---------------------------------------------------------------------------
// Helper: escape HTML to prevent XSS
// ---------------------------------------------------------------------------

/// Escape HTML special characters to prevent XSS.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// ---------------------------------------------------------------------------
// Helper: build HTML report from segments and breakdown
// ---------------------------------------------------------------------------

fn build_report_html(
    title: &str,
    date_label: &str,
    segments: &[ActivitySegment],
    breakdown: &[(String, i64)],
    llm_summary: Option<&str>,
) -> String {
    let total_secs: i64 = segments.iter().map(|s| s.duration_secs).sum();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;

    let mut html = format!(
        r#"<h2>{}</h2>
<p><strong>{}</strong> — 总耗时: {}小时{}分钟</p>
<h3>活动分类统计</h3>
<ul>"#,
        html_escape(title),
        html_escape(date_label),
        hours,
        mins,
    );

    for (cat, secs) in breakdown {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        html.push_str(&format!(
            "<li><strong>{}</strong>: {}小时{}分钟</li>",
            html_escape(cat),
            h,
            m,
        ));
    }

    html.push_str("</ul><h3>时间线</h3><table border='1' cellpadding='4' cellspacing='0' style='border-collapse:collapse;width:100%'><tr><th>时间</th><th>活动</th><th>分类</th><th>时长</th></tr>");

    for s in segments {
        let dur_mins = s.duration_secs / 60;
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}分钟</td></tr>",
            html_escape(&s.start_time),
            html_escape(s.llm_summary.as_deref().unwrap_or("—")),
            html_escape(&s.category),
            dur_mins,
        ));
    }

    html.push_str("</table>");

    if let Some(summary) = llm_summary {
        html.push_str(&format!("
<h3>AI 总结</h3>
<div style='background:#f5f5f5;padding:12px;border-radius:8px;'>{}</div>", html_escape(summary)));
    }

    html
}

fn build_markdown_report(
    title: &str,
    date_label: &str,
    segments: &[ActivitySegment],
    breakdown: &[(String, i64)],
    llm_summary: Option<&str>,
) -> String {
    let total_secs: i64 = segments.iter().map(|s| s.duration_secs).sum();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;

    let mut md = format!(
        "# {}\n\n**{}** — 总耗时: {}小时{}分钟\n\n## 活动分类统计\n\n",
        html_escape(title),
        html_escape(date_label),
        hours,
        mins,
    );

    for (cat, secs) in breakdown {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        md.push_str(&format!("- **{}**: {}小时{}分钟\n", html_escape(cat), h, m));
    }

    md.push_str("\n## 时间线\n\n| 时间 | 活动 | 分类 | 时长 |\n|------|------|------|------|\n");
    for s in segments {
        let dur_mins = s.duration_secs / 60;
        md.push_str(&format!(
            "| {} | {} | {} | {}分钟 |\n",
            html_escape(&s.start_time),
            html_escape(s.llm_summary.as_deref().unwrap_or("—")),
            html_escape(&s.category),
            dur_mins,
        ));
    }

    if let Some(summary) = llm_summary {
        md.push_str(&format!("\n## AI 总结\n\n{}\n", html_escape(summary)));
    }

    md
}

// ---------------------------------------------------------------------------
// Helper: call LLM for report summary if configured
// ---------------------------------------------------------------------------

async fn generate_llm_summary(
    state: &crate::AppState,
    app_handle: &tauri::AppHandle,
    segments: &[ActivitySegment],
    prompt: &str,
) -> Option<String> {
    let db_config = match state.db.get_active_config() {
        Ok(Some(c)) => c,
        _ => return None,
    };

    let llm_cfg = crate::config::LlmConfig {
        name: db_config.name,
        provider: db_config.provider,
        base_url: db_config.base_url,
        model: db_config.model,
        api_key: db_config.api_key,
        max_tokens: db_config.max_tokens as u32,
        is_active: true,
        use_batch_api: false,
    };

    let adapter = match crate::llm::create_adapter(&llm_cfg) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[report] Failed to create LLM adapter: {e}");
            return None;
        }
    };

    let seg_summaries: Vec<crate::llm::adapter::ActivitySegmentSummary> = segments
        .iter()
        .map(|s| crate::llm::adapter::ActivitySegmentSummary {
            start_time: s.start_time.clone(),
            end_time: s.end_time.clone(),
            duration_secs: s.duration_secs,
            app_name: s.app_name.clone(),
            category: s.category.clone(),
            summary: s.llm_summary.clone().unwrap_or_default(),
        })
        .collect();

    let context = crate::llm::adapter::ReportContext {
        date_range: (
            segments.first().map(|s| s.start_time.clone()).unwrap_or_default(),
            segments.last().map(|s| s.end_time.clone()).unwrap_or_default(),
        ),
        segments: seg_summaries,
    };

    let timeout_duration = std::time::Duration::from_secs(25);
    match tokio::time::timeout(timeout_duration, adapter.generate_report(&context, prompt)).await
    {
        Ok(Ok((text, usage))) => {
            // 记录 LLM 费用
            if let Some(ref u) = usage {
                let cost = crate::llm::adapter::estimate_cost(adapter.model_name(), u);
                let now_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let usage_log = crate::db::models::LlmUsageLog {
                    id: 0,
                    model: adapter.model_name().to_string(),
                    provider: adapter.provider_name().to_string(),
                    prompt_tokens: u.prompt_tokens as i64,
                    completion_tokens: u.completion_tokens as i64,
                    total_tokens: u.total_tokens as i64,
                    estimated_cost: cost,
                    created_at: now_str.clone(),
                };
                if let Err(e) = state.db.insert_llm_usage_log(&usage_log) {
                    eprintln!("[REPORT] Failed to insert LLM usage log: {e}");
                } else {
                    eprintln!("[LLM-COST] Report summary: tokens={}, cost=¥{cost:.6}", u.total_tokens);
                    let _ = app_handle.emit(
                        "analysis:llm-cost",
                        serde_json::json!({
                            "model": adapter.model_name(),
                            "provider": adapter.provider_name(),
                            "prompt_tokens": u.prompt_tokens,
                            "completion_tokens": u.completion_tokens,
                            "total_tokens": u.total_tokens,
                            "cost": cost,
                            "type": "report",
                            "timestamp": now_str,
                        }),
                    );
                }
            }
            Some(text)
        }
        Ok(Err(e)) => {
            eprintln!("[REPORT] LLM summary failed: {e}");
            None
        }
        Err(_elapsed) => {
            eprintln!("[REPORT] LLM summary timed out after 25s, graceful degradation");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Generate a daily report for the given date (YYYY-MM-DD).
/// 保留旧接口兼容，内部改为调用拆分后的函数。
#[tauri::command]
pub async fn generate_daily_report(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    date: String,
) -> Result<String, String> {
    eprintln!("[REPORT] generate_daily_report (legacy) for {date}");
    let data = build_report_data(&state, "daily", Some(&date), None, None, None).await?;

    if data.segments.is_empty() {
        return Ok(format!("# {date} 日报\n\n当天没有记录到活动。"));
    }

    let locale = crate::config::AppConfig::load_or_default().locale;
    let llm_summary = generate_llm_summary(
        &state,
        &app_handle,
        &data.segments,
        crate::config::get_daily_summary_prompt(&locale),
    )
    .await;

    let md = if let Some(ref summary) = llm_summary {
        let breakdown = category_breakdown(&data.segments);
        let full_md = build_markdown_report("日报", &date, &data.segments, &breakdown, Some(summary));
        // Persist
        let now_str = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let break_json = serde_json::to_string(&breakdown).unwrap_or_default();
        let full_html = build_report_html("日报", &date, &data.segments, &breakdown, Some(summary));
        let existing = state.db.get_daily_summary(&date).ok().flatten();
        let summary_record = DailySummary {
            id: existing.as_ref().map(|s| s.id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            date: date.clone(),
            total_seconds: data.total_seconds,
            segment_count: data.segments.len() as i64,
            activity_breakdown: Some(break_json),
            llm_summary: Some(summary.clone()),
            user_notes: existing.as_ref().and_then(|s| s.user_notes.clone()),
            report_html: Some(full_html),
            created_at: existing.as_ref().map(|s| s.created_at.clone()).unwrap_or_else(|| now_str.clone()),
            updated_at: now_str,
        };
        let _ = state.db.upsert_daily_summary(&summary_record);
        full_md
    } else {
        data.markdown
    };

    Ok(md)
}

/// Generate a weekly report ending on the given date (YYYY-MM-DD).
/// This is called from the lib.rs delegate rather than being a #[tauri::command] directly,
/// 保留旧接口兼容，内部改为调用拆分后的函数。
pub async fn generate_weekly_report_impl(
    state: &crate::AppState,
    app_handle: &tauri::AppHandle,
    end_date: &str,
) -> Result<String, String> {
    eprintln!("[REPORT] generate_weekly_report_impl (legacy) for {end_date}");
    let data = build_report_data(state, "weekly", None, Some(end_date), None, None).await?;
    let date_label = data.date_label.clone();

    if data.segments.is_empty() {
        return Ok(format!("# {date_label} 周报\n\n该周期内没有记录到活动。"));
    }

    let locale = crate::config::AppConfig::load_or_default().locale;
    let llm_summary = generate_llm_summary(
        state,
        app_handle,
        &data.segments,
        crate::config::get_weekly_summary_prompt(&locale),
    )
    .await;

    let md = if let Some(ref summary) = llm_summary {
        let breakdown = category_breakdown(&data.segments);
        let full_md = build_markdown_report("周报", &date_label, &data.segments, &breakdown, Some(summary));
        let full_html = build_report_html("周报", &date_label, &data.segments, &breakdown, Some(summary));
        let break_json = serde_json::to_string(&breakdown).unwrap_or_default();
        let end_dt = NaiveDate::parse_from_str(end_date, "%Y-%m-%d").unwrap_or_default();
        let start_dt = end_dt.checked_sub_days(chrono::Days::new(6)).unwrap_or(end_dt);
        let from_s = start_dt.format("%Y-%m-%d").to_string();
        let to_s = format!("{} 23:59:59", end_date);
        let daily_trend_json = serde_json::to_string(
            &state.db.get_daily_trend(&from_s, &to_s).unwrap_or_default(),
        ).unwrap_or_default();

        let summary = PeriodSummary {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: "weekly".to_string(),
            start_date: from_s,
            end_date: end_date.to_string(),
            total_seconds: data.total_seconds,
            daily_trend: Some(daily_trend_json),
            activity_breakdown: Some(break_json),
            llm_summary: Some(summary.clone()),
            user_notes: None,
            report_html: Some(full_html),
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        let _ = state.db.insert_period_summary(&summary);
        full_md
    } else {
        data.markdown
    };

    Ok(md)
}

/// Generate a monthly report for the given year and month.
/// 保留旧接口兼容，内部改为调用拆分后的函数。
pub async fn generate_monthly_report_impl(
    state: &crate::AppState,
    app_handle: &tauri::AppHandle,
    year: i32,
    month: u32,
) -> Result<String, String> {
    eprintln!("[REPORT] generate_monthly_report_impl (legacy) for {year}-{month}");
    let data = build_report_data(state, "monthly", None, None, Some(year), Some(month)).await?;
    let date_label = data.date_label.clone();

    if data.segments.is_empty() {
        return Ok(format!("# {date_label} 月报\n\n该月没有记录到活动。"));
    }

    let locale = crate::config::AppConfig::load_or_default().locale;
    let llm_summary = generate_llm_summary(
        state,
        app_handle,
        &data.segments,
        crate::config::get_monthly_summary_prompt(&locale),
    )
    .await;

    let md = if let Some(ref summary) = llm_summary {
        let breakdown = category_breakdown(&data.segments);
        let full_md = build_markdown_report("月报", &date_label, &data.segments, &breakdown, Some(summary));
        let full_html = build_report_html("月报", &date_label, &data.segments, &breakdown, Some(summary));
        let break_json = serde_json::to_string(&breakdown).unwrap_or_default();

        let start = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default();
        let end = {
            if month == 12 {
                NaiveDate::from_ymd_opt(year + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(year, month + 1, 1)
            }
            .map(|d| d.pred_opt().unwrap_or(d))
            .unwrap_or(start)
        };
        let from = start.format("%Y-%m-%d").to_string();
        let to = end.format("%Y-%m-%d 23:59:59").to_string();
        let daily_trend_json = serde_json::to_string(
            &state.db.get_daily_trend(&from, &to).unwrap_or_default(),
        ).unwrap_or_default();

        let summary = PeriodSummary {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: "monthly".to_string(),
            start_date: from,
            end_date: end.format("%Y-%m-%d").to_string(),
            total_seconds: data.total_seconds,
            daily_trend: Some(daily_trend_json),
            activity_breakdown: Some(break_json),
            llm_summary: Some(summary.clone()),
            user_notes: None,
            report_html: Some(full_html),
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        let _ = state.db.insert_period_summary(&summary);
        full_md
    } else {
        data.markdown
    };

    Ok(md)
}

/// Get the daily summary for a specific date.
#[tauri::command]
pub async fn get_daily_summary(
    state: State<'_, crate::AppState>,
    date: String,
) -> Result<Option<DailySummary>, String> {
    let result = state.db.get_daily_summary(&date).map_err(|e| e.to_string())?;
    eprintln!("[REPORT] get_daily_summary for {date}: found={}", result.is_some());
    Ok(result)
}

/// Get activity category breakdown for a time range.
#[tauri::command]
pub async fn get_activity_breakdown(
    state: State<'_, crate::AppState>,
    from: String,
    to: String,
) -> Result<Vec<(String, i64)>, String> {
    let breakdown = state
        .db
        .get_category_breakdown(&from, &to)
        .map_err(|e| e.to_string())?;
    let cats: Vec<String> = breakdown.iter().map(|(c, _)| c.clone()).collect();
    eprintln!("[REPORT] Breakdown {from}~{to}: {} categories: {:?}", breakdown.len(), cats);
    Ok(breakdown)
}

// ---------------------------------------------------------------------------
// Legacy command (preserved for backward compatibility)
// ---------------------------------------------------------------------------

/// Legacy report generation — delegates to daily report.
#[tauri::command]
pub async fn generate_report(app_handle: tauri::AppHandle, state: State<'_, crate::AppState>, date: String) -> Result<String, String> {
    eprintln!("[REPORT] generate_report (legacy) for {date}");
    generate_daily_report(app_handle, state, date).await
}

// ---------------------------------------------------------------------------
// Internal: 构建 ReportData（不含 LLM 调用）
// ---------------------------------------------------------------------------

async fn build_report_data(
    state: &crate::AppState,
    report_type: &str,
    date: Option<&str>,
    end_date: Option<&str>,
    year: Option<i32>,
    month: Option<u32>,
) -> Result<ReportData, String> {
    let (title, date_label, segments) = match report_type {
        "daily" => {
            let d = date.ok_or_else(|| "date is required for daily report".to_string())?;
            let segs = state.db.get_segments_by_date(d).map_err(|e| e.to_string())?;
            if segs.is_empty() {
                eprintln!("[REPORT] get_report_data(daily) for {d}: 0 segments");
            }
            ("日报", d.to_string(), segs)
        }
        "weekly" => {
            let end = end_date.ok_or_else(|| "end_date is required for weekly report".to_string())?;
            let end_dt = NaiveDate::parse_from_str(end, "%Y-%m-%d")
                .map_err(|e| format!("Invalid end_date format: {e}"))?;
            let start = end_dt.checked_sub_days(chrono::Days::new(6)).unwrap_or(end_dt);
            let from = start.format("%Y-%m-%d").to_string();
            let to = format!("{} 23:59:59", end);
            let label = format!("{from} ~ {end}");
            let segs = state.db.get_segments_in_range(&from, &to).map_err(|e| e.to_string())?;
            if segs.is_empty() {
                eprintln!("[REPORT] get_report_data(weekly) {from}~{end}: 0 segments");
            }
            ("周报", label, segs)
        }
        "monthly" => {
            let y = year.ok_or_else(|| "year is required for monthly report".to_string())?;
            let m = month.ok_or_else(|| "month is required for monthly report".to_string())?;
            let start = NaiveDate::from_ymd_opt(y, m, 1)
                .ok_or_else(|| format!("Invalid year/month: {y}-{m}"))?;
            let end = {
                if m == 12 {
                    NaiveDate::from_ymd_opt(y + 1, 1, 1)
                } else {
                    NaiveDate::from_ymd_opt(y, m + 1, 1)
                }
                .map(|d| d.pred_opt().unwrap_or(d))
                .unwrap_or(start)
            };
            let from = start.format("%Y-%m-%d").to_string();
            let to = format!("{} 23:59:59", end.format("%Y-%m-%d"));
            let label = format!("{y}年{m}月");
            let segs = state.db.get_segments_in_range(&from, &to).map_err(|e| e.to_string())?;
            if segs.is_empty() {
                eprintln!("[REPORT] get_report_data(monthly) {y}-{m}: 0 segments");
            }
            ("月报", label, segs)
        }
        _ => return Err(format!("Unknown report_type: {report_type}, expected 'daily', 'weekly', or 'monthly'")),
    };

    let merged = merge_adjacent(&segments);
    let total_secs: i64 = segments.iter().map(|s| s.duration_secs).sum();
    let breakdown = category_breakdown(&merged);
    let html = build_report_html(&title, &date_label, &merged, &breakdown, None);
    let md = build_markdown_report(&title, &date_label, &merged, &breakdown, None);

    // 检查 DB 中是否已有历史 AI 总结
    let has_existing_summary = match report_type {
        "daily" => {
            let d = date.unwrap_or_default();
            state.db.get_daily_summary(d)
                .ok()
                .flatten()
                .and_then(|s| s.llm_summary)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        }
        "weekly" | "monthly" => {
            state.db.get_period_summaries_by_type(report_type)
                .ok()
                .unwrap_or_default()
                .first()
                .and_then(|s| s.llm_summary.as_ref())
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        }
        _ => false,
    };

    Ok(ReportData {
        title: title.to_string(),
        date_label,
        total_seconds: total_secs,
        segments: merged,
        breakdown,
        html,
        markdown: md,
        has_existing_summary,
    })
}

// ---------------------------------------------------------------------------
// 新命令：get_report_data — 只查 DB，不调 LLM，秒级返回
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_report_data(
    state: State<'_, crate::AppState>,
    report_type: String,
    date: Option<String>,
    end_date: Option<String>,
    year: Option<i32>,
    month: Option<u32>,
) -> Result<ReportData, String> {
    eprintln!("[REPORT] get_report_data: type={report_type}, date={date:?}, end_date={end_date:?}, year={year:?}, month={month:?}");
    build_report_data(
        &state,
        &report_type,
        date.as_deref(),
        end_date.as_deref(),
        year,
        month,
    )
    .await
}

// ---------------------------------------------------------------------------
// 新命令：generate_report_summary — 仅调 LLM 生成总结并持久化
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn generate_report_summary(
    app_handle: tauri::AppHandle,
    state: State<'_, crate::AppState>,
    report_type: String,
    date: Option<String>,
    end_date: Option<String>,
    year: Option<i32>,
    month: Option<u32>,
) -> Result<String, String> {
    eprintln!("[REPORT] generate_report_summary: type={report_type}, date={date:?}, end_date={end_date:?}, year={year:?}, month={month:?}");

    // 1. 查询 segments
    let (segments, date_label_str) = match report_type.as_str() {
        "daily" => {
            let d = date.as_deref().ok_or_else(|| "date is required for daily summary".to_string())?;
            let segs = state.db.get_segments_by_date(d).map_err(|e| e.to_string())?;
            if segs.is_empty() {
                return Ok(format!("# {d} 日报\n\n当天没有记录到活动。"));
            }
            let merged = merge_adjacent(&segs);
            (merged, d.to_string())
        }
        "weekly" => {
            let end = end_date.as_deref().ok_or_else(|| "end_date is required for weekly summary".to_string())?;
            let end_dt = NaiveDate::parse_from_str(end, "%Y-%m-%d")
                .map_err(|e| format!("Invalid end_date format: {e}"))?;
            let start = end_dt.checked_sub_days(chrono::Days::new(6)).unwrap_or(end_dt);
            let from = start.format("%Y-%m-%d").to_string();
            let to = format!("{} 23:59:59", end);
            let label = format!("{from} ~ {end}");
            let segs = state.db.get_segments_in_range(&from, &to).map_err(|e| e.to_string())?;
            if segs.is_empty() {
                return Ok(format!("# {label} 周报\n\n该周期内没有记录到活动。"));
            }
            let merged = merge_adjacent(&segs);
            (merged, label)
        }
        "monthly" => {
            let y = year.ok_or_else(|| "year is required for monthly summary".to_string())?;
            let m = month.ok_or_else(|| "month is required for monthly summary".to_string())?;
            let start = NaiveDate::from_ymd_opt(y, m, 1)
                .ok_or_else(|| format!("Invalid year/month: {y}-{m}"))?;
            let end = {
                if m == 12 {
                    NaiveDate::from_ymd_opt(y + 1, 1, 1)
                } else {
                    NaiveDate::from_ymd_opt(y, m + 1, 1)
                }
                .map(|d| d.pred_opt().unwrap_or(d))
                .unwrap_or(start)
            };
            let from = start.format("%Y-%m-%d").to_string();
            let to = format!("{} 23:59:59", end.format("%Y-%m-%d"));
            let label = format!("{y}年{m}月");
            let segs = state.db.get_segments_in_range(&from, &to).map_err(|e| e.to_string())?;
            if segs.is_empty() {
                return Ok(format!("# {label} 月报\n\n该月没有记录到活动。"));
            }
            let merged = merge_adjacent(&segs);
            (merged, label)
        }
        _ => return Err(format!("Unknown report_type: {report_type}, expected 'daily', 'weekly', or 'monthly'")),
    };

    // 2. 选择 prompt
    let locale = crate::config::AppConfig::load_or_default().locale;
    let prompt = match report_type.as_str() {
        "daily" => crate::config::get_daily_summary_prompt(&locale),
        "weekly" => crate::config::get_weekly_summary_prompt(&locale),
        "monthly" => crate::config::get_monthly_summary_prompt(&locale),
        _ => return Err(format!("Unknown report_type: {report_type}")),
    };

    // 3. 调用 LLM
    let summary_text = generate_llm_summary(&state, &app_handle, &segments, prompt).await;

    if let Some(ref text) = summary_text {
        // 4. 持久化总结
        match report_type.as_str() {
            "daily" => {
                let d = date.as_deref().unwrap_or_default();
                // 获取已有 summary 或构造新的
                let existing = state.db.get_daily_summary(d).ok().flatten();
                let total_secs: i64 = segments.iter().map(|s| s.duration_secs).sum();
                let breakdown = category_breakdown(&segments);
                let break_json = serde_json::to_string(&breakdown).unwrap_or_default();
                let now_str = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let html = build_report_html("日报", d, &segments, &breakdown, Some(text));
                let summary = DailySummary {
                    id: existing.as_ref().map(|s| s.id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    date: d.to_string(),
                    total_seconds: total_secs,
                    segment_count: segments.len() as i64,
                    activity_breakdown: Some(break_json),
                    llm_summary: Some(text.clone()),
                    user_notes: existing.as_ref().and_then(|s| s.user_notes.clone()),
                    report_html: Some(html),
                    created_at: existing.as_ref().map(|s| s.created_at.clone()).unwrap_or_else(|| now_str.clone()),
                    updated_at: now_str,
                };
                let _ = state.db.upsert_daily_summary(&summary);
            }
            "weekly" | "monthly" => {
                let (from, to_str) = {
                    let end = end_date.as_deref().unwrap_or_default();
                    let end_dt = NaiveDate::parse_from_str(end, "%Y-%m-%d").ok();
                    let start = end_dt.and_then(|d| d.checked_sub_days(chrono::Days::new(6))).unwrap_or_default();
                    (start.format("%Y-%m-%d").to_string(), end.to_string())
                };
                // 对于月度，重新计算正确范围
                let (from_final, to_final) = if report_type == "monthly" {
                    let y = year.unwrap_or(0);
                    let m = month.unwrap_or(1);
                    let s = NaiveDate::from_ymd_opt(y, m, 1).unwrap_or_default();
                    let e = {
                        if m == 12 {
                            NaiveDate::from_ymd_opt(y + 1, 1, 1)
                        } else {
                            NaiveDate::from_ymd_opt(y, m + 1, 1)
                        }
                        .map(|d| d.pred_opt().unwrap_or(d))
                        .unwrap_or(s)
                    };
                    (s.format("%Y-%m-%d").to_string(), e.format("%Y-%m-%d").to_string())
                } else {
                    (from, to_str)
                };
                let total_secs: i64 = segments.iter().map(|s| s.duration_secs).sum();
                let daily_trend_json = serde_json::to_string(
                    &state.db.get_daily_trend(&from_final, &format!("{} 23:59:59", &to_final)).unwrap_or_default(),
                ).unwrap_or_default();
                let breakdown = category_breakdown(&segments);
                let break_json = serde_json::to_string(&breakdown).unwrap_or_default();
                let title = if report_type == "weekly" { "周报" } else { "月报" };
                let html = build_report_html(title, &date_label_str, &segments, &breakdown, Some(text));
                let summary = PeriodSummary {
                    id: uuid::Uuid::new_v4().to_string(),
                    r#type: report_type.clone(),
                    start_date: from_final,
                    end_date: to_final,
                    total_seconds: total_secs,
                    daily_trend: Some(daily_trend_json),
                    activity_breakdown: Some(break_json),
                    llm_summary: Some(text.clone()),
                    user_notes: None,
                    report_html: Some(html),
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };
                let _ = state.db.insert_period_summary(&summary);
            }
            _ => {}
        }
        Ok(text.clone())
    } else {
        // LLM 不可用（没配置/超时），返回空总结
        eprintln!("[REPORT] generate_report_summary: LLM unavailable, returning empty summary");
        Ok(String::new())
    }
}
