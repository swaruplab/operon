import { invoke } from '@tauri-apps/api/core';

export interface ModelInfo {
  id: string;
  display_name: string;
  created_at: string;
  max_input_tokens: number;
  max_tokens: number;
}

export type ModelTier = 'opus' | 'sonnet' | 'haiku' | 'other';

export interface GroupedModels {
  opus: ModelInfo[];
  sonnet: ModelInfo[];
  haiku: ModelInfo[];
  other: ModelInfo[];
}

export function tierOf(model: ModelInfo): ModelTier {
  const id = model.id.toLowerCase();
  if (id.includes('opus')) return 'opus';
  if (id.includes('sonnet')) return 'sonnet';
  if (id.includes('haiku')) return 'haiku';
  return 'other';
}

export function groupAndSort(models: ModelInfo[]): GroupedModels {
  const grouped: GroupedModels = { opus: [], sonnet: [], haiku: [], other: [] };
  for (const m of models) grouped[tierOf(m)].push(m);
  const byNewest = (a: ModelInfo, b: ModelInfo) =>
    (b.created_at || '').localeCompare(a.created_at || '');
  grouped.opus.sort(byNewest);
  grouped.sonnet.sort(byNewest);
  grouped.haiku.sort(byNewest);
  grouped.other.sort(byNewest);
  return grouped;
}

/// Returns cached models, or the Rust-bundled fallback if no cache. Never throws.
export async function getCachedModels(): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>('get_cached_models');
}

/// Forces a fresh fetch from Anthropic's /v1/models and updates the cache.
export async function fetchAnthropicModels(apiKey: string): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>('fetch_anthropic_models', { apiKey });
}

/// Background refresh: no-op if cache is < 7 days old or no API key is set.
/// Returns true if the cache was refreshed.
export async function refreshModelsIfStale(apiKey: string | null): Promise<boolean> {
  return invoke<boolean>('refresh_models_if_stale', { apiKey });
}
