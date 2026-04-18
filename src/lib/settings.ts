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
}

export const DEFAULT_SETTINGS: AppSettings = {
  theme: 'dark',
  font_size: 13,
  font_family: 'JetBrains Mono',
  tab_size: 2,
  word_wrap: false,
  minimap_enabled: true,
  model: 'claude-opus-4-20250514',
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
