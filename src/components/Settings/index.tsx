import { useEffect, useState } from "react";
import { useConfigStore } from "../../stores/useConfigStore";
import type { LlmConfig } from "../../types";
import { useI18n } from "../../i18n";
import { api } from "../../lib/tauri";

type SettingsTab = "screenshot" | "llm" | "privacy";

const TAB_KEYS: Record<SettingsTab, string> = {
  screenshot: "settings.tab.screenshot",
  llm: "settings.tab.llm",
  privacy: "settings.tab.privacy",
};

export function Settings() {
  const { t } = useI18n();
  const {
    screenshotConfig,
    llmConfigs,
    activeLlmConfig,
    loadConfig,
    updateScreenshotConfig,
    saveLlmConfig,
    deleteLlmConfig,
    testLlmConnection,
  } = useConfigStore();

  const [activeTab, setActiveTab] = useState<SettingsTab>("screenshot");

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  return (
    <div className="settings-page">
      <div className="settings-tabs">
        {(Object.entries(TAB_KEYS) as [SettingsTab, string][]).map(
          ([key, tKey]) => (
            <button
              key={key}
              className={`tab-btn ${activeTab === key ? "active" : ""}`}
              onClick={() => setActiveTab(key)}
            >
              {t(tKey)}
            </button>
          ),
        )}
      </div>

      <div className="settings-content">
        {activeTab === "screenshot" && (
          <ScreenshotSettingsTab
            config={screenshotConfig}
            onSave={updateScreenshotConfig}
          />
        )}
        {activeTab === "llm" && (
          <LlmSettingsTab
            configs={llmConfigs}
            activeIdx={activeLlmConfig}
            onSave={saveLlmConfig}
            onDelete={deleteLlmConfig}
            onTest={testLlmConnection}
          />
        )}
        {activeTab === "privacy" && <PrivacySettingsTab />}
      </div>
    </div>
  );
}

// ── Screenshot Settings Tab ─────────────────────────────────────

