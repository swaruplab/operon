import { invoke } from '@tauri-apps/api/core';

export interface PortkeyPreset {
  id: string;
  label: string;
  base_url: string;
  description: string;
  eligibility: string;
  signup_url: string;
  docs_url: string;
  privacy_summary: string;
  suggested_models: string[];
}

export interface PortkeyModel {
  id: string;
  object: string;
  created: number;
  owned_by: string;
}

export async function listPortkeyPresets(): Promise<PortkeyPreset[]> {
  return invoke<PortkeyPreset[]>('list_portkey_presets');
}

/// Optional 7-day background refresh of the preset manifest from GitHub.
/// Resolves to true if the cache was updated.
export async function refreshPortkeyPresets(): Promise<boolean> {
  return invoke<boolean>('refresh_portkey_presets');
}

export async function fetchPortkeyModels(baseUrl: string, apiKey: string): Promise<PortkeyModel[]> {
  return invoke<PortkeyModel[]>('fetch_portkey_models', { baseUrl, apiKey });
}
