import { useState, useEffect, useCallback } from 'react';
import { X, Settings, Key, Trash2, LogIn, CheckCircle, Loader2, Wrench, Server, Plus, AlertTriangle, ExternalLink, ChevronDown, ChevronRight, ShieldOff, ShieldCheck, Shield, Cpu, RefreshCw, Lock, Globe } from 'lucide-react';
import { SetupWizard } from '../setup/SetupWizard';
import { isMac, getPlatformInfo } from '../../lib/platform';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type { AppSettings } from '../../lib/settings';
import { DEFAULT_SETTINGS, detectCustomModels, testCustomEndpoint, testCustomEndpointViaProxy, startTranslationProxy, stopTranslationProxy, translationProxyStatus, type ProxyStatus } from '../../lib/settings';
import { getCachedModels, fetchAnthropicModels, groupAndSort, supportedEffortLevels, type ModelInfo } from '../../lib/models';
import {
  listPortkeyPresets, fetchPortkeyModels,
  groupPortkeyModelsByFamily, familyLabel, pickBestPortkeyModel,
  isAnthropicPortkeyModel,
  type PortkeyPreset, type PortkeyModel,
} from '../../lib/portkey';
import { getApiKey } from '../../lib/claude';
import type { MCPCatalogEntry, MCPServerConfig, MCPServerStatus, DependencyStatus } from '../../types/mcp';
import { getMCPCatalog, listMCPServers, enableMCPServer, disableMCPServer, installMCPServer, removeMCPServer, addMCPServer, checkMCPDependencies, updateMCPServerEnv } from '../../lib/mcp';
import { listInstalledExtensions, getExtensionConfigSchema, getExtensionSettings, updateExtensionSettings } from '../../lib/extensions';
import type { InstalledExtension } from '../../types/extensions';