function ScreenshotSettingsTab({
  config,
  onSave,
}: {
  config: { interval_secs: number; analysis_interval_secs: number; retention_days: number; include_cursor: boolean; capture_all_displays?: boolean };
  onSave: (c: { interval_secs: number; analysis_interval_secs: number; jpeg_quality: number; max_width: number; include_cursor: boolean; dedup_threshold: number; retention_days: number; capture_all_displays?: boolean }) => Promise<void>;
}) {
  const { t } = useI18n();
  const [interval, setInterval] = useState(config.interval_secs);
  const [analysisInterval, setAnalysisInterval] = useState(config.analysis_interval_secs);
  const [retention, setRetention] = useState(config.retention_days);
  const [includeCursor, setIncludeCursor] = useState(true);
  const [captureAllDisplays, setCaptureAllDisplays] = useState(false);
  const [saved, setSaved] = useState(false);

  const intervalOptions = [30, 60, 120, 300];
  const analysisOptions = [
    { value: 60, label: t('settings.interval.minute', { 'opt/60': 1 }) },
    { value: 120, label: t('settings.interval.minute', { 'opt/60': 2 }) },
    { value: 300, label: t('settings.interval.minute', { 'opt/60': 5 }) },
    { value: 600, label: t('settings.interval.minute', { 'opt/60': 10 }) },
    { value: 1800, label: t('settings.interval.minute', { 'opt/60': 30 }) },
  ];

  useEffect(() => {
    setInterval(config.interval_secs);
    setAnalysisInterval(config.analysis_interval_secs);
    setRetention(config.retention_days);
    setIncludeCursor(config.include_cursor);
    setCaptureAllDisplays(config.capture_all_displays ?? false);
  }, [config]);

  const handleSave = async () => {
    await onSave({
      interval_secs: interval,
      analysis_interval_secs: analysisInterval,
      jpeg_quality: 85,
      max_width: 1200,
      include_cursor: includeCursor,
      dedup_threshold: 5,
      retention_days: retention,
      capture_all_displays: captureAllDisplays,
    });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div className="settings-section">
      <h3>{t('settings.screenshot.freq')}</h3>
      <div className="radio-group">
        {intervalOptions.map((opt) => (
          <label key={opt} className="radio-label">
            <input
              type="radio"
              name="interval"
              checked={interval === opt}
              onChange={() => setInterval(opt)}
            />
            {opt < 60 ? t('settings.interval.second', { opt }) : t('settings.interval.minute', { 'opt/60': opt / 60 })}
          </label>
        ))}
        <label className="radio-label">
          <input
            type="radio"
            name="interval"
            checked={!intervalOptions.includes(interval)}
            onChange={() => setInterval(90)}
          />
          {t('settings.custom')}
          {!intervalOptions.includes(interval) && (
            <input
              type="number"
              value={interval}
              onChange={(e) => setInterval(Number(e.target.value))}
              className="input inline-input"
              min={10}
            />
          )}
        </label>
      </div>

      <h3>{t('settings.analysis.freq')}</h3>
      <p className="text-muted">{t('settings.analysis.desc')}</p>
      <div className="radio-group">
        {analysisOptions.map((opt) => (
          <label key={opt.value} className="radio-label">
            <input
              type="radio"
              name="analysisInterval"
              checked={analysisInterval === opt.value}
              onChange={() => setAnalysisInterval(opt.value)}
            />
            {opt.label}
          </label>
        ))}
        <label className="radio-label">
          <input
            type="radio"
            name="analysisInterval"
            checked={!analysisOptions.some((o) => o.value === analysisInterval)}
            onChange={() => setAnalysisInterval(analysisInterval || 90)}
          />
          {t('settings.custom')}
          {!analysisOptions.some((o) => o.value === analysisInterval) && (
            <input
              type="number"
              value={analysisInterval}
              onChange={(e) => setAnalysisInterval(Number(e.target.value))}
              className="input inline-input"
              min={interval}
              step={interval}
            />
          )}
        </label>
      </div>

      <h3>{t('settings.retention.days')}</h3>
      <select
        value={retention}
        onChange={(e) => setRetention(Number(e.target.value))}
        className="select"
      >
        <option value={3}>{t('settings.retention.day', { d: 3 })}</option>
        <option value={7}>{t('settings.retention.day', { d: 7 })}</option>
        <option value={14}>{t('settings.retention.day', { d: 14 })}</option>
        <option value={30}>{t('settings.retention.day', { d: 30 })}</option>
        <option value={90}>{t('settings.retention.day', { d: 90 })}</option>
      </select>

      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={includeCursor}
          onChange={(e) => setIncludeCursor(e.target.checked)}
        />
        {t('settings.includeCursor')}
      </label>

      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={captureAllDisplays}
          onChange={(e) => setCaptureAllDisplays(e.target.checked)}
        />
        {t('settings.captureAllDisplays')}
      </label>

      <button className="btn btn-primary" onClick={handleSave}>
        {saved ? t('settings.saved') : t('settings.saveBtn')}
      </button>
    </div>
  );
}

// ── LLM Settings Tab ────────────────────────────────────────────

