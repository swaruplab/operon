import { useState, useEffect, useCallback } from 'react';
import { X, Settings, Key, Trash2, LogIn, CheckCircle, Loader2, Wrench, Server, Plus, AlertTriangle, ExternalLink, ChevronDown, ChevronRight, ShieldOff, ShieldCheck, Shield } from 'lucide-react';
import { SetupWizard } from '../setup/SetupWizard';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import type { AppSettings } from '../../lib/settings';
import { DEFAULT_SETTINGS } from '../../lib/settings';
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
      enabled ? 'border-blue-800/40 bg-blue-950/10' : 'border-zinc-800 bg-zinc-900/40'
    }`}>
      {/* Main row */}
      <div className="flex items-start gap-3 px-3.5 py-3">
        {/* Icon */}
        <div className={`mt-0.5 p-1.5 rounded-md ${enabled ? 'bg-blue-900/30' : 'bg-zinc-800/60'}`}>
          <Server className={`w-3.5 h-3.5 ${enabled ? 'text-blue-400' : 'text-zinc-500'}`} />
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-[13px] font-medium text-zinc-200">{entry?.name || server.config.name}</span>
            <span className={`text-[9px] font-medium uppercase tracking-wide px-1.5 py-[1px] rounded ${
              entry?.runtime === 'node'
                ? 'bg-green-900/30 text-green-400 border border-green-800/30'
                : 'bg-yellow-900/30 text-yellow-400 border border-yellow-800/30'
            }`}>
              {entry?.runtime === 'node' ? 'Node.js' : 'Python'}
            </span>
            {entry && (
              <span className="text-[10px] text-zinc-500">{entry.tools_count} tools</span>
            )}
          </div>
          <p className="text-[11px] text-zinc-500 mt-1 leading-relaxed line-clamp-2">
            {entry?.description || server.config.description}
          </p>
        </div>

        {/* Toggle */}
        <div className="shrink-0 mt-0.5">
          {isInstalling ? (
            <Loader2 className="w-4 h-4 text-blue-400 animate-spin" />
          ) : (
            <button
              onClick={onToggle}
              className={`relative inline-flex items-center w-9 h-5 rounded-full transition-colors duration-200 ${
                enabled ? 'bg-blue-500' : 'bg-zinc-600'
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
        className="flex items-center gap-1 w-full px-3.5 py-1.5 text-[10px] text-zinc-500 hover:text-zinc-300 transition-colors border-t border-zinc-800/40"
      >
        {expanded ? <ChevronDown className="w-2.5 h-2.5" /> : <ChevronRight className="w-2.5 h-2.5" />}
        Details
      </button>

      {/* Expanded details */}
      {expanded && entry && (
        <div className="px-3.5 pb-3 space-y-2.5">
          <div>
            <span className="text-[10px] text-zinc-400 font-medium">Tools:</span>
            <div className="mt-1 flex flex-wrap gap-1">
              {entry.tools_summary.slice(0, 8).map((tool, i) => (
                <span key={i} className="text-[9px] text-zinc-400 bg-zinc-800/60 px-1.5 py-0.5 rounded font-mono">
                  {tool}
                </span>
              ))}
              {entry.tools_summary.length > 8 && (
                <span className="text-[9px] text-zinc-600 px-1.5 py-0.5">+{entry.tools_summary.length - 8} more</span>
              )}
            </div>
          </div>
          {entry.databases.length > 0 && (
            <div>
              <span className="text-[10px] text-zinc-400 font-medium">Databases:</span>
              <p className="text-[10px] text-zinc-500 mt-0.5 leading-relaxed">{entry.databases.join(' \u00b7 ')}</p>
            </div>
          )}
          <div className="flex items-center gap-3 pt-1">
            <span className="text-[10px] text-zinc-600">License: {entry.license}</span>
            {entry.homepage && (
              <a
                onClick={(e) => { e.preventDefault(); invoke('open_url', { url: entry.homepage }); }}
                href="#"
                className="flex items-center gap-1 text-[10px] text-blue-400 hover:text-blue-300"
              >
                <ExternalLink className="w-2.5 h-2.5" /> Homepage
              </a>
            )}
          </div>
          {/* Environment Variables (API keys etc.) */}
          {envKeys.length > 0 && (
            <div>
              <span className="text-[10px] text-zinc-400 font-medium">Environment Variables:</span>
              <div className="mt-1.5 space-y-1.5">
                {envKeys.map((key) => (
                  <div key={key} className="flex items-center gap-2">
                    <span className="text-[10px] text-zinc-500 font-mono shrink-0 min-w-0 truncate" title={key}>
                      {key.replace(/_/g, '_\u200B')}
                    </span>
                    <input
                      type="password"
                      value={envValues[key] || ''}
                      onChange={(e) => setEnvValues(prev => ({ ...prev, [key]: e.target.value }))}
                      placeholder="Enter value..."
                      className="flex-1 px-2 py-1 bg-zinc-900 border border-zinc-700 rounded text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-blue-600/50 font-mono min-w-0"
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
                  <p className="text-[10px] text-emerald-400 mt-1">
                    Keys saved. Start a <strong>new chat session</strong> for changes to take effect.
                  </p>
                )}
              </div>
            </div>
          )}

          {depCheck && (
            <div className={`flex items-center gap-2 p-2 rounded-md text-[10px] ${
              depCheck.satisfied
                ? 'bg-green-950/20 text-green-400 border border-green-900/20'
                : 'bg-yellow-950/20 text-yellow-400 border border-yellow-900/20'
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

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SettingsPanel({ isOpen, onClose }: SettingsPanelProps) {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [apiKey, setApiKey] = useState('');
  const [hasKey, setHasKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [activeSection, setActiveSection] = useState<'editor' | 'terminal' | 'claude' | 'auth' | 'mcp' | 'extensions' | 'setup'>(
    'editor',
  );
  const [showSetupWizard, setShowSetupWizard] = useState(false);
  const [authStatus, setAuthStatus] = useState<{ authenticated: boolean; method: string } | null>(null);
  const [oauthChecking, setOauthChecking] = useState(false);

  // MCP state
  const [mcpServers, setMcpServers] = useState<MCPServerStatus[]>([]);
  const [mcpCatalog, setMcpCatalog] = useState<MCPCatalogEntry[]>([]);
  const [mcpLoading, setMcpLoading] = useState(false);
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
    }
  }, [isOpen, refreshAuthStatus, refreshMCPServers, refreshExtensionSettings]);

  const saveSettings = useCallback(async (updated: AppSettings) => {
    setSaving(true);
    try {
      await invoke('update_settings', { settings: updated });
      setSettings(updated);
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
    { id: 'auth' as const, label: 'Authentication' },
    { id: 'mcp' as const, label: 'MCP Servers' },
    { id: 'extensions' as const, label: 'Extension Settings' },
    { id: 'setup' as const, label: 'Setup Wizard' },
  ];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/60" onClick={onClose} />
      <div className="relative w-[700px] max-h-[80vh] bg-zinc-900 rounded-xl border border-zinc-700 shadow-2xl flex overflow-hidden">
        {/* Sidebar */}
        <div className="w-[180px] border-r border-zinc-800 py-3">
          <div className="flex items-center gap-2 px-4 pb-3 border-b border-zinc-800">
            <Settings className="w-4 h-4 text-zinc-400" />
            <span className="text-sm font-medium text-zinc-300">Settings</span>
          </div>
          <div className="py-2">
            {sections.map((section) => (
              <button
                key={section.id}
                onClick={() => setActiveSection(section.id)}
                className={`w-full text-left px-4 py-1.5 text-sm ${
                  activeSection === section.id
                    ? 'bg-zinc-800 text-zinc-100'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/50'
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
            className="absolute top-3 right-3 p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300"
          >
            <X className="w-4 h-4" />
          </button>

          {activeSection === 'editor' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-zinc-200">Editor Settings</h3>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Font Size</span>
                <input
                  type="number"
                  value={settings.font_size}
                  onChange={(e) =>
                    saveSettings({ ...settings, font_size: parseInt(e.target.value) || 13 })
                  }
                  className="w-20 px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 outline-none"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Tab Size</span>
                <input
                  type="number"
                  value={settings.tab_size}
                  onChange={(e) =>
                    saveSettings({ ...settings, tab_size: parseInt(e.target.value) || 2 })
                  }
                  className="w-20 px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 outline-none"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Word Wrap</span>
                <input
                  type="checkbox"
                  checked={settings.word_wrap}
                  onChange={(e) => saveSettings({ ...settings, word_wrap: e.target.checked })}
                  className="w-4 h-4 accent-blue-500"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Minimap</span>
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
                <span className="text-sm text-zinc-400">Show Hidden Files</span>
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
              <h3 className="text-sm font-medium text-zinc-200">Terminal Settings</h3>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Terminal Font Size</span>
                <input
                  type="number"
                  value={settings.terminal_font_size}
                  onChange={(e) =>
                    saveSettings({
                      ...settings,
                      terminal_font_size: parseInt(e.target.value) || 13,
                    })
                  }
                  className="w-20 px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 outline-none"
                />
              </label>
            </div>
          )}

          {activeSection === 'claude' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-zinc-200">Claude Code Settings</h3>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Default Model</span>
                <select
                  value={settings.model}
                  onChange={(e) => saveSettings({ ...settings, model: e.target.value })}
                  className="w-56 px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 outline-none"
                >
                  <option value="claude-sonnet-4-20250514">Sonnet 4</option>
                  <option value="claude-opus-4-20250514">Opus 4</option>
                  <option value="claude-haiku-4-5-20251001">Haiku 4.5</option>
                </select>
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Max Turns</span>
                <input
                  type="number"
                  value={settings.max_turns}
                  onChange={(e) =>
                    saveSettings({ ...settings, max_turns: parseInt(e.target.value) || 25 })
                  }
                  className="w-20 px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 outline-none"
                />
              </label>

              <label className="flex items-center justify-between">
                <span className="text-sm text-zinc-400">Max Budget (USD)</span>
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
                  className="w-20 px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 outline-none"
                />
              </label>

              {/* Permission Level */}
              <div className="pt-3 border-t border-zinc-800">
                <div className="flex items-center gap-2 mb-3">
                  <Shield className="w-4 h-4 text-zinc-400" />
                  <span className="text-sm font-medium text-zinc-200">Permission Level</span>
                </div>
                <div className="space-y-2">
                  {([
                    {
                      value: 'full_auto',
                      label: 'Full Auto',
                      desc: 'Claude reads, writes, and executes commands without asking. Fastest workflow.',
                      icon: ShieldOff,
                      color: 'text-amber-400',
                      border: settings.permission_mode === 'full_auto' ? 'border-amber-500/60 bg-amber-950/20' : 'border-zinc-700/50 hover:border-zinc-600',
                    },
                    {
                      value: 'safe_mode',
                      label: 'Safe Mode',
                      desc: 'Claude can read and search freely, but writes, edits, and bash commands require approval.',
                      icon: ShieldCheck,
                      color: 'text-blue-400',
                      border: settings.permission_mode === 'safe_mode' ? 'border-blue-500/60 bg-blue-950/20' : 'border-zinc-700/50 hover:border-zinc-600',
                    },
                    {
                      value: 'supervised',
                      label: 'Supervised',
                      desc: 'Claude asks permission for every action. Maximum control, slower workflow.',
                      icon: Shield,
                      color: 'text-green-400',
                      border: settings.permission_mode === 'supervised' ? 'border-green-500/60 bg-green-950/20' : 'border-zinc-700/50 hover:border-zinc-600',
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
                        <Icon className={`w-4 h-4 mt-0.5 shrink-0 ${isActive ? opt.color : 'text-zinc-500'}`} />
                        <div className="min-w-0">
                          <div className={`text-xs font-medium ${isActive ? 'text-zinc-100' : 'text-zinc-400'}`}>
                            {opt.label}
                            {opt.value === 'full_auto' && (
                              <span className="ml-1.5 text-[10px] text-zinc-600 font-normal">default</span>
                            )}
                          </div>
                          <div className="text-[11px] text-zinc-500 mt-0.5 leading-relaxed">{opt.desc}</div>
                        </div>
                        {isActive && (
                          <CheckCircle className={`w-3.5 h-3.5 mt-0.5 shrink-0 ml-auto ${opt.color}`} />
                        )}
                      </button>
                    );
                  })}
                </div>
                {settings.permission_mode === 'supervised' && (
                  <div className="mt-2 flex items-start gap-2 px-3 py-2 bg-yellow-950/20 border border-yellow-800/30 rounded text-[11px] text-yellow-400/80">
                    <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
                    <span>Supervised mode requires Claude Code to be authenticated interactively. Each action will prompt in the terminal.</span>
                  </div>
                )}
              </div>
            </div>
          )}

          {activeSection === 'mcp' && (
            <div className="space-y-5">
              <div>
                <div className="flex items-center justify-between">
                  <h3 className="text-sm font-medium text-zinc-200">MCP Servers</h3>
                  {mcpLoading && <Loader2 className="w-3.5 h-3.5 text-zinc-500 animate-spin" />}
                </div>
                <p className="text-[11px] text-zinc-500 mt-1.5 leading-relaxed">
                  MCP servers give Claude access to external tools and databases.
                  Enabled servers are automatically available in all Claude sessions.
                </p>
              </div>

              {mcpError && (
                <div className="flex items-center gap-2 p-2.5 bg-red-950/20 border border-red-900/30 rounded-lg">
                  <AlertTriangle className="w-3.5 h-3.5 text-red-400 shrink-0" />
                  <span className="text-[11px] text-red-300">{mcpError}</span>
                  <button onClick={() => setMcpError(null)} className="ml-auto text-zinc-600 hover:text-zinc-400">
                    <X className="w-3 h-3" />
                  </button>
                </div>
              )}

              {/* Catalog Servers */}
              <div className="space-y-2.5">
                <h4 className="text-[10px] font-semibold text-zinc-500 uppercase tracking-wider">Research Tools Catalog</h4>
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
                  <h4 className="text-[10px] font-semibold text-zinc-500 uppercase tracking-wider">Custom Servers</h4>
                  <button
                    onClick={() => setShowCustomForm(!showCustomForm)}
                    className="flex items-center gap-1 text-[10px] text-blue-400 hover:text-blue-300 transition-colors"
                  >
                    <Plus className="w-3 h-3" /> Add
                  </button>
                </div>

                {showCustomForm && (
                  <div className="p-3 bg-zinc-800/50 border border-zinc-700 rounded-lg space-y-2">
                    <input
                      type="text"
                      value={customServer.name}
                      onChange={(e) => setCustomServer(prev => ({ ...prev, name: e.target.value }))}
                      placeholder="Server name"
                      className="w-full px-2 py-1 bg-zinc-900 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none"
                    />
                    <input
                      type="text"
                      value={customServer.command}
                      onChange={(e) => setCustomServer(prev => ({ ...prev, command: e.target.value }))}
                      placeholder="Command (e.g. npx, uvx, node)"
                      className="w-full px-2 py-1 bg-zinc-900 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none"
                    />
                    <input
                      type="text"
                      value={customServer.args}
                      onChange={(e) => setCustomServer(prev => ({ ...prev, args: e.target.value }))}
                      placeholder="Arguments (space-separated)"
                      className="w-full px-2 py-1 bg-zinc-900 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none"
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
                        className="px-3 py-1 bg-blue-600 hover:bg-blue-500 disabled:bg-zinc-700 rounded text-xs text-white"
                      >
                        Add Server
                      </button>
                      <button
                        onClick={() => { setShowCustomForm(false); setCustomServer({ name: '', command: '', args: '' }); }}
                        className="px-3 py-1 bg-zinc-800 hover:bg-zinc-700 rounded text-xs text-zinc-400"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                )}

                {mcpServers.filter(s => !s.from_catalog).map((server) => (
                  <div key={server.config.name} className="flex items-center gap-3 px-3 py-2 bg-zinc-800/30 border border-zinc-800 rounded-lg">
                    <Server className="w-3.5 h-3.5 text-zinc-500 shrink-0" />
                    <div className="flex-1 min-w-0">
                      <span className="text-sm text-zinc-300">{server.config.name}</span>
                      <p className="text-[10px] text-zinc-600 font-mono truncate">{server.config.command} {server.config.args.join(' ')}</p>
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
                        server.config.enabled ? 'bg-blue-600' : 'bg-zinc-700'
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
                      className="text-zinc-600 hover:text-red-400 transition-colors"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                ))}

                {mcpServers.filter(s => !s.from_catalog).length === 0 && !showCustomForm && (
                  <p className="text-[11px] text-zinc-600 italic">No custom servers configured</p>
                )}
              </div>
            </div>
          )}

          {activeSection === 'extensions' && (
            <div className="space-y-5">
              <div>
                <h3 className="text-sm font-medium text-zinc-200">Extension Settings</h3>
                <p className="text-[11px] text-zinc-500 mt-1.5 leading-relaxed">
                  Configure settings for installed extensions. Changes are saved automatically.
                </p>
              </div>

              {extSettingsLoading ? (
                <div className="flex items-center justify-center py-8">
                  <Loader2 className="w-4 h-4 text-blue-400 animate-spin" />
                </div>
              ) : installedExtensions.length === 0 ? (
                <p className="text-[11px] text-zinc-500 italic">No installed extensions with configuration options.</p>
              ) : (
                <div className="space-y-4">
                  {installedExtensions.map((ext) => {
                    const schema = extensionConfigSchemas[ext.id] as any;
                    const currentSettings = extensionSettingsForms[ext.id] || {};
                    const properties = schema?.properties || {};

                    return (
                      <div key={ext.id} className="border border-zinc-800 rounded-lg bg-zinc-900/40 overflow-hidden">
                        {/* Extension header */}
                        <div className="flex items-center gap-2.5 px-3.5 py-2.5 border-b border-zinc-800/60 bg-zinc-800/20">
                          <div className="p-1.5 rounded-md bg-zinc-800/60">
                            <Settings className="w-3.5 h-3.5 text-zinc-400" />
                          </div>
                          <span className="text-[13px] font-medium text-zinc-200">{ext.display_name}</span>
                          <span className="text-[10px] text-zinc-600 ml-auto">{Object.keys(properties).length} settings</span>
                        </div>

                        {/* Settings list */}
                        <div className="divide-y divide-zinc-800/40">
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
                                      <div className="text-[12px] text-zinc-300">{displayName}</div>
                                      {description && (
                                        <p className="text-[10px] text-zinc-600 mt-0.5 leading-relaxed line-clamp-2">{description}</p>
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
                                        Boolean(currentValue) ? 'bg-blue-500' : 'bg-zinc-600'
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
                                      <div className="text-[12px] text-zinc-300">{displayName}</div>
                                      {description && (
                                        <p className="text-[10px] text-zinc-600 mt-0.5 leading-relaxed line-clamp-2">{description}</p>
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
                                        className="w-full max-w-[200px] px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded-md text-[12px] text-zinc-100 outline-none focus:border-blue-500/50 transition-colors"
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
                                        className="w-full max-w-[200px] px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded-md text-[12px] text-zinc-100 outline-none focus:border-blue-500/50 transition-colors appearance-none cursor-pointer"
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
                                        className="w-full max-w-[300px] px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded-md text-[12px] text-zinc-100 outline-none focus:border-blue-500/50 transition-colors"
                                        placeholder={prop.default != null ? String(prop.default) : ''}
                                      />
                                    )}
                                  </div>
                                )}
                                {/* Show full key as subtle reference */}
                                <div className="text-[9px] text-zinc-700 mt-1 font-mono truncate">{key}</div>
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
              <h3 className="text-sm font-medium text-zinc-200">Setup & Installation</h3>
              <p className="text-sm text-zinc-400">
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

                <p className="text-xs text-zinc-600">
                  The wizard checks for Xcode CLI Tools, Homebrew, Node.js, GitHub CLI, Claude Code, and the PDF report library (reportlab), and can install any missing dependencies.
                </p>
              </div>

              <div className="border-t border-zinc-800 pt-4">
                <h4 className="text-sm font-medium text-zinc-300 mb-2">Remote Server Setup</h4>
                <p className="text-sm text-zinc-400 mb-3">
                  To install Claude Code on a remote HPC server, connect to the server via SSH first (using the SSH panel in the sidebar), then run:
                </p>
                <div className="bg-zinc-950 rounded-lg p-3 font-mono text-xs text-zinc-300 space-y-2">
                  <div>
                    <p className="text-zinc-500"># Install Claude Code (no Node.js required)</p>
                    <p>curl -fsSL https://claude.ai/install.sh | bash</p>
                  </div>
                  <div>
                    <p className="text-zinc-500"># Install PDF report library</p>
                    <p>pip3 install reportlab --user</p>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeSection === 'auth' && (
            <div className="space-y-5">
              <h3 className="text-sm font-medium text-zinc-200">Authentication</h3>

              {/* Current status banner */}
              {authStatus && (
                <div className={`flex items-center gap-3 p-3 rounded-lg border ${
                  authStatus.authenticated
                    ? 'bg-green-900/10 border-green-800/30'
                    : 'bg-yellow-900/10 border-yellow-800/30'
                }`}>
                  {authStatus.authenticated ? (
                    <CheckCircle className="w-5 h-5 text-green-400 shrink-0" />
                  ) : (
                    <Key className="w-5 h-5 text-yellow-400 shrink-0" />
                  )}
                  <div>
                    <p className={`text-sm font-medium ${authStatus.authenticated ? 'text-green-300' : 'text-yellow-300'}`}>
                      {authStatus.authenticated
                        ? authStatus.method === 'oauth'
                          ? 'Signed in with Anthropic account'
                          : 'Authenticated with API key'
                        : 'Not authenticated'}
                    </p>
                    <p className="text-[11px] text-zinc-500 mt-0.5">
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
              <div className="p-4 bg-zinc-800 rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <LogIn className="w-4 h-4 text-orange-400" />
                  <span className="text-sm font-medium text-zinc-200">Anthropic Account</span>
                  {authStatus?.method === 'oauth' && (
                    <span className="ml-auto text-[11px] text-green-400 bg-green-400/10 px-2 py-0.5 rounded-full">
                      Active
                    </span>
                  )}
                </div>
                <p className="text-[12px] text-zinc-500 mb-3">
                  For Max, Pro &amp; Team subscribers. Runs <code className="bg-zinc-900 px-1 rounded text-zinc-400">claude login</code> in a terminal tab.
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
                    className="flex items-center gap-2 px-3 py-1.5 bg-zinc-700 hover:bg-zinc-600 disabled:bg-zinc-800 rounded text-sm text-zinc-300 transition-colors"
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
              <div className="p-4 bg-zinc-800 rounded-lg">
                <div className="flex items-center gap-2 mb-1">
                  <Key className="w-4 h-4 text-blue-400" />
                  <span className="text-sm font-medium text-zinc-200">API Key</span>
                  {hasKey && (
                    <span className="ml-auto text-[11px] text-green-400 bg-green-400/10 px-2 py-0.5 rounded-full">
                      Configured
                    </span>
                  )}
                </div>
                <p className="text-[12px] text-zinc-500 mb-3">
                  For direct API billing. Get your key from <span className="text-zinc-400">console.anthropic.com</span>.
                </p>

                {hasKey ? (
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-zinc-500 flex-1">API key is stored in memory</span>
                    <button
                      onClick={() => { handleDeleteApiKey(); refreshAuthStatus(); }}
                      className="flex items-center gap-1 px-2.5 py-1.5 text-xs text-red-400 hover:text-red-300 bg-red-900/20 hover:bg-red-900/30 rounded transition-colors"
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
                      className="flex-1 px-2.5 py-1.5 bg-zinc-900 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500 transition-colors"
                    />
                    <button
                      onClick={() => { handleSaveApiKey().then(() => refreshAuthStatus()); }}
                      disabled={!apiKey.trim()}
                      className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 disabled:bg-zinc-700 rounded text-sm text-white transition-colors"
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
