import { invoke } from '@tauri-apps/api/core';
import type { MCPServerConfig } from '../types/mcp';

export interface AppSettings {
  theme: string;
  font_size: number;
  font_family: string;
  tab_size: number;
  word_wrap: boolean;
  minimap_enabled: boolean;
  model: string;
  /** Anthropic effort/reasoning level: 'low' | 'medium' | 'high' | 'max' | 'xhigh'.
   *  Skipped at command-build time if the chosen model doesn't support the level. */
  effort: string;
  max_turns: number;
  max_budget_usd: number;
  permission_mode: string; // 'full_auto' | 'safe_mode' | 'supervised'
  show_hidden_files: boolean;
  terminal_font_size: number;
  mcp_servers: MCPServerConfig[];
  extension_settings: Record<string, Record<string, unknown>>;
  last_project_path?: string | null;
  // AI provider (OpenAI-compatible endpoint support)
  ai_provider: 'anthropic' | 'custom';
  custom_base_url: string;
  custom_api_key: string;
  custom_model: string;
  /** Route custom-provider requests through the bundled Anthropic↔OpenAI
   *  translation sidecar. Off by default — the primary route is an
   *  Anthropic-compatible gateway (OpenRouter/LiteLLM) called directly.
   *  Enable only for OpenAI-only local runtimes (Ollama/vLLM/LM-Studio<0.4.1). */
  use_translation_proxy: boolean;
  /** Use the xterm.js WebGL renderer addon. Turn off on setups where the
   *  WebGL atlas renders with hairline / ghost-stroke artifacts (seen on
   *  some Apple-silicon + Studio Display scaled modes). */
  terminal_use_webgl: boolean;
  /** Wrap new SSH terminals in a shared tmux session so jobs survive Operon
   *  quitting / laptop sleeping. No-op if the remote has no tmux installed. */
  ssh_auto_tmux: boolean;
  /** Name of the shared tmux session Operon attaches to. */
  ssh_tmux_session: string;
  /** Soft wall-clock cap on a single agent session, in minutes. Frontend
   *  shows warn-only banners at 75% and 100%. 0 disables the budget. */
  session_time_budget_minutes: number;
}

export const DEFAULT_SETTINGS: AppSettings = {
  theme: 'dark',
  font_size: 13,
  font_family: 'JetBrains Mono',
  tab_size: 2,
  word_wrap: false,
  minimap_enabled: true,
  model: 'claude-opus-4-8',
  effort: 'high',
  max_turns: 25,
  max_budget_usd: 5.0,
  permission_mode: 'full_auto',
  show_hidden_files: false,
  terminal_font_size: 13,
  mcp_servers: [],
  extension_settings: {},
  last_project_path: null,
  ai_provider: 'anthropic',
  custom_base_url: '',
  custom_api_key: '',
  custom_model: '',
  use_translation_proxy: false,
  terminal_use_webgl: true,
  ssh_auto_tmux: true,
  ssh_tmux_session: 'operon-main',
  session_time_budget_minutes: 90,
};

export async function detectCustomModels(baseUrl: string, apiKey?: string): Promise<string[]> {
  return invoke('detect_custom_models', { baseUrl, apiKey: apiKey || null });
}

export async function testCustomEndpoint(baseUrl: string, apiKey: string | undefined, model: string): Promise<string> {
  return invoke('test_custom_endpoint', { baseUrl, apiKey: apiKey || null, model });
}

export async function getSettings(): Promise<AppSettings> {
  return invoke('get_settings');
}

export async function updateSettings(settings: AppSettings): Promise<void> {
  return invoke('update_settings', { settings });
}

// ── Translation proxy (Anthropic ↔ OpenAI) ───────────────────────────────
export interface ProxyStatus {
  running: boolean;
  port: number | null;
  url: string | null;
  upstream_base_url: string | null;
}

export async function startTranslationProxy(
  upstreamBaseUrl: string,
  upstreamApiKey?: string,
): Promise<string> {
  return invoke('start_translation_proxy', {
    upstreamBaseUrl,
    upstreamApiKey: upstreamApiKey || null,
  });
}

export async function stopTranslationProxy(): Promise<void> {
  return invoke('stop_translation_proxy');
}

export async function translationProxyStatus(): Promise<ProxyStatus> {
  return invoke('translation_proxy_status');
}