function LlmSettingsTab({
  configs,
  activeIdx,
  onSave,
  onDelete,
  onTest,
}: {
  configs: LlmConfig[];
  activeIdx: number | null;
  onSave: (c: LlmConfig) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
  onTest: (c: LlmConfig) => Promise<boolean>;
}) {
  const { t, locale, setLocale } = useI18n();
  const [selectedIdx, setSelectedIdx] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [provider, setProvider] = useState("openai");
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("gpt-4o");
  const [apiKey, setApiKey] = useState("");
  const [maxTokens, setMaxTokens] = useState(4096);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [useBatchApi, setUseBatchApi] = useState(false);

  const currentConfig =
    selectedIdx !== null ? configs[selectedIdx] : null;

  // 当 configs 加载完成后，自动选中 active 或第一个配置
  useEffect(() => {
    if (configs.length === 0) return;
    // 已有选中项且对应的配置仍在列表中，不做切换以免覆盖用户操作
    if (selectedIdx !== null && configs[selectedIdx]) return;
    const idx = activeIdx !== null && activeIdx < configs.length ? activeIdx : 0;
    loadSelectedImpl(idx);
  }, [configs]);

  const loadSelectedImpl = (idx: number) => {
    const cfg = configs[idx];
    if (!cfg) return;
    setSelectedIdx(idx);
    setName(cfg.name);
    setProvider(cfg.provider);
    setBaseUrl(cfg.base_url);
    setModel(cfg.model);
    setApiKey(cfg.api_key ?? "");
    setMaxTokens(cfg.max_tokens);
    setUseBatchApi(cfg.use_batch_api ?? false);
    setTestResult(null);
  };

  const loadSelected = (idx: number) => {
    loadSelectedImpl(idx);
  };

  const handleSave = async () => {
    const config: LlmConfig = {
      id: currentConfig?.id,
      name,
      provider,
      base_url: baseUrl,
      model,
      api_key: apiKey || undefined,
      max_tokens: maxTokens,
      is_active: currentConfig?.is_active ?? false,
      use_batch_api: useBatchApi,
    };
    await onSave(config);
    setTestResult(null);
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    const config: LlmConfig = {
      name,
      provider,
      base_url: baseUrl,
      model,
      api_key: apiKey || undefined,
      max_tokens: maxTokens,
      is_active: false,
    };
    const ok = await onTest(config);
    setTestResult(ok ? t('settings.connected') : t('settings.connectFailed'));
    setTesting(false);
  };

  return (
    <div className="settings-section">
      <div className="language-switcher" style={{ display: 'flex', gap: '8px', justifyContent: 'flex-end', marginBottom: '12px' }}>
        <button
          className={`btn btn-sm ${locale === 'zh' ? 'btn-primary' : 'btn-ghost'}`}
          onClick={() => { setLocale('zh'); api.setLocale('zh'); }}
        >
          {t('settings.lang.zh')}
        </button>
        <button
          className={`btn btn-sm ${locale === 'en' ? 'btn-primary' : 'btn-ghost'}`}
          onClick={() => { setLocale('en'); api.setLocale('en'); }}
        >
          {t('settings.lang.en')}
        </button>
      </div>
      {configs.length > 0 && (
        <>
          <h3>{t('settings.currentConfig')}</h3>
          <select
            className="select"
            value={selectedIdx ?? ""}
            onChange={(e) => loadSelected(Number(e.target.value))}
          >
            <option value="" disabled>
              {t('settings.currentConfig')}...
            </option>
            {configs.map((c, i) => (
              <option key={c.id ?? i} value={i}>
                {c.name} {c.is_active ? `(${t('settings.currentConfig')})` : ""}
              </option>
            ))}
          </select>
        </>
      )}

      <h3>{currentConfig ? t('settings.editConfig') : `${t('settings.save')}...`}</h3>

      <div className="form-group">
        <label>{t('settings.configName')}</label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="input"
          placeholder={t('settings.placeholder.provider')}
        />
      </div>

      <div className="form-group">
        <label>{t('settings.apiType')}</label>
        <select
          value={provider}
          onChange={(e) => setProvider(e.target.value)}
          className="select"
        >
          <option value="openai">OpenAI</option>
          <option value="anthropic">Anthropic</option>
          <option value="ollama">Ollama</option>
          <option value="custom">{t('settings.provider.custom')}</option>
        </select>
      </div>

      <div className="form-group">
        <label>{t('settings.baseUrl')}</label>
        <input
          type="url"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          className="input"
          placeholder="https://api.openai.com/v1"
        />
      </div>

      <div className="form-group">
        <label>{t('settings.apiKey')}</label>
        <div className="input-row">
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            className="input"
            placeholder="sk-..."
          />
          <button
            className="btn btn-secondary"
            onClick={handleTest}
            disabled={testing || !apiKey}
          >
            {testing ? `${t('settings.test')}...` : t('settings.test')}
          </button>
        </div>
        {testResult && (
          <span
            className={`test-result ${testResult === t('settings.connected') ? "success" : "error"}`}
          >
            {testResult}
          </span>
        )}
      </div>

      <div className="form-group">
        <label>{t('settings.model')}</label>
        <input
          type="text"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          className="input"
          placeholder="gpt-4o / qwen2.5:7b"
        />
      </div>

      <div className="form-group">
        <label>{t('settings.maxTokensLabel', { n: maxTokens })}</label>
        <input
          type="range"
          min={256}
          max={16384}
          step={128}
          value={maxTokens}
          onChange={(e) => setMaxTokens(Number(e.target.value))}
          className="slider"
        />
      </div>

      <div className="form-group">
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={useBatchApi}
            onChange={(e) => setUseBatchApi(e.target.checked)}
          />
          🚀 {t('settings.batchApi.enable')}
        </label>
        <p className="text-muted">{t('settings.batchApi.desc')}</p>
      </div>

      <div className="form-actions">
        <button className="btn btn-primary" onClick={handleSave}>
          💾 {t('settings.save')}
        </button>
        {currentConfig && !currentConfig.is_active && (
          <button
            className="btn btn-secondary"
            onClick={async () => {
              await onSave({ ...currentConfig, is_active: true });
            }}
          >
            {t('settings.setDefault')}
          </button>
        )}
        {currentConfig && (
          <button
            className="btn btn-danger"
            onClick={() => {
              onDelete(currentConfig.name);
              setSelectedIdx(null);
            }}
          >
            🗑️ {t('settings.delete')}
          </button>
        )}
      </div>
    </div>
  );
}