// Extracted component to avoid useState inside map()
function CatalogServerCard({ server, entry, depCheck, isInstalling, onToggle, onError, onRefresh }: {
  server: MCPServerStatus;
  entry: MCPCatalogEntry | null;
  depCheck?: DependencyStatus;
  isInstalling: boolean;
  onToggle: () => void;
  onError: (msg: string) => void;
  onRefresh: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const enabled = server.config.enabled;

  // Env var editing — merge catalog defaults with saved values
  const catalogEnv = entry?.config.env || {};
  const savedEnv = server.config.env || {};
  const envKeys = Object.keys({ ...catalogEnv, ...savedEnv });
  const [envValues, setEnvValues] = useState<Record<string, string>>(() => {
    const merged: Record<string, string> = {};
    for (const k of envKeys) {
      merged[k] = savedEnv[k] || catalogEnv[k] || '';
    }
    return merged;
  });
  const [envSaving, setEnvSaving] = useState(false);
  const [envSaved, setEnvSaved] = useState(false);

  return (
    <div className={`rounded-lg border transition-colors ${
      enabled ? 'border-blue-800/40 bg-blue-950/10' : 'border-border-default bg-panel/40'
    }`}>
      {/* Main row */}
      <div className="flex items-start gap-3 px-3.5 py-3">
        {/* Icon */}
        <div className={`mt-0.5 p-1.5 rounded-md ${enabled ? 'bg-blue-900/30' : 'bg-surface/60'}`}>
          <Server className={`w-3.5 h-3.5 ${enabled ? 'text-blue-600 dark:text-blue-400' : 'text-muted'}`} />
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-[13px] font-medium text-primary">{entry?.name || server.config.name}</span>
            <span className={`text-[9px] font-medium uppercase tracking-wide px-1.5 py-[1px] rounded ${
              entry?.runtime === 'node'
                ? 'bg-green-900/30 text-green-600 dark:text-green-400 border border-green-800/30'
                : 'bg-yellow-900/30 text-yellow-600 dark:text-yellow-400 border border-yellow-800/30'
            }`}>
              {entry?.runtime === 'node' ? 'Node.js' : 'Python'}
            </span>
            {entry && (
              <span className="text-[10px] text-muted">{entry.tools_count} tools</span>
            )}
          </div>
          <p className="text-[11px] text-muted mt-1 leading-relaxed line-clamp-2">
            {entry?.description || server.config.description}
          </p>
        </div>

        {/* Toggle */}
        <div className="shrink-0 mt-0.5">
          {isInstalling ? (
            <Loader2 className="w-4 h-4 text-blue-600 dark:text-blue-400 animate-spin" />
          ) : (
            <button
              onClick={onToggle}
              className={`relative inline-flex items-center w-9 h-5 rounded-full transition-colors duration-200 ${
                enabled ? 'bg-blue-500' : 'bg-elevated'
              }`}
              aria-label={enabled ? 'Disable server' : 'Enable server'}
            >
              <span
                className={`inline-block w-3.5 h-3.5 rounded-full bg-white shadow transition-transform duration-200 ${
                  enabled ? 'translate-x-[18px]' : 'translate-x-[3px]'
                }`}
              />
            </button>
          )}
        </div>
      </div>

      {/* Details toggle */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-1 w-full px-3.5 py-1.5 text-[10px] text-muted hover:text-secondary transition-colors border-t border-border-default/40"
      >
        {expanded ? <ChevronDown className="w-2.5 h-2.5" /> : <ChevronRight className="w-2.5 h-2.5" />}
        Details
      </button>

      {/* Expanded details */}
      {expanded && entry && (
        <div className="px-3.5 pb-3 space-y-2.5">
          <div>
            <span className="text-[10px] text-secondary font-medium">Tools:</span>
            <div className="mt-1 flex flex-wrap gap-1">
              {entry.tools_summary.slice(0, 8).map((tool, i) => (
                <span key={i} className="text-[9px] text-secondary bg-surface/60 px-1.5 py-0.5 rounded font-mono">
                  {tool}
                </span>
              ))}
              {entry.tools_summary.length > 8 && (
                <span className="text-[9px] text-subtle px-1.5 py-0.5">+{entry.tools_summary.length - 8} more</span>
              )}
            </div>
          </div>
          {entry.databases.length > 0 && (
            <div>
              <span className="text-[10px] text-secondary font-medium">Databases:</span>
              <p className="text-[10px] text-muted mt-0.5 leading-relaxed">{entry.databases.join(' \u00b7 ')}</p>
            </div>
          )}
          <div className="flex items-center gap-3 pt-1">
            <span className="text-[10px] text-subtle">License: {entry.license}</span>
            {entry.homepage && (
              <a
                onClick={(e) => { e.preventDefault(); invoke('open_url', { url: entry.homepage }); }}
                href="#"
                className="flex items-center gap-1 text-[10px] text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-700"
              >
                <ExternalLink className="w-2.5 h-2.5" /> Homepage
              </a>
            )}
          </div>
          {/* Environment Variables (API keys etc.) */}
          {envKeys.length > 0 && (
            <div>
              <span className="text-[10px] text-secondary font-medium">Environment Variables:</span>
              <div className="mt-1.5 space-y-1.5">
                {envKeys.map((key) => (
                  <div key={key} className="flex items-center gap-2">
                    <span className="text-[10px] text-muted font-mono shrink-0 min-w-0 truncate" title={key}>
                      {key.replace(/_/g, '_\u200B')}
                    </span>
                    <input
                      type="password"
                      value={envValues[key] || ''}
                      onChange={(e) => setEnvValues(prev => ({ ...prev, [key]: e.target.value }))}
                      placeholder="Enter value..."
                      className="flex-1 px-2 py-1 bg-panel border border-border-strong rounded text-[11px] text-primary placeholder:text-subtle outline-none focus:border-blue-600/50 font-mono min-w-0"
                    />
                  </div>
                ))}
                <button
                  onClick={async () => {
                    setEnvSaving(true);
                    try {
                      // Filter out empty values
                      const filtered: Record<string, string> = {};
                      for (const [k, v] of Object.entries(envValues)) {
                        if (v.trim()) filtered[k] = v.trim();
                      }
                      await updateMCPServerEnv(server.config.name, filtered);
                      onRefresh();
                      setEnvSaved(true);
                      setTimeout(() => setEnvSaved(false), 6000);
                    } catch (e) {
                      onError(String(e));
                    }
                    setEnvSaving(false);
                  }}
                  disabled={envSaving}
                  className="text-[10px] px-2.5 py-1 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-white rounded transition-colors font-medium"
                >
                  {envSaving ? 'Saving...' : 'Save Keys'}
                </button>
                {envSaved && (
                  <p className="text-[10px] text-emerald-600 dark:text-emerald-400 mt-1">
                    Keys saved. Start a <strong>new chat session</strong> for changes to take effect.
                  </p>
                )}
              </div>
            </div>
          )}

          {depCheck && (
            <div className={`flex items-center gap-2 p-2 rounded-md text-[10px] ${
              depCheck.satisfied
                ? 'bg-green-950/20 text-green-600 dark:text-green-400 border border-green-900/20'
                : 'bg-yellow-950/20 text-yellow-600 dark:text-yellow-400 border border-yellow-900/20'
            }`}>
              {depCheck.satisfied ? (
                <><CheckCircle className="w-3 h-3 shrink-0" /> {depCheck.runtime} {depCheck.runtime_version}</>
              ) : (
                <><AlertTriangle className="w-3 h-3 shrink-0" /> {depCheck.install_hint}</>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── Portkey provider sub-panel ──
// Lives inside the Provider section. Renders the deployment dropdown
// (UCI ZotGPT / Portkey Cloud / self-hosted / custom), pre-fills the base
// URL from the picked preset, accepts the virtual key, and lets the user
// pick a model — auto-fetched from the gateway's /v1/models endpoint with
// a free-text fallback when fetch fails or for offline use.
function PortkeyProviderPanel({
  settings,
  saveSettings,
}: {
  settings: AppSettings;
  saveSettings: (s: AppSettings) => void;
}) {
  const [presets, setPresets] = useState<PortkeyPreset[]>([]);
  const [models, setModels] = useState<PortkeyModel[]>([]);
  const [fetchingModels, setFetchingModels] = useState(false);
  const [modelError, setModelError] = useState<string | null>(null);
  const [customSlug, setCustomSlug] = useState(false);

  useEffect(() => {
    listPortkeyPresets().then(setPresets).catch(() => {});
  }, []);

  const activePreset = presets.find((p) => p.id === settings.portkey_preset_id);

  const pickPreset = (id: string) => {
    const p = presets.find((x) => x.id === id);
    if (!p) return;
    // Pre-fill the base URL from the preset. Empty for self-hosted/custom so
    // the user types their own. Don't clobber a user-entered URL when they
    // re-select the same preset.
    //
    // Do NOT pre-save portkey_model here — let the live catalog auto-pick the
    // best model once the user pastes a virtual key (see the effect below).
    // Pre-saving from suggested_models[0] locks in a possibly-stale value
    // before the live catalog has a chance to show e.g. Opus 4.8.
    const next: AppSettings = {
      ...settings,
      portkey_preset_id: id,
      portkey_base_url: p.base_url || settings.portkey_base_url,
    };
    saveSettings(next);
    setModels([]);
    setModelError(null);
  };

  const refreshModels = async (opts?: { autoPickBest?: boolean }) => {
    setFetchingModels(true);
    setModelError(null);
    try {
      const fresh = await fetchPortkeyModels(
        settings.portkey_base_url,
        settings.portkey_api_key,
      );
      setModels(fresh);
      if (fresh.length === 0) {
        setModelError('Gateway returned 0 models — paste a slug manually below.');
      } else if (opts?.autoPickBest && !settings.portkey_model) {
        // First connect — auto-pick the best Claude in the catalog.
        const best = pickBestPortkeyModel(fresh.map((m) => m.id));
        if (best) saveSettings({ ...settings, portkey_model: best });
      }
    } catch (err) {
      setModelError(String(err));
    } finally {
      setFetchingModels(false);
    }
  };

  // Auto-fetch the catalog as soon as the user pastes a virtual key + has a
  // base URL. Debounced so we don't spam the gateway on every keystroke.
  useEffect(() => {
    const key = settings.portkey_api_key.trim();
    const base = settings.portkey_base_url.trim();
    if (!key || !base) return;
    if (models.length > 0) return;        // already have a catalog
    const t = setTimeout(() => {
      refreshModels({ autoPickBest: true }).catch(() => {});
    }, 600);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.portkey_api_key, settings.portkey_base_url]);

  // Auto start/stop the bundled translation proxy when the user picks a
  // non-Anthropic Portkey model. Portkey's Anthropic-passthrough surface
  // (/v1/messages) only handles Claude; Moonshot Kimi / GPT / Gemini must
  // come in via Portkey's OpenAI Chat-Completions endpoint, which Claude
  // Code can't speak directly. The bundled `anthropic-proxy` sidecar
  // translates Anthropic → OpenAI in-process on localhost.
  //
  // Why this is critical for Bedrock non-Anthropic models specifically:
  // Claude Code packs a JSON blob into `metadata.user_id` for telemetry.
  // Portkey's Anthropic→Bedrock translator copies that into Bedrock's
  // `requestMetadata`, whose regex `[a-zA-Z0-9\s:_@$#=/+,-.]` rejects
  // the JSON braces. The anthropic-proxy drops `metadata` entirely,
  // bypassing the issue. If the proxy fails to start the user gets a
  // confusing 400 — so surface start errors via setProxyError below
  // instead of silently swallowing them.
  //
  // Windows users: the sidecar is a no-op stub on Windows (the upstream
  // depends on Unix-only daemonize), so we surface a warning instead.
  const [proxyError, setProxyError] = useState<string | null>(null);

  // Authoritative backend capability flag: is the bundled Anthropic→OpenAI
  // translation proxy sidecar available on this platform? (macOS only.)
  // Replaces fragile isWindows/isLinux UA checks. Seed with the UA-derived
  // `isMac` value so the panel is correct before the backend reports in — and,
  // critically, so a transient `get_platform_info` rejection (e.g. a startup
  // race before the command is registered) degrades to the UA answer rather
  // than hard-disabling the proxy on macOS. `getPlatformInfo()` resets its
  // cached promise on failure, so this effect's later success still corrects
  // the value if the first call was the one that raced.
  const [translationProxySupported, setTranslationProxySupported] = useState(isMac);
  useEffect(() => {
    getPlatformInfo()
      .then((info) => setTranslationProxySupported(info.translationProxySupported))
      .catch(() => setTranslationProxySupported(isMac));
  }, []);

  useEffect(() => {
    const model = settings.portkey_model.trim();
    const base = settings.portkey_base_url.trim();
    const key = settings.portkey_api_key.trim();
    if (!model || !base || !key) return;
    if (isAnthropicPortkeyModel(model)) {
      setProxyError(null);
      stopTranslationProxy().catch(() => {});
      return;
    }
    if (!translationProxySupported) return;     // no proxy binary bundled for this platform; warning shown in UI
    setProxyError(null);
    startTranslationProxy(base, key).catch((err) => {
      const msg = String(err);
      setProxyError(msg);
      console.error('[portkey] startTranslationProxy failed:', msg);
    });
  }, [settings.portkey_model, settings.portkey_base_url, settings.portkey_api_key, translationProxySupported]);

  const hintModels: string[] = activePreset?.suggested_models ?? [];
  // Combined list: live-fetched (preferred) OR hint list (fallback). De-dup.
  const modelOptions: string[] = [
    ...new Set([...models.map((m) => m.id), ...hintModels]),
  ];
  // Group by family (Anthropic / Google / Moonshot / …) sorted best-first.
  // OpenAI is hidden until Phase 13 (native multi-provider agent loop) lands:
  // GPT-5+ / o-series need /v1/responses and tool-use semantics our current
  // anthropic-proxy can't translate. Non-reasoning OpenAI models (GPT-4o,
  // GPT-4.1) technically work via the proxy today, but we hide them too to
  // avoid "some OpenAI works, some doesn't" confusion. Re-enable by removing
  // this filter once Phase 13 ships.
  const groupedModels = groupPortkeyModelsByFamily(modelOptions).filter(
    ([family]) => family !== 'OpenAI',
  );

  return (
    <div className="space-y-4 border-t border-border-default pt-4">
      {/* Deployment dropdown */}
      <label className="block">
        <span className="block text-[11px] text-secondary mb-1">Gateway deployment</span>
        <select
          value={settings.portkey_preset_id || ''}
          onChange={(e) => pickPreset(e.target.value)}
          className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-emerald-600/50"
        >
          <option value="" disabled>
            Choose your gateway…
          </option>
          {presets.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        {activePreset && (
          <span className="block text-[10px] text-muted mt-1">{activePreset.description}</span>
        )}
      </label>

      {/* Preset-specific eligibility + signup links */}
      {activePreset && (activePreset.eligibility || activePreset.signup_url) && (
        <div className="bg-panel/50 border border-border-default rounded-md p-2.5 space-y-1.5 text-[11px]">
          {activePreset.eligibility && (
            <div className="text-secondary">
              <span className="text-muted">Eligibility · </span>
              {activePreset.eligibility}
            </div>
          )}
          <div className="flex items-center gap-3 text-[11px]">
            {activePreset.signup_url && (
              <button
                onClick={() =>
                  invoke('open_url', { url: activePreset.signup_url }).catch(() => {})
                }
                className="flex items-center gap-1 text-emerald-600 dark:text-emerald-400 hover:text-emerald-800 dark:hover:text-emerald-700"
              >
                <ExternalLink className="w-3 h-3 pointer-events-none" />
                Get an API key
              </button>
            )}
            {activePreset.docs_url && (
              <button
                onClick={() =>
                  invoke('open_url', { url: activePreset.docs_url }).catch(() => {})
                }
                className="flex items-center gap-1 text-secondary hover:text-secondary"
              >
                <ExternalLink className="w-3 h-3 pointer-events-none" />
                Docs
              </button>
            )}
          </div>
        </div>
      )}

      {/* Base URL — pre-filled from preset, editable for self-hosted/custom */}
      <label className="block">
        <span className="block text-[11px] text-secondary mb-1">Base URL</span>
        <input
          type="text"
          value={settings.portkey_base_url}
          onChange={(e) => saveSettings({ ...settings, portkey_base_url: e.target.value })}
          placeholder="https://api.zotgpt.uci.edu/v1"
          className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-emerald-600/50 font-mono"
        />
        <span className="block text-[10px] text-subtle mt-1">
          Should end in <span className="font-mono">/v1</span>. Portkey speaks Anthropic's
          <span className="font-mono"> /v1/messages</span> natively — works on Windows + HPC.
        </span>
      </label>

      {/* Virtual API key */}
      <label className="block">
        <span className="block text-[11px] text-secondary mb-1">Virtual API key</span>
        <input
          type="password"
          value={settings.portkey_api_key}
          onChange={(e) => saveSettings({ ...settings, portkey_api_key: e.target.value })}
          placeholder="pk-…"
          className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-emerald-600/50 font-mono"
        />
      </label>

      {/* Model selector */}
      <div className="space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-[11px] text-secondary">Model</span>
          <div className="flex items-center gap-2">
            <button
              onClick={() => refreshModels()}
              disabled={
                fetchingModels ||
                !settings.portkey_base_url.trim() ||
                !settings.portkey_api_key.trim()
              }
              className="flex items-center gap-1 px-1.5 py-0.5 text-[10px] text-secondary hover:text-primary disabled:opacity-40"
              title="Call /v1/models on the configured gateway"
            >
              <RefreshCw className={`w-3 h-3 ${fetchingModels ? 'animate-spin' : ''}`} />
              {fetchingModels ? 'Loading…' : 'Refresh catalog'}
            </button>
            <button
              onClick={() => setCustomSlug((v) => !v)}
              className="text-[10px] text-muted hover:text-secondary"
            >
              {customSlug ? 'Pick from list' : 'Paste slug'}
            </button>
          </div>
        </div>
        {customSlug ? (
          <input
            type="text"
            value={settings.portkey_model}
            onChange={(e) => saveSettings({ ...settings, portkey_model: e.target.value })}
            placeholder="@workspace/model-slug"
            className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-emerald-600/50 font-mono"
          />
        ) : (
          <select
            value={settings.portkey_model}
            onChange={(e) => saveSettings({ ...settings, portkey_model: e.target.value })}
            disabled={modelOptions.length === 0}
            className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-emerald-600/50 disabled:opacity-50"
          >
            {modelOptions.length === 0 ? (
              <option value="">
                {settings.portkey_model || 'Paste a virtual key — catalog loads automatically'}
              </option>
            ) : (
              <>
                {settings.portkey_model && !modelOptions.includes(settings.portkey_model) && (
                  <option value={settings.portkey_model}>
                    {settings.portkey_model} (saved)
                  </option>
                )}
                {groupedModels.map(([family, models]) => (
                  <optgroup key={family} label={familyLabel(family)}>
                    {models.map((m) => (
                      <option key={m.slug} value={m.slug}>
                        {m.display_name}
                        {m.workspace_hint ? `  ·  via ${m.workspace_hint}` : ''}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </>
            )}
          </select>
        )}
        {modelError && <div className="text-[10px] text-amber-600 dark:text-amber-400">{modelError}</div>}
        {models.length === 0 && hintModels.length > 0 && !modelError && !fetchingModels && (
          <div className="text-[10px] text-subtle">
            Showing suggested defaults for {activePreset?.label}. Paste your virtual key above to auto-load the live catalog.
          </div>
        )}
        {models.length > 0 && !modelError && (
          <div className="text-[10px] text-subtle">
            Grouped by model family · {models.length} models available · best-first
          </div>
        )}
        {settings.portkey_model && !isAnthropicPortkeyModel(settings.portkey_model) && (
          /^@[^/]+\/(?:gpt|o[1-9])/i.test(settings.portkey_model) ? (
            <div className="flex items-start gap-1.5 text-[10px] text-amber-600 dark:text-amber-400 leading-relaxed mt-1">
              <AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" />
              <span>
                OpenAI is temporarily disabled. GPT-5+/o-series need OpenAI's
                Responses API (which our translation proxy can't speak yet)
                and we're hiding all OpenAI models to avoid mixed-support
                confusion. Coming back in Phase 13 — native multi-provider
                agent loop. Pick a Claude, Kimi, or Gemini model meanwhile.
              </span>
            </div>
          ) : !translationProxySupported ? (
            <div className="flex items-start gap-1.5 text-[10px] text-amber-600 dark:text-amber-400 leading-relaxed mt-1">
              <AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" />
              <span>
                Non-Anthropic Portkey models need the local translation proxy,
                which isn't available on this platform. Pick a Claude model, or
                connect to a remote Anthropic-compatible endpoint
                (LiteLLM/OpenRouter) via the Custom provider instead.
              </span>
            </div>
          ) : proxyError ? (
            <div className="flex items-start gap-1.5 text-[10px] text-red-600 dark:text-red-400 leading-relaxed mt-1">
              <AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" />
              <span>
                Translation proxy failed to start: {proxyError}. Non-Anthropic
                Portkey models will fail until this is fixed.
              </span>
            </div>
          ) : (
            <div className="flex items-start gap-1.5 text-[10px] text-purple-600 dark:text-purple-400 leading-relaxed mt-1">
              <Server className="w-3 h-3 mt-0.5 shrink-0" />
              <span>
                Routed via local Anthropic→OpenAI translation proxy (Claude
                Code can't speak this model's API directly). Started
                automatically; tool-use quality may be lower than Claude.
              </span>
            </div>
          )
        )}
      </div>

      {/* Privacy footnote */}
      {activePreset?.privacy_summary && (
        <div className="flex items-start gap-1.5 text-[10px] text-muted leading-relaxed pt-1">
          <Lock className="w-3 h-3 mt-0.5 shrink-0 text-subtle" />
          <span>
            <span className="text-secondary">Privacy · </span>
            {activePreset.privacy_summary}
          </span>
        </div>
      )}
    </div>
  );
}

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
  initialSection?: string;
}

export function SettingsPanel({ isOpen, onClose, initialSection }: SettingsPanelProps) {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [apiKey, setApiKey] = useState('');
  const [hasKey, setHasKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [activeSection, setActiveSection] = useState<'editor' | 'terminal' | 'claude' | 'provider' | 'auth' | 'mcp' | 'extensions' | 'setup'>(
    'editor',
  );
  // AI provider local state (independent of settings persistence so we can show pending status)
  const [providerModels, setProviderModels] = useState<string[]>([]);
  const [providerDetecting, setProviderDetecting] = useState(false);
  const [providerTestResult, setProviderTestResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [providerTesting, setProviderTesting] = useState(false);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus>({ running: false, port: null, url: null, upstream_base_url: null });
  const [proxyBusy, setProxyBusy] = useState(false);
  const [proxyError, setProxyError] = useState<string | null>(null);
  const [showSetupWizard, setShowSetupWizard] = useState(false);
  const [authStatus, setAuthStatus] = useState<{ authenticated: boolean; method: string } | null>(null);
  const [oauthChecking, setOauthChecking] = useState(false);

  // MCP state
  const [mcpServers, setMcpServers] = useState<MCPServerStatus[]>([]);
  const [mcpCatalog, setMcpCatalog] = useState<MCPCatalogEntry[]>([]);
  const [mcpLoading, setMcpLoading] = useState(false);
  const [anthropicModels, setAnthropicModels] = useState<ModelInfo[]>([]);
  const [modelsRefreshing, setModelsRefreshing] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [mcpDepChecks, setMcpDepChecks] = useState<Record<string, DependencyStatus>>({});
  const [mcpInstalling, setMcpInstalling] = useState<string | null>(null);
  const [mcpError, setMcpError] = useState<string | null>(null);
  const [showCustomForm, setShowCustomForm] = useState(false);
  const [customServer, setCustomServer] = useState({ name: '', command: '', args: '' });

  // Extension Settings state
  const [installedExtensions, setInstalledExtensions] = useState<InstalledExtension[]>([]);
  const [extensionSettingsForms, setExtensionSettingsForms] = useState<Record<string, Record<string, unknown>>>({});
  const [extensionConfigSchemas, setExtensionConfigSchemas] = useState<Record<string, Record<string, unknown>>>({});
  const [extSettingsLoading, setExtSettingsLoading] = useState(false);

  const refreshAuthStatus = useCallback(async () => {
    try {
      const status = await invoke<{ authenticated: boolean; method: string }>('check_auth_status');
      setAuthStatus(status);
    } catch {
      setAuthStatus({ authenticated: false, method: 'none' });
    }
  }, []);

  const refreshMCPServers = useCallback(async () => {
    setMcpLoading(true);
    try {
      const [servers, catalog] = await Promise.all([listMCPServers(), getMCPCatalog()]);
      setMcpServers(servers);
      setMcpCatalog(catalog);
    } catch (e) {
      console.error('Failed to load MCP servers:', e);
    }
    setMcpLoading(false);
  }, []);

  const refreshExtensionSettings = useCallback(async () => {
    setExtSettingsLoading(true);
    try {
      const extensions = await listInstalledExtensions();
      const extensionsWithConfig = extensions.filter((ext) => ext.contributions.configuration);
      setInstalledExtensions(extensionsWithConfig);

      // Load config schemas and current settings for each extension
      const schemas: Record<string, Record<string, unknown>> = {};
      const forms: Record<string, Record<string, unknown>> = {};
      for (const ext of extensionsWithConfig) {
        try {
          const schema = await getExtensionConfigSchema(ext.id);
          const settings = await getExtensionSettings(ext.id);
          if (schema && typeof schema === 'object') {
            schemas[ext.id] = schema as Record<string, unknown>;
          }
          forms[ext.id] = settings || {};
        } catch (err) {
          console.warn(`Failed to load settings for extension ${ext.id}:`, err);
        }
      }
      setExtensionConfigSchemas(schemas);
      setExtensionSettingsForms(forms);
    } catch (e) {
      console.error('Failed to load extension settings:', e);
    }
    setExtSettingsLoading(false);
  }, []);

  useEffect(() => {
    if (isOpen) {
      invoke<AppSettings>('get_settings')
        .then(setSettings)
        .catch(() => setSettings(DEFAULT_SETTINGS));
      invoke<string | null>('get_api_key').then((key) => setHasKey(!!key));
      refreshAuthStatus();
      refreshMCPServers();
      refreshExtensionSettings();
      translationProxyStatus().then(setProxyStatus).catch(() => {});
      getCachedModels().then(setAnthropicModels).catch(() => {});
      // Jump to a specific section if the opener requested one
      if (initialSection) {
        setActiveSection(initialSection as typeof activeSection);
      }
    }
  }, [isOpen, initialSection, refreshAuthStatus, refreshMCPServers, refreshExtensionSettings]);

  const refreshAnthropicModels = useCallback(async () => {
    setModelsRefreshing(true);
    setModelsError(null);
    try {
      const key = await getApiKey();
      if (!key || !key.trim()) {
        setModelsError('Add your Anthropic API key in Auth to refresh from the live catalog.');
        return;
      }
      const fresh = await fetchAnthropicModels(key);
      setAnthropicModels(fresh);
      emit('models-refreshed', null).catch(() => {});
    } catch (err) {
      setModelsError(String(err));
    } finally {
      setModelsRefreshing(false);
    }
  }, []);

  // Poll proxy status while the Provider section is open so the status chip
  // stays accurate if the proxy exits on its own.
  useEffect(() => {
    if (!isOpen || activeSection !== 'provider') return;
    const handle = setInterval(() => {
      translationProxyStatus().then(setProxyStatus).catch(() => {});
    }, 2000);
    return () => clearInterval(handle);
  }, [isOpen, activeSection]);

  const saveSettings = useCallback(async (updated: AppSettings) => {
    setSaving(true);
    try {
      await invoke('update_settings', { settings: updated });
      setSettings(updated);
      // Notify other components (ChatPanel model picker, etc.) of the change.
      emit('app-settings-changed', updated).catch(() => {});
    } catch (err) {
      console.error('Failed to save settings:', err);
    }
    setSaving(false);
  }, []);

  const handleSaveApiKey = async () => {
    if (!apiKey.trim()) return;
    await invoke('store_api_key', { key: apiKey.trim() });
    setHasKey(true);
    setApiKey('');
  };

  const handleDeleteApiKey = async () => {
    await invoke('delete_api_key');
    setHasKey(false);
  };

  if (!isOpen) return null;

  const sections = [
    { id: 'editor' as const, label: 'Editor' },
    { id: 'terminal' as const, label: 'Terminal' },
    { id: 'claude' as const, label: 'Claude' },
    { id: 'provider' as const, label: 'AI Provider' },
    { id: 'auth' as const, label: 'Authentication' },
    { id: 'mcp' as const, label: 'MCP Servers' },
    { id: 'extensions' as const, label: 'Extension Settings' },
    { id: 'setup' as const, label: 'Setup Wizard' },
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/60" onClick={onClose} />
      <div className="relative w-[700px] max-h-[80vh] bg-panel rounded-xl border border-border-strong shadow-2xl flex overflow-hidden">
        {/* Sidebar */}
        <div className="w-[180px] border-r border-border-default py-3">
          <div className="flex items-center gap-2 px-4 pb-3 border-b border-border-default">
            <Settings className="w-4 h-4 text-secondary" />
            <span className="text-sm font-medium text-secondary">Settings</span>
          </div>
          <div className="py-2">
            {sections.map((section) => (
              <button
                key={section.id}
                onClick={() => setActiveSection(section.id)}
                className={`w-full text-left px-4 py-1.5 text-sm ${
                  activeSection === section.id
                    ? 'bg-surface text-primary'
                    : 'text-secondary hover:text-primary hover:bg-hover/50'
                }`}
              >
                {section.label}
              </button>
            ))}
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          <button
            onClick={onClose}
            className="absolute top-3 right-3 p-1 rounded hover:bg-hover text-muted hover:text-secondary"
          >
            <X className="w-4 h-4" />
          </button>

          {activeSection === 'editor' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-primary">Editor Settings</h3>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Font Size</span>
                <input
                  type="number"
                  value={settings.font_size}
                  onChange={(e) =>
                    saveSettings({ ...settings, font_size: parseInt(e.target.value) || 13 })
                  }
                  className="w-20 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Tab Size</span>
                <input
                  type="number"
                  value={settings.tab_size}
                  onChange={(e) =>
                    saveSettings({ ...settings, tab_size: parseInt(e.target.value) || 2 })
                  }
                  className="w-20 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Word Wrap</span>
                <input
                  type="checkbox"
                  checked={settings.word_wrap}
                  onChange={(e) => saveSettings({ ...settings, word_wrap: e.target.checked })}
                  className="w-4 h-4 accent-blue-500"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Minimap</span>
                <input
                  type="checkbox"
                  checked={settings.minimap_enabled}
                  onChange={(e) =>
                    saveSettings({ ...settings, minimap_enabled: e.target.checked })
                  }
                  className="w-4 h-4 accent-blue-500"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Show Hidden Files</span>
                <input
                  type="checkbox"
                  checked={settings.show_hidden_files}
                  onChange={(e) =>
                    saveSettings({ ...settings, show_hidden_files: e.target.checked })
                  }
                  className="w-4 h-4 accent-blue-500"
                />
              </label>
            </div>
          )}

          {activeSection === 'terminal' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-primary">Terminal Settings</h3>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Terminal Font Size</span>
                <input
                  type="number"
                  value={settings.terminal_font_size}
                  onChange={(e) =>
                    saveSettings({
                      ...settings,
                      terminal_font_size: parseInt(e.target.value) || 13,
                    })
                  }
                  className="w-20 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none"
                />
              </label>

              <label className="flex items-start justify-between gap-4">
                <div className="flex-1">
                  <div className="text-sm text-secondary">Use WebGL renderer</div>
                  <div className="text-xs text-muted mt-0.5">
                    Faster, but some GPU + external-display combinations
                    (e.g. Mac mini + Apple Studio Display scaled modes)
                    render glyphs with hairline artifacts. Turn off to use
                    the canvas renderer. Reopen the terminal tab to apply.
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={settings.terminal_use_webgl}
                  onChange={(e) =>
                    saveSettings({
                      ...settings,
                      terminal_use_webgl: e.target.checked,
                    })
                  }
                  className="mt-1 h-4 w-4 accent-blue-500"
                />
              </label>

              <label className="flex items-start justify-between gap-4">
                <div className="flex-1">
                  <div className="text-sm text-secondary">
                    Auto-wrap SSH in tmux
                  </div>
                  <div className="text-xs text-muted mt-0.5">
                    Wrap each new SSH terminal in a shared tmux session so
                    jobs keep running after Operon quits or your laptop
                    sleeps. No-op on hosts without tmux. Open a new terminal
                    to apply.
                  </div>
                </div>
                <input
                  type="checkbox"
                  checked={settings.ssh_auto_tmux}
                  onChange={(e) =>
                    saveSettings({
                      ...settings,
                      ssh_auto_tmux: e.target.checked,
                    })
                  }
                  className="mt-1 h-4 w-4 accent-blue-500"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">tmux session name</span>
                <input
                  type="text"
                  value={settings.ssh_tmux_session}
                  disabled={!settings.ssh_auto_tmux}
                  onChange={(e) =>
                    saveSettings({
                      ...settings,
                      ssh_tmux_session: e.target.value,
                    })
                  }
                  className="w-40 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none disabled:opacity-40"
                />
              </label>
            </div>
          )}

          {activeSection === 'claude' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-primary">Claude Code Settings</h3>

              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-sm text-secondary">Default Model</div>
                  {modelsError && (
                    <div className="text-xs text-amber-600 dark:text-amber-400 mt-1 max-w-xs">{modelsError}</div>
                  )}
                </div>
                <div className="flex items-center gap-1.5">
                  <select
                    value={settings.model}
                    onChange={(e) => saveSettings({ ...settings, model: e.target.value })}
                    className="w-56 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none"
                  >
                    {(() => {
                      const grouped = groupAndSort(anthropicModels);
                      const groups: Array<[string, ModelInfo[]]> = [
                        ['Opus', grouped.opus],
                        ['Sonnet', grouped.sonnet],
                        ['Haiku', grouped.haiku],
                        ['Other', grouped.other],
                      ];
                      // Make sure the saved model id is selectable even if it's
                      // not in the fetched list (e.g. user typed a custom id).
                      const ids = new Set(anthropicModels.map((m) => m.id));
                      const orphan = !ids.has(settings.model) ? settings.model : null;
                      return (
                        <>
                          {orphan && <option value={orphan}>{orphan} (saved)</option>}
                          {groups.map(([label, list]) =>
                            list.length === 0 ? null : (
                              <optgroup key={label} label={label}>
                                {list.map((m) => (
                                  <option key={m.id} value={m.id}>
                                    {m.display_name || m.id}
                                  </option>
                                ))}
                              </optgroup>
                            )
                          )}
                        </>
                      );
                    })()}
                  </select>
                  <button
                    type="button"
                    onClick={refreshAnthropicModels}
                    disabled={modelsRefreshing}
                    title="Refresh from api.anthropic.com/v1/models"
                    className="p-1.5 rounded hover:bg-hover text-secondary hover:text-primary disabled:opacity-40"
                  >
                    <RefreshCw className={`w-3.5 h-3.5 ${modelsRefreshing ? 'animate-spin' : ''}`} />
                  </button>
                </div>
              </div>

              {(() => {
                const currentModel = anthropicModels.find((m) => m.id === settings.model);
                const levels = supportedEffortLevels(currentModel);
                return (
                  <label className="flex items-center justify-between">
                    <div>
                      <div className="text-sm text-secondary">Reasoning Effort</div>
                      {levels.length === 0 && (
                        <div className="text-xs text-muted mt-0.5">Not supported by this model</div>
                      )}
                    </div>
                    <select
                      value={settings.effort}
                      onChange={(e) => saveSettings({ ...settings, effort: e.target.value })}
                      disabled={levels.length === 0}
                      className="w-56 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none disabled:opacity-40"
                    >
                      {levels.length === 0 ? (
                        <option value={settings.effort}>—</option>
                      ) : (
                        levels.map((lvl) => (
                          <option key={lvl} value={lvl}>{lvl}</option>
                        ))
                      )}
                    </select>
                  </label>
                );
              })()}

              <div className="flex items-center justify-between">
                <div className="pr-4">
                  <div className="text-sm text-secondary">Ultrathink</div>
                  <p className="text-[11px] text-muted mt-0.5 leading-relaxed">
                    Append the <code>ultrathink</code> keyword to every prompt, requesting Claude's maximum extended-thinking budget. Off by default — uses more tokens and is slower, but reasons harder on difficult problems.
                  </p>
                </div>
                <button
                  onClick={() => saveSettings({ ...settings, ultrathink: !settings.ultrathink })}
                  className={`shrink-0 relative inline-flex items-center w-9 h-5 rounded-full transition-colors duration-200 ${
                    settings.ultrathink ? 'bg-purple-500' : 'bg-elevated'
                  }`}
                  aria-label="Toggle ultrathink"
                >
                  <span className={`inline-block w-3.5 h-3.5 bg-white rounded-full transition-transform duration-200 ${
                    settings.ultrathink ? 'translate-x-5' : 'translate-x-0.5'
                  }`} />
                </button>
              </div>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Max Turns</span>
                <input
                  type="number"
                  value={settings.max_turns}
                  onChange={(e) =>
                    saveSettings({ ...settings, max_turns: parseInt(e.target.value) || 25 })
                  }
                  className="w-20 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-secondary">Max Budget (USD)</span>
                <input
                  type="number"
                  step="0.5"
                  value={settings.max_budget_usd}
                  onChange={(e) =>
                    saveSettings({
                      ...settings,
                      max_budget_usd: parseFloat(e.target.value) || 5.0,
                    })
                  }
                  className="w-20 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none"
                />
              </label>

              <label className="flex items-start justify-between gap-4">
                <span className="text-sm text-secondary flex-1">
                  Session Time Budget (min)
                  <span className="block text-[11px] text-muted mt-0.5">
                    Warn-only banner at 75% and 100%. Override per-session next to Send. 0 = off.
                  </span>
                </span>
                <input
                  type="number"
                  min={0}
                  step={5}
                  value={settings.session_time_budget_minutes}
                  onChange={(e) =>
                    saveSettings({
                      ...settings,
                      session_time_budget_minutes: Math.max(0, parseInt(e.target.value) || 0),
                    })
                  }
                  className="w-20 px-2 py-1 bg-surface border border-border-strong rounded text-sm text-primary outline-none"
                />
              </label>

              {/* Permission Level */}
              <div className="pt-3 border-t border-border-default">
                <div className="flex items-center gap-2 mb-3">
                  <Shield className="w-4 h-4 text-secondary" />
                  <span className="text-sm font-medium text-primary">Permission Level</span>
                </div>
                <div className="space-y-2">
                  {([
                    {
                      value: 'full_auto',
                      label: 'Full Auto',
                      desc: 'Claude reads, writes, and executes commands without asking. Fastest workflow.',
                      icon: ShieldOff,
                      color: 'text-amber-600 dark:text-amber-400',
                      border: settings.permission_mode === 'full_auto' ? 'border-amber-500/60 bg-amber-950/20' : 'border-border-strong/50 hover:border-border-strong',
                    },
                    {
                      value: 'safe_mode',
                      label: 'Safe Mode',
                      desc: 'Claude can read and search freely, but writes, edits, and bash commands require approval.',
                      icon: ShieldCheck,
                      color: 'text-blue-600 dark:text-blue-400',
                      border: settings.permission_mode === 'safe_mode' ? 'border-blue-500/60 bg-blue-950/20' : 'border-border-strong/50 hover:border-border-strong',
                    },
                    {
                      value: 'supervised',
                      label: 'Supervised',
                      desc: 'Claude asks permission for every action. Maximum control, slower workflow.',
                      icon: Shield,
                      color: 'text-green-600 dark:text-green-400',
                      border: settings.permission_mode === 'supervised' ? 'border-green-500/60 bg-green-950/20' : 'border-border-strong/50 hover:border-border-strong',
                    },
                  ] as const).map((opt) => {
                    const Icon = opt.icon;
                    const isActive = settings.permission_mode === opt.value;
                    return (
                      <button
                        key={opt.value}
                        onClick={() => saveSettings({ ...settings, permission_mode: opt.value })}
                        className={`w-full flex items-start gap-3 px-3 py-2.5 rounded-lg border transition-all text-left ${opt.border}`}
                      >
                        <Icon className={`w-4 h-4 mt-0.5 shrink-0 ${isActive ? opt.color : 'text-muted'}`} />
                        <div className="min-w-0">
                          <div className={`text-xs font-medium ${isActive ? 'text-primary' : 'text-secondary'}`}>
                            {opt.label}
                            {opt.value === 'full_auto' && (
                              <span className="ml-1.5 text-[10px] text-subtle font-normal">default</span>
                            )}
                          </div>
                          <div className="text-[11px] text-muted mt-0.5 leading-relaxed">{opt.desc}</div>
                        </div>
                        {isActive && (
                          <CheckCircle className={`w-3.5 h-3.5 mt-0.5 shrink-0 ml-auto ${opt.color}`} />
                        )}
                      </button>
                    );
                  })}
                </div>
                {settings.permission_mode === 'supervised' && (
                  <div className="mt-2 flex items-start gap-2 px-3 py-2 bg-yellow-950/20 border border-yellow-800/30 rounded text-[11px] text-yellow-600 dark:text-yellow-400/80">
                    <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                    <span>Supervised mode requires Claude Code to be authenticated interactively. Each action will prompt in the terminal.</span>
                  </div>
                )}
              </div>

              {/* HPC: keep Claude off the login node */}
              <div>
                <label className="text-xs text-secondary block mb-1.5">HPC login-node policy</label>
                <div className="text-[11px] text-muted mb-2 leading-relaxed">
                  Some clusters (e.g. UCI RCIC) automatically kill any Claude
                  process on a login node. With this on, Operon does not run
                  Claude auth/dependency checks on the login node — it assumes the
                  remote is set up (install and <code>claude login</code> once
                  during setup) and runs everyday agent work on the compute node,
                  where the agent surfaces any missing-Claude or expired-login
                  problem. Manual Install / Retry / Login still use the login node.
                </div>
                <label className="flex items-center gap-2 text-xs text-secondary">
                  <input
                    type="checkbox"
                    checked={settings.hpc_restrict_login_node !== false}
                    onChange={(e) => saveSettings({ ...settings, hpc_restrict_login_node: e.target.checked })}
                    className="accent-blue-500"
                  />
                  Restrict Claude to interactive / compute nodes (recommended for HPC)
                </label>
              </div>

              {/* Light code reviewer */}
              <div>
                <label className="text-xs text-secondary block mb-1.5">Code reviewer</label>
                <div className="text-[11px] text-muted mb-2 leading-relaxed">
                  A cheap second opinion on analysis code and sbatch scripts, run on a{' '}
                  <em>different</em> model with a fresh context (a model grading its own
                  conversation is the weakest possible check). Catches things like scVI fit on
                  log-normalised data, cluster-then-DE circularity, cell-level tests that ignore
                  donors, and outputs written to node-local <code>/tmp</code>. Advisory only — it
                  never edits your code, and you can always submit anyway.
                </div>
                <label className="flex items-center gap-2 text-xs text-secondary mb-2">
                  <input
                    type="checkbox"
                    checked={settings.reviewer_enabled !== false}
                    onChange={(e) => saveSettings({ ...settings, reviewer_enabled: e.target.checked })}
                    className="accent-blue-500"
                  />
                  Enable code reviewer
                </label>
                <label className="flex items-center gap-2 text-xs text-secondary mb-2">
                  <input
                    type="checkbox"
                    checked={settings.reviewer_auto_sbatch !== false}
                    disabled={settings.reviewer_enabled === false}
                    onChange={(e) => saveSettings({ ...settings, reviewer_auto_sbatch: e.target.checked })}
                    className="accent-blue-500"
                  />
                  Auto-review sbatch scripts before submitting
                </label>
                {(() => {
                  const rid = settings.reviewer_model || 'claude-sonnet-5';
                  const rm = anthropicModels.find((m) => m.id === rid);
                  const levels = supportedEffortLevels(rm);
                  const opts = levels.length ? levels : ['low'];
                  return (
                    <div className="flex gap-2">
                      <div className="flex-1 min-w-0">
                        <label className="text-[10px] text-muted block mb-1">Reviewer model</label>
                        <select
                          value={rid}
                          disabled={settings.reviewer_enabled === false}
                          onChange={(e) => saveSettings({ ...settings, reviewer_model: e.target.value })}
                          className="w-full bg-surface border border-border-strong rounded px-2 py-1 text-xs text-primary disabled:opacity-50"
                        >
                          {anthropicModels.length === 0 && <option value={rid}>{rid}</option>}
                          {anthropicModels.map((m) => (
                            <option key={m.id} value={m.id}>
                              {m.display_name || m.id}
                            </option>
                          ))}
                        </select>
                      </div>
                      <div className="w-24 shrink-0">
                        <label className="text-[10px] text-muted block mb-1">Effort</label>
                        <select
                          value={settings.reviewer_effort || 'low'}
                          disabled={settings.reviewer_enabled === false}
                          onChange={(e) => saveSettings({ ...settings, reviewer_effort: e.target.value })}
                          className="w-full bg-surface border border-border-strong rounded px-2 py-1 text-xs text-primary disabled:opacity-50"
                        >
                          {opts.map((lv) => (
                            <option key={lv} value={lv}>
                              {lv}
                            </option>
                          ))}
                        </select>
                      </div>
                    </div>
                  );
                })()}
              </div>
            </div>
          )}

          {activeSection === 'provider' && (
            <div className="space-y-5">
              <div>
                <h3 className="text-sm font-medium text-primary">AI Provider</h3>
                <p className="text-[11px] text-muted mt-1 leading-relaxed">
                  Route Claude Code to other models — OpenAI, Gemini, open-weights, and more. The recommended way is an Anthropic-compatible gateway (OpenRouter or LiteLLM): Claude Code calls it directly, with no proxy, and it works everywhere — including Windows and remote/HPC sessions.
                </p>
              </div>

              {/* Provider choice */}
              <div className="grid grid-cols-3 gap-2">
                <button
                  onClick={() => saveSettings({ ...settings, ai_provider: 'anthropic' })}
                  className={`p-3 rounded-lg border text-left transition-all ${
                    settings.ai_provider === 'anthropic'
                      ? 'border-blue-500/60 bg-blue-950/20'
                      : 'border-border-strong/50 hover:border-border-strong'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <CheckCircle className={`w-3.5 h-3.5 ${settings.ai_provider === 'anthropic' ? 'text-blue-600 dark:text-blue-400' : 'text-subtle'}`} />
                    <span className="text-xs font-medium text-primary">Anthropic</span>
                    <span className="ml-auto text-[10px] text-muted">default</span>
                  </div>
                  <p className="text-[11px] text-muted mt-1">Hosted Claude API. Best tool-use quality.</p>
                </button>

                <button
                  onClick={() => saveSettings({ ...settings, ai_provider: 'portkey' })}
                  className={`p-3 rounded-lg border text-left transition-all ${
                    settings.ai_provider === 'portkey'
                      ? 'border-emerald-500/60 bg-emerald-950/20'
                      : 'border-border-strong/50 hover:border-border-strong'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <Shield className={`w-3.5 h-3.5 ${settings.ai_provider === 'portkey' ? 'text-emerald-600 dark:text-emerald-400' : 'text-subtle'}`} />
                    <span className="text-xs font-medium text-primary">Portkey gateway</span>
                    <span className="ml-auto text-[10px] text-muted">institutional</span>
                  </div>
                  <p className="text-[11px] text-muted mt-1">UCI ZotGPT, Portkey Cloud, or your own.</p>
                </button>

                <button
                  onClick={() => saveSettings({ ...settings, ai_provider: 'custom' })}
                  className={`p-3 rounded-lg border text-left transition-all ${
                    settings.ai_provider === 'custom'
                      ? 'border-purple-500/60 bg-purple-950/20'
                      : 'border-border-strong/50 hover:border-border-strong'
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <Cpu className={`w-3.5 h-3.5 ${settings.ai_provider === 'custom' ? 'text-purple-600 dark:text-purple-400' : 'text-subtle'}`} />
                    <span className="text-xs font-medium text-primary">Custom provider</span>
                    <span className="ml-auto text-[10px] text-muted">advanced</span>
                  </div>
                  <p className="text-[11px] text-muted mt-1">OpenAI/Gemini/open-weights via gateway or local.</p>
                </button>
              </div>

              {settings.ai_provider === 'portkey' && (
                <PortkeyProviderPanel settings={settings} saveSettings={saveSettings} />
              )}

              {settings.ai_provider === 'custom' && (
                <div className="space-y-4 border-t border-border-default pt-4">
                  {/* Presets — grouped by route. Clicking a gateway preset turns
                      the translation proxy OFF; a local-runtime preset turns it ON. */}
                  <div className="space-y-2.5">
                    <div>
                      <span className="block text-[11px] text-secondary mb-1">
                        Gateways <span className="text-green-500/70">· recommended — no proxy</span>
                      </span>
                      <div className="flex flex-wrap gap-1.5">
                        {([
                          { label: 'OpenRouter', url: 'https://openrouter.ai/api/v1' },
                          { label: 'LiteLLM', url: 'http://localhost:4000/v1' },
                        ] as const).map((p) => (
                          <button
                            key={p.label}
                            onClick={() => saveSettings({ ...settings, custom_base_url: p.url, use_translation_proxy: false })}
                            className="px-2 py-1 text-[11px] bg-surface hover:bg-elevated text-secondary rounded border border-border-strong"
                          >
                            {p.label}
                          </button>
                        ))}
                      </div>
                    </div>
                    <div>
                      <span className="block text-[11px] text-secondary mb-1">
                        Local runtimes <span className="text-amber-500/70">· needs translation proxy</span>
                      </span>
                      <div className="flex flex-wrap gap-1.5">
                        {([
                          { label: 'Ollama', url: 'http://localhost:11434/v1' },
                          { label: 'LM Studio', url: 'http://localhost:1234/v1' },
                          { label: 'vLLM', url: 'http://localhost:8000/v1' },
                        ] as const).map((p) => (
                          <button
                            key={p.label}
                            onClick={() => saveSettings({ ...settings, custom_base_url: p.url, use_translation_proxy: true })}
                            className="px-2 py-1 text-[11px] bg-surface hover:bg-elevated text-secondary rounded border border-border-strong"
                          >
                            {p.label}
                          </button>
                        ))}
                      </div>
                    </div>
                  </div>

                  {/* Base URL */}
                  <label className="block">
                    <span className="block text-[11px] text-secondary mb-1">Base URL</span>
                    <input
                      type="text"
                      value={settings.custom_base_url}
                      onChange={(e) => saveSettings({ ...settings, custom_base_url: e.target.value })}
                      placeholder="http://localhost:11434/v1"
                      className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-purple-600/50 font-mono"
                    />
                    <span className="block text-[10px] text-subtle mt-1">The URL of your OpenAI-compatible endpoint. Must end in /v1 (or equivalent).</span>
                  </label>

                  {/* API key */}
                  <label className="block">
                    <span className="block text-[11px] text-secondary mb-1">API key <span className="text-subtle">(optional for local endpoints)</span></span>
                    <input
                      type="password"
                      value={settings.custom_api_key}
                      onChange={(e) => saveSettings({ ...settings, custom_api_key: e.target.value })}
                      placeholder="sk-…"
                      className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-purple-600/50 font-mono"
                    />
                  </label>

                  {/* Model picker */}
                  <label className="block">
                    <span className="flex items-center justify-between mb-1">
                      <span className="text-[11px] text-secondary">Default model</span>
                      <button
                        onClick={async () => {
                          setProviderDetecting(true);
                          setProviderModels([]);
                          try {
                            const models = await detectCustomModels(settings.custom_base_url, settings.custom_api_key || undefined);
                            setProviderModels(models);
                            // Auto-select first model if none chosen yet
                            if (!settings.custom_model && models.length > 0) {
                              saveSettings({ ...settings, custom_model: models[0] });
                            }
                          } catch (e: any) {
                            setProviderTestResult({ ok: false, msg: `Detect failed: ${e}` });
                          } finally {
                            setProviderDetecting(false);
                          }
                        }}
                        disabled={!settings.custom_base_url.trim() || providerDetecting}
                        className="flex items-center gap-1 px-1.5 py-0.5 text-[10px] text-purple-700 dark:text-purple-300 hover:text-purple-200 disabled:text-subtle disabled:cursor-not-allowed"
                      >
                        {providerDetecting ? <Loader2 className="w-3 h-3 animate-spin" /> : <RefreshCw className="w-3 h-3" />}
                        Detect models
                      </button>
                    </span>
                    {providerModels.length > 0 ? (
                      <select
                        value={settings.custom_model}
                        onChange={(e) => saveSettings({ ...settings, custom_model: e.target.value })}
                        className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-purple-600/50 font-mono"
                      >
                        {providerModels.map((m) => <option key={m} value={m}>{m}</option>)}
                      </select>
                    ) : (
                      <input
                        type="text"
                        value={settings.custom_model}
                        onChange={(e) => saveSettings({ ...settings, custom_model: e.target.value })}
                        placeholder="qwen2.5-coder:32b"
                        className="w-full px-2 py-1.5 bg-surface border border-border-strong rounded text-xs text-primary outline-none focus:border-purple-600/50 font-mono"
                      />
                    )}
                  </label>

                  {/* Test connection */}
                  <div>
                    <button
                      onClick={async () => {
                        setProviderTesting(true);
                        setProviderTestResult(null);
                        try {
                          // When the proxy is enabled, test the REAL chat path
                          // (Anthropic → proxy → endpoint), not just the raw
                          // OpenAI surface — otherwise this passes while chats
                          // still fail because the proxy is down.
                          const msg = settings.use_translation_proxy
                            ? await testCustomEndpointViaProxy(
                                settings.custom_base_url,
                                settings.custom_api_key || undefined,
                                settings.custom_model,
                              )
                            : await testCustomEndpoint(
                                settings.custom_base_url,
                                settings.custom_api_key || undefined,
                                settings.custom_model,
                              );
                          setProviderTestResult({ ok: true, msg });
                          // The via-proxy test starts the proxy — refresh the chip.
                          if (settings.use_translation_proxy) {
                            translationProxyStatus().then(setProxyStatus).catch(() => {});
                          }
                        } catch (e: any) {
                          setProviderTestResult({ ok: false, msg: String(e) });
                        } finally {
                          setProviderTesting(false);
                        }
                      }}
                      disabled={!settings.custom_base_url.trim() || !settings.custom_model || providerTesting}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-purple-600 hover:bg-purple-500 disabled:bg-elevated disabled:text-muted text-white text-xs font-medium rounded-md"
                    >
                      {providerTesting ? <Loader2 className="w-3 h-3 animate-spin" /> : <CheckCircle className="w-3 h-3" />}
                      Test connection
                    </button>
                    {providerTestResult && (
                      <div className={`mt-2 px-3 py-2 rounded text-[11px] flex items-start gap-2 ${
                        providerTestResult.ok
                          ? 'bg-green-950/30 border border-green-800/40 text-green-700 dark:text-green-300'
                          : 'bg-red-950/30 border border-red-800/40 text-red-700 dark:text-red-300'
                      }`}>
                        {providerTestResult.ok ? <CheckCircle className="w-3.5 h-3.5 shrink-0 mt-0.5" /> : <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />}
                        <span className="break-all">{providerTestResult.msg}</span>
                      </div>
                    )}
                  </div>

                  {/* Translation proxy — Anthropic ↔ OpenAI bridge */}
                  <div className="px-3 py-3 bg-panel/60 border border-border-default rounded space-y-2.5">
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="text-[12px] font-medium text-primary flex items-center gap-2">
                          Translation proxy
                          {proxyStatus.running ? (
                            <span className="inline-flex items-center gap-1 px-1.5 py-[1px] rounded text-[10px] font-mono bg-green-900/30 border border-green-800/40 text-green-600 dark:text-green-400">
                              <span className="w-1.5 h-1.5 rounded-full bg-green-400" />
                              running :{proxyStatus.port}
                            </span>
                          ) : (
                            <span className="inline-flex items-center gap-1 px-1.5 py-[1px] rounded text-[10px] font-mono bg-surface border border-border-strong text-muted">
                              <span className="w-1.5 h-1.5 rounded-full bg-muted" />
                              stopped
                            </span>
                          )}
                        </div>
                        <p className="text-[11px] text-muted mt-1 leading-relaxed">
                          Optional — leave OFF for the recommended gateway route. Turn it on only when your endpoint speaks OpenAI Chat Completions but not the Anthropic Messages API: Ollama, vLLM, and LM Studio older than 0.4.1. The proxy runs locally, so it is not available on Windows or for remote/HPC sessions.
                        </p>
                      </div>
                      <button
                        onClick={() => saveSettings({ ...settings, use_translation_proxy: !settings.use_translation_proxy })}
                        className={`shrink-0 relative inline-flex items-center w-9 h-5 rounded-full transition-colors duration-200 ${
                          settings.use_translation_proxy ? 'bg-purple-500' : 'bg-elevated'
                        }`}
                        aria-label="Toggle translation proxy"
                      >
                        <span className={`inline-block w-3.5 h-3.5 bg-white rounded-full transition-transform duration-200 ${
                          settings.use_translation_proxy ? 'translate-x-5' : 'translate-x-0.5'
                        }`} />
                      </button>
                    </div>

                    {settings.use_translation_proxy && (
                      <div className="flex items-center gap-2">
                        <button
                          onClick={async () => {
                            setProxyBusy(true);
                            setProxyError(null);
                            try {
                              await startTranslationProxy(settings.custom_base_url, settings.custom_api_key || undefined);
                              setProxyStatus(await translationProxyStatus());
                            } catch (e: unknown) {
                              setProxyError(String(e));
                            } finally {
                              setProxyBusy(false);
                            }
                          }}
                          disabled={!settings.custom_base_url.trim() || proxyBusy}
                          className="flex items-center gap-1.5 px-3 py-1.5 bg-surface hover:bg-elevated disabled:bg-surface/50 disabled:text-subtle text-primary text-[11px] font-medium rounded-md border border-border-strong"
                        >
                          {proxyBusy ? <Loader2 className="w-3 h-3 animate-spin" /> : <RefreshCw className="w-3 h-3" />}
                          {proxyStatus.running ? 'Restart proxy' : 'Start proxy'}
                        </button>
                        {proxyStatus.running && (
                          <button
                            onClick={async () => {
                              setProxyBusy(true);
                              setProxyError(null);
                              try {
                                await stopTranslationProxy();
                                setProxyStatus(await translationProxyStatus());
                              } catch (e: unknown) {
                                setProxyError(String(e));
                              } finally {
                                setProxyBusy(false);
                              }
                            }}
                            disabled={proxyBusy}
                            className="flex items-center gap-1.5 px-3 py-1.5 bg-surface hover:bg-elevated text-secondary text-[11px] font-medium rounded-md border border-border-strong"
                          >
                            Stop
                          </button>
                        )}
                        {proxyStatus.running && (
                          <span className="text-[10px] font-mono text-muted truncate">
                            → {proxyStatus.upstream_base_url}
                          </span>
                        )}
                      </div>
                    )}

                    {proxyError && (
                      <div className="px-2.5 py-1.5 bg-red-950/30 border border-red-800/40 rounded text-[11px] text-red-700 dark:text-red-300 flex items-start gap-2">
                        <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                        <span className="break-all">{proxyError}</span>
                      </div>
                    )}
                  </div>

                  {/* Caveats */}
                  <div className="px-3 py-2 bg-amber-950/20 border border-amber-800/30 rounded text-[11px] text-amber-600 dark:text-amber-400/80 flex items-start gap-2">
                    <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                    <div>
                      Agentic tool-use quality drops sharply below ~30B models. Extended thinking and some tools may behave differently across backends. Remote (SSH/tmux) sessions require a manual `ssh -R {proxyStatus.port ?? '<port>'}:127.0.0.1:{proxyStatus.port ?? '<port>'}` reverse tunnel — auto-tunneling for remote modes is not yet wired in this release.
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}

          {activeSection === 'mcp' && (
            <div className="space-y-5">
              <div>
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-medium text-primary">MCP Servers</h3>
                  {mcpLoading && <Loader2 className="w-3.5 h-3.5 text-muted animate-spin" />}
                </div>
                <p className="text-[11px] text-muted mt-1.5 leading-relaxed">
                  MCP servers give Claude access to external tools and databases.
                  Enabled servers are automatically available in all Claude sessions.
                </p>
              </div>

              {mcpError && (
                <div className="flex items-center gap-2 p-2.5 bg-red-950/20 border border-red-900/30 rounded-lg">
                  <AlertTriangle className="w-3.5 h-3.5 text-red-600 dark:text-red-400 shrink-0" />
                  <span className="text-[11px] text-red-700 dark:text-red-300">{mcpError}</span>
                  <button onClick={() => setMcpError(null)} className="ml-auto text-subtle hover:text-secondary">
                    <X className="w-3 h-3" />
                  </button>
                </div>
              )}

              {/* Catalog Servers */}
              <div className="space-y-2.5">
                <h4 className="text-[10px] font-semibold text-muted uppercase tracking-wider">Research Tools Catalog</h4>
                {mcpServers.filter(s => s.from_catalog).map((server) => (
                  <CatalogServerCard
                    key={server.config.name}
                    server={server}
                    entry={server.catalog_entry}
                    depCheck={mcpDepChecks[server.config.name]}
                    isInstalling={mcpInstalling === server.config.name}
                    onError={setMcpError}
                    onRefresh={refreshMCPServers}
                    onToggle={async () => {
                      setMcpError(null);
                      if (server.config.enabled) {
                        try {
                          await disableMCPServer(server.config.name);
                          await refreshMCPServers();
                        } catch (e) {
                          setMcpError(String(e));
                        }
                      } else {
                        setMcpInstalling(server.config.name);
                        try {
                          const dep = await checkMCPDependencies(server.config.name);
                          setMcpDepChecks(prev => ({ ...prev, [server.config.name]: dep }));
                          if (dep.satisfied && server.catalog_entry) {
                            await installMCPServer(server.catalog_entry.id);
                            await refreshMCPServers();
                          } else {
                            setMcpError(`${server.catalog_entry?.runtime || 'Runtime'} not found. ${dep.install_hint}`);
                          }
                        } catch (e) {
                          setMcpError(String(e));
                        }
                        setMcpInstalling(null);
                      }
                    }}
                  />
                ))}
              </div>

              {/* Custom Servers */}
              <div className="space-y-2.5">
                <div className="flex items-center justify-between">
                  <h4 className="text-[10px] font-semibold text-muted uppercase tracking-wider">Custom Servers</h4>
                  <button
                    onClick={() => setShowCustomForm(!showCustomForm)}
                    className="flex items-center gap-1 text-[10px] text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-700 transition-colors"
                  >
                    <Plus className="w-3 h-3" /> Add
                  </button>
                </div>

                {showCustomForm && (
                  <div className="p-3 bg-surface/50 border border-border-strong rounded-lg space-y-2">
                    <input
                      type="text"
                      value={customServer.name}
                      onChange={(e) => setCustomServer(prev => ({ ...prev, name: e.target.value }))}
                      placeholder="Server name"
                      className="w-full px-2 py-1 bg-panel border border-border-strong rounded text-sm text-primary placeholder:text-subtle outline-none"
                    />
                    <input
                      type="text"
                      value={customServer.command}
                      onChange={(e) => setCustomServer(prev => ({ ...prev, command: e.target.value }))}
                      placeholder="Command (e.g. npx, uvx, node)"
                      className="w-full px-2 py-1 bg-panel border border-border-strong rounded text-sm text-primary placeholder:text-subtle outline-none"
                    />
                    <input
                      type="text"
                      value={customServer.args}
                      onChange={(e) => setCustomServer(prev => ({ ...prev, args: e.target.value }))}
                      placeholder="Arguments (space-separated)"
                      className="w-full px-2 py-1 bg-panel border border-border-strong rounded text-sm text-primary placeholder:text-subtle outline-none"
                    />
                    <div className="flex gap-2 pt-1">
                      <button
                        onClick={async () => {
                          if (!customServer.name.trim() || !customServer.command.trim()) return;
                          try {
                            await addMCPServer({
                              name: customServer.name.trim(),
                              enabled: true,
                              command: customServer.command.trim(),
                              args: customServer.args.trim().split(/\s+/).filter(Boolean),
                              env: {},
                              catalog_id: null,
                              description: null,
                            });
                            setCustomServer({ name: '', command: '', args: '' });
                            setShowCustomForm(false);
                            await refreshMCPServers();
                          } catch (e) {
                            setMcpError(String(e));
                          }
                        }}
                        disabled={!customServer.name.trim() || !customServer.command.trim()}
                        className="px-3 py-1 bg-blue-600 hover:bg-blue-500 disabled:bg-elevated rounded text-xs text-white"
                      >
                        Add Server
                      </button>
                      <button
                        onClick={() => { setShowCustomForm(false); setCustomServer({ name: '', command: '', args: '' }); }}
                        className="px-3 py-1 bg-surface hover:bg-elevated rounded text-xs text-secondary"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                )}

                {mcpServers.filter(s => !s.from_catalog).map((server) => (
                  <div key={server.config.name} className="flex items-center gap-3 px-3 py-2 bg-surface/30 border border-border-default rounded-lg">
                    <Server className="w-3.5 h-3.5 text-muted shrink-0" />
                    <div className="flex-1 min-w-0">
                      <span className="text-sm text-secondary">{server.config.name}</span>
                      <p className="text-[10px] text-subtle font-mono truncate">{server.config.command} {server.config.args.join(' ')}</p>
                    </div>
                    <button
                      onClick={async () => {
                        if (server.config.enabled) {
                          await disableMCPServer(server.config.name);
                        } else {
                          await enableMCPServer(server.config.name);
                        }
                        await refreshMCPServers();
                      }}
                      className={`relative w-9 h-5 rounded-full transition-colors ${
                        server.config.enabled ? 'bg-blue-600' : 'bg-elevated'
                      }`}
                    >
                      <span className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                        server.config.enabled ? 'translate-x-4' : 'translate-x-0.5'
                      }`} />
                    </button>
                    <button
                      onClick={async () => {
                        await removeMCPServer(server.config.name);
                        await refreshMCPServers();
                      }}
                      className="text-subtle hover:text-red-700 dark:hover:text-red-600 transition-colors"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                ))}

                {mcpServers.filter(s => !s.from_catalog).length === 0 && !showCustomForm && (
                  <p className="text-[11px] text-subtle italic">No custom servers configured</p>
                )}
              </div>
            </div>
          )}

          {activeSection === 'extensions' && (
            <div className="space-y-5">
              <div>
                <h3 className="text-sm font-medium text-primary">Extension Settings</h3>
                <p className="text-[11px] text-muted mt-1.5 leading-relaxed">
                  Configure settings for installed extensions. Changes are saved automatically.
                </p>
              </div>

              {extSettingsLoading ? (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="w-4 h-4 text-blue-600 dark:text-blue-400 animate-spin" />
                </div>
              ) : installedExtensions.length === 0 ? (
                <p className="text-[11px] text-muted italic">No installed extensions with configuration options.</p>
              ) : (
                <div className="space-y-4">
                  {installedExtensions.map((ext) => {
                    const schema = extensionConfigSchemas[ext.id] as any;
                    const currentSettings = extensionSettingsForms[ext.id] || {};
                    const properties = schema?.properties || {};

                    return (
                      <div key={ext.id} className="border border-border-default rounded-lg bg-panel/40 overflow-hidden">
                        {/* Extension header */}
                        <div className="flex items-center gap-2.5 px-3.5 py-2.5 border-b border-border-default/60 bg-surface/20">
                          <div className="p-1.5 rounded-md bg-surface/60">
                            <Settings className="w-3.5 h-3.5 text-secondary" />
                          </div>
                          <span className="text-[13px] font-medium text-primary">{ext.display_name}</span>
                          <span className="text-[10px] text-subtle ml-auto">{Object.keys(properties).length} settings</span>
                        </div>

                        {/* Settings list */}
                        <div className="divide-y divide-border-default/40">
                          {Object.entries(properties).map(([key, prop]: [string, any]) => {
                            const currentValue = currentSettings[key];
                            const type = prop.type;
                            const description = prop.description;
                            // Format the key: take last segment and convert camelCase to readable
                            const shortKey = key.includes('.') ? key.split('.').pop()! : key;
                            const displayName = shortKey.replace(/([A-Z])/g, ' $1').replace(/^./, s => s.toUpperCase()).trim();

                            const handleChange = (value: unknown) => {
                              setExtensionSettingsForms((prev) => ({
                                ...prev,
                                [ext.id]: { ...prev[ext.id], [key]: value },
                              }));
                            };

                            const saveField = async () => {
                              try {
                                const updated = { ...currentSettings, [key]: currentSettings[key] };
                                await updateExtensionSettings(ext.id, updated);
                              } catch (err) {
                                console.error(`Failed to save extension setting ${key}:`, err);
                              }
                            };

                            return (
                              <div key={key} className="px-3.5 py-2.5">
                                {type === 'boolean' ? (
                                  /* Boolean: toggle row */
                                  <div className="flex items-center justify-between gap-3">
                                    <div className="flex-1 min-w-0">
                                      <div className="text-[12px] text-secondary">{displayName}</div>
                                      {description && (
                                        <p className="text-[10px] text-subtle mt-0.5 leading-relaxed line-clamp-2">{description}</p>
                                      )}
                                    </div>
                                    <button
                                      onClick={() => {
                                        const newVal = !Boolean(currentValue);
                                        setExtensionSettingsForms((prev) => ({
                                          ...prev,
                                          [ext.id]: { ...prev[ext.id], [key]: newVal },
                                        }));
                                        updateExtensionSettings(ext.id, {
                                          ...currentSettings,
                                          [key]: newVal,
                                        }).catch(() => {});
                                      }}
                                      className={`relative shrink-0 inline-flex items-center w-9 h-5 rounded-full transition-colors duration-200 ${
                                        Boolean(currentValue) ? 'bg-blue-500' : 'bg-elevated'
                                      }`}
                                      aria-label={`Toggle ${displayName}`}
                                    >
                                      <span className={`inline-block w-3.5 h-3.5 rounded-full bg-white shadow transition-transform duration-200 ${
                                        Boolean(currentValue) ? 'translate-x-[18px]' : 'translate-x-[3px]'
                                      }`} />
                                    </button>
                                  </div>
                                ) : (
                                  /* Non-boolean: stacked layout */
                                  <div className="space-y-1.5">
                                    <div>
                                      <div className="text-[12px] text-secondary">{displayName}</div>
                                      {description && (
                                        <p className="text-[10px] text-subtle mt-0.5 leading-relaxed line-clamp-2">{description}</p>
                                      )}
                                    </div>
                                    {type === 'number' ? (
                                      <input
                                        type="number"
                                        value={currentValue != null ? String(currentValue) : ''}
                                        onChange={(e) => {
                                          const num = e.target.value ? Number(e.target.value) : 0;
                                          handleChange(num);
                                        }}
                                        onBlur={() =>
                                          updateExtensionSettings(ext.id, {
                                            ...currentSettings,
                                            [key]: currentSettings[key],
                                          }).catch(() => {})
                                        }
                                        className="w-full max-w-[200px] px-2.5 py-1.5 bg-surface border border-border-strong rounded-md text-[12px] text-primary outline-none focus:border-blue-500/50 transition-colors"
                                      />
                                    ) : prop.enum ? (
                                      <select
                                        value={String(currentValue ?? '')}
                                        onChange={(e) => {
                                          const value = e.target.value;
                                          handleChange(value);
                                          updateExtensionSettings(ext.id, {
                                            ...currentSettings,
                                            [key]: value,
                                          }).catch(() => {});
                                        }}
                                        className="w-full max-w-[200px] px-2.5 py-1.5 bg-surface border border-border-strong rounded-md text-[12px] text-primary outline-none focus:border-blue-500/50 transition-colors appearance-none cursor-pointer"
                                      >
                                        {prop.enum.map((opt: any) => (
                                          <option key={opt} value={opt}>
                                            {opt}
                                          </option>
                                        ))}
                                      </select>
                                    ) : (
                                      <input
                                        type="text"
                                        value={String(currentValue ?? '')}
                                        onChange={(e) => handleChange(e.target.value)}
                                        onBlur={() =>
                                          updateExtensionSettings(ext.id, {
                                            ...currentSettings,
                                            [key]: currentSettings[key],
                                          }).catch(() => {})
                                        }
                                        className="w-full max-w-[300px] px-2.5 py-1.5 bg-surface border border-border-strong rounded-md text-[12px] text-primary outline-none focus:border-blue-500/50 transition-colors"
                                        placeholder={prop.default != null ? String(prop.default) : ''}
                                      />
                                    )}
                                  </div>
                                )}
                                {/* Show full key as subtle reference */}
                                <div className="text-[9px] text-subtle mt-1 font-mono truncate">{key}</div>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {activeSection === 'setup' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-primary">Setup & Installation</h3>
              <p className="text-sm text-secondary">
                Run the setup wizard to check and install dependencies for Claude Code on your local machine or remote HPC servers.
              </p>

              <div className="space-y-3">
                <button
                  onClick={() => setShowSetupWizard(true)}
                  className="flex items-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors text-sm w-full justify-center"
                >
                  <Wrench className="w-4 h-4" />
                  Run Setup Wizard
                </button>

                <p className="text-xs text-subtle">
                  The wizard checks for {isMac ? 'Xcode CLI Tools, Homebrew, ' : ''}Node.js, GitHub CLI, Claude Code, and the PDF report library (reportlab), and can install any missing dependencies.
                </p>
              </div>

              <div className="border-t border-border-default pt-4">
                <h4 className="text-sm font-medium text-secondary mb-2">Remote Server Setup</h4>
                <p className="text-sm text-secondary mb-3">
                  To install Claude Code on a remote HPC server, connect to the server via SSH first (using the SSH panel in the sidebar), then run:
                </p>
                <div className="bg-canvas rounded-lg p-3 font-mono text-xs text-secondary space-y-2">
                  <div>
                    <p className="text-muted"># Install Claude Code (no Node.js required)</p>
                    <p>curl -fsSL https://claude.ai/install.sh | bash</p>
                  </div>
                  <div>
                    <p className="text-muted"># Install PDF report library</p>
                    <p>pip3 install reportlab --user</p>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeSection === 'auth' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-primary">Authentication</h3>

              {/* Current status banner */}
              {authStatus && (
                <div className={`flex items-center gap-3 p-3 rounded-lg border ${
                  authStatus.authenticated
                    ? 'bg-green-900/10 border-green-800/30'
                    : 'bg-yellow-900/10 border-yellow-800/30'
                }`}>
                  {authStatus.authenticated ? (
                    <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400 shrink-0" />
                  ) : (
                    <Key className="w-5 h-5 text-yellow-600 dark:text-yellow-400 shrink-0" />
                  )}
                  <div>
                    <p className={`text-sm font-medium ${authStatus.authenticated ? 'text-green-700 dark:text-green-300' : 'text-yellow-700 dark:text-yellow-300'}`}>
                      {authStatus.authenticated
                        ? authStatus.method === 'oauth'
                          ? 'Signed in with Anthropic account'
                          : 'Authenticated with API key'
                        : 'Not authenticated'}
                    </p>
                    <p className="text-[11px] text-muted mt-0.5">
                      {authStatus.authenticated
                        ? authStatus.method === 'oauth'
                          ? 'Using your Max, Pro, or Team subscription'
                          : 'Using direct API billing'
                        : 'Choose a method below to connect to Claude'}
                    </p>
                  </div>
                </div>
              )}

              {/* Option 1: Anthropic Account (OAuth) */}
              <div className="p-4 bg-surface rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <LogIn className="w-4 h-4 text-orange-600 dark:text-orange-400" />
                  <span className="text-sm font-medium text-primary">Anthropic Account</span>
                  {authStatus?.method === 'oauth' && (
                    <span className="ml-auto text-[11px] text-green-600 dark:text-green-400 bg-green-400/10 px-2 py-0.5 rounded-full">
                      Active
                    </span>
                  )}
                </div>
                <p className="text-[12px] text-muted mb-3">
                  For Max, Pro &amp; Team subscribers. Runs <code className="bg-panel px-1 rounded text-secondary">claude login</code> in a terminal tab.
                </p>

                <div className="flex gap-2">
                  <button
                    onClick={async () => {
                      try {
                        const terminalId = crypto.randomUUID();
                        await emit('open-login-terminal', {
                          terminalId,
                          title: 'Claude Login',
                          command: 'claude login',
                        });
                      } catch (err) {
                        console.error('Failed to launch login:', err);
                      }
                    }}
                    className="flex items-center gap-2 px-3 py-1.5 bg-orange-600 hover:bg-orange-700 rounded text-sm text-white transition-colors"
                  >
                    <LogIn className="w-3.5 h-3.5" />
                    {authStatus?.method === 'oauth' ? 'Re-authenticate' : 'Sign in'}
                  </button>
                  <button
                    onClick={async () => {
                      setOauthChecking(true);
                      await refreshAuthStatus();
                      setOauthChecking(false);
                    }}
                    disabled={oauthChecking}
                    className="flex items-center gap-2 px-3 py-1.5 bg-elevated hover:bg-elevated disabled:bg-surface rounded text-sm text-secondary transition-colors"
                  >
                    {oauthChecking ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : (
                      <CheckCircle className="w-3.5 h-3.5" />
                    )}
                    {oauthChecking ? 'Checking...' : 'Verify login'}
                  </button>
                </div>
              </div>

              {/* Option 2: API Key */}
              <div className="p-4 bg-surface rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <Key className="w-4 h-4 text-blue-600 dark:text-blue-400" />
                  <span className="text-sm font-medium text-primary">API Key</span>
                  {hasKey && (
                    <span className="ml-auto text-[11px] text-green-600 dark:text-green-400 bg-green-400/10 px-2 py-0.5 rounded-full">
                      Configured
                    </span>
                  )}
                </div>
                <p className="text-[12px] text-muted mb-3">
                  For direct API billing. Get your key from <span className="text-secondary">console.anthropic.com</span>.
                </p>

                {hasKey ? (
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted flex-1">API key is stored in memory</span>
                    <button
                      onClick={() => { handleDeleteApiKey(); refreshAuthStatus(); }}
                      className="flex items-center gap-1 px-2.5 py-1.5 text-xs text-red-600 dark:text-red-400 hover:text-red-800 dark:hover:text-red-700 bg-red-900/20 hover:bg-red-900/30 rounded transition-colors"
                    >
                      <Trash2 className="w-3 h-3" />
                      Remove
                    </button>
                  </div>
                ) : (
                  <div className="flex gap-2">
                    <input
                      type="password"
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      onKeyDown={(e) => e.key === 'Enter' && handleSaveApiKey()}
                      placeholder="sk-ant-..."
                      className="flex-1 px-2.5 py-1.5 bg-panel border border-border-strong rounded text-sm text-primary placeholder:text-subtle outline-none focus:border-blue-500 transition-colors"
                    />
                    <button
                      onClick={() => { handleSaveApiKey().then(() => refreshAuthStatus()); }}
                      disabled={!apiKey.trim()}
                      className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 disabled:bg-elevated rounded text-sm text-white transition-colors"
                    >
                      Save
                    </button>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
      {/* Setup Wizard Modal */}
      {showSetupWizard && (
        <SetupWizard
          mode="modal"
          onComplete={() => setShowSetupWizard(false)}
        />
      )}
    </div>
  );
}
