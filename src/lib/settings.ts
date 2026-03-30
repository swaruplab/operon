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
}

export const DEFAULT_SETTINGS: AppSettings = {
  theme: 'dark',
  font_size: 13,
  font_family: 'JetBrains Mono',
  tab_size: 2,
  word_wrap: false,
  minimap_enabled: true,
  model: 'claude-sonnet-4-20250514',
  max_turns: 25,
  max_budget_usd: 5.0,
  permission_mode: 'full_auto',
  show_hidden_files: false,
  terminal_font_size: 13,
  mcp_servers: [],
  extension_settings: {},
};

export async function getSettings(): Promise<AppSettings> {
  return invoke('get_settings');
}

export async function updateSettings(settings: AppSettings): Promise<void> {
  return invoke('update_settings', { settings });
}