// ── Privacy Settings Tab ────────────────────────────────────────

function PrivacySettingsTab() {
  const { t } = useI18n();
  const { privacyRules, addPrivacyRule, deletePrivacyRule } = useConfigStore();
  const [newPattern, setNewPattern] = useState("");
  const [newType, setNewType] = useState<"app" | "title">("app");

  const handleAdd = async () => {
    if (!newPattern.trim()) return;
    await addPrivacyRule({
      rule_type: newType === "app" ? "app_block" : "title_block",
      pattern: newPattern.trim(),
      is_active: true,
    });
    setNewPattern("");
  };

  const blockedApps = privacyRules.filter(
    (r) => r.rule_type === "app_block",
  );
  const blockedTitles = privacyRules.filter(
    (r) => r.rule_type === "title_block",
  );

  return (
    <div className="settings-section">
      <h3>{t('settings.privacy.appBlock')}</h3>
      <p className="text-muted">
        {t('settings.privacy.appBlockDesc')}
      </p>

      <div className="rules-list">
        {blockedApps.map((rule) => (
          <div key={rule.id} className="rule-item">
            <span>{rule.pattern}</span>
            <button
              className="btn btn-ghost btn-sm text-danger"
              onClick={() => rule.id !== undefined && deletePrivacyRule(rule.id)}
            >
              🗑️
            </button>
          </div>
        ))}
        {blockedApps.length === 0 && (
          <p className="text-muted">{t('settings.privacy.noAppBlock')}</p>
        )}
      </div>

      <h3>{t('settings.privacy.titleBlock')}</h3>
      <p className="text-muted">
        {t('settings.privacy.titleBlockDesc')}
      </p>

      <div className="rules-list">
        {blockedTitles.map((rule) => (
          <div key={rule.id} className="rule-item">
            <span>{rule.pattern}</span>
            <button
              className="btn btn-ghost btn-sm text-danger"
              onClick={() => rule.id !== undefined && deletePrivacyRule(rule.id)}
            >
              🗑️
            </button>
          </div>
        ))}
        {blockedTitles.length === 0 && (
          <p className="text-muted">{t('settings.privacy.noTitleBlock')}</p>
        )}
      </div>

      <h4>{t('settings.privacy.addRule')}</h4>
      <div className="input-row">
        <select
          value={newType}
          onChange={(e) => setNewType(e.target.value as "app" | "title")}
          className="select"
        >
          <option value="app">{t('settings.select.appBlock')}</option>
          <option value="title">{t('settings.select.titleBlock')}</option>
        </select>
        <input
          type="text"
          value={newPattern}
          onChange={(e) => setNewPattern(e.target.value)}
          placeholder={t('settings.privacy.placeholder')}
          className="input"
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
        />
        <button className="btn btn-primary btn-sm" onClick={handleAdd}>
          {t('settings.add')}
        </button>
      </div>
    </div>
  );
}
