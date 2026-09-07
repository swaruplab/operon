import { invoke } from '@tauri-apps/api/core';

export interface CapabilitySupport {
  supported: boolean;
}

export interface EffortCapability {
  supported: boolean;
  low: CapabilitySupport;
  medium: CapabilitySupport;
  high: CapabilitySupport;
  xhigh: CapabilitySupport;
  max: CapabilitySupport;
}

export interface ModelCapabilities {
  effort: EffortCapability;
}

export interface ModelInfo {
  id: string;
  display_name: string;
  created_at: string;
  max_input_tokens: number;
  max_tokens: number;
  capabilities: ModelCapabilities;
}

export type EffortLevel = 'low' | 'medium' | 'high' | 'xhigh' | 'max';

/// Canonical order, cheapest/shallowest to most expensive/deepest:
/// low < medium < high < xhigh < max — `xhigh` sits BETWEEN `high` and `max`.
/// Drives both the Settings dropdown order and the click-to-cycle order of the
/// effort button in the chat composer, so it must stay in this order.
const EFFORT_ORDER: EffortLevel[] = ['low', 'medium', 'high', 'xhigh', 'max'];

/// Return the effort levels supported by a given model, in canonical order.
/// Empty array means the model doesn't support `--effort` at all (e.g. Haiku 4.5).
export function supportedEffortLevels(model: ModelInfo | undefined): EffortLevel[] {
  if (!model?.capabilities?.effort?.supported) return [];
  const e = model.capabilities.effort;
  return EFFORT_ORDER.filter((lvl) => e[lvl]?.supported);
}

/// The effort level actually in force for `model` given a persisted `effort`.
///
/// A stored level is not necessarily offered by the model currently selected —
/// Sonnet 4.6 stops at `high`, Haiku 4.5 supports none — and the backend simply
/// omits `--effort` when the level is unsupported (`model_supports_effort_level`
/// in models.rs). Showing the raw stored value would therefore claim a depth the
/// run will not use. This returns the highest supported level at or below the
/// stored one, so the UI states what will really happen; `null` means the model
/// takes no effort flag at all.
export function clampEffort(
  model: ModelInfo | undefined,
  effort: string,
): EffortLevel | null {
  const levels = supportedEffortLevels(model);
  if (levels.length === 0) return null;
  const wanted = EFFORT_ORDER.indexOf(effort as EffortLevel);
  // An unrecognised stored value (hand-edited settings.json) falls back to the
  // model's own ceiling rather than silently picking `low`.
  if (wanted === -1) return levels[levels.length - 1];
  const atOrBelow = levels.filter((lvl) => EFFORT_ORDER.indexOf(lvl) <= wanted);
  return atOrBelow.length > 0 ? atOrBelow[atOrBelow.length - 1] : levels[0];
}

export type ModelTier = 'fable' | 'opus' | 'sonnet' | 'haiku' | 'other';

export interface GroupedModels {
  fable: ModelInfo[];
  opus: ModelInfo[];
  sonnet: ModelInfo[];
  haiku: ModelInfo[];
  other: ModelInfo[];
}

export function tierOf(model: ModelInfo): ModelTier {
  const id = model.id.toLowerCase();
  if (id.includes('fable')) return 'fable';
  if (id.includes('opus')) return 'opus';
  if (id.includes('sonnet')) return 'sonnet';
  if (id.includes('haiku')) return 'haiku';
  return 'other';
}

export function groupAndSort(models: ModelInfo[]): GroupedModels {
  const grouped: GroupedModels = { fable: [], opus: [], sonnet: [], haiku: [], other: [] };
  for (const m of models) grouped[tierOf(m)].push(m);
  const byNewest = (a: ModelInfo, b: ModelInfo) =>
    (b.created_at || '').localeCompare(a.created_at || '');
  grouped.fable.sort(byNewest);
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

