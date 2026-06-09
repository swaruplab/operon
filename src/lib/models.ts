import { invoke } from '@tauri-apps/api/core';

export interface CapabilitySupport {
  supported: boolean;
}

export interface EffortCapability {
  supported: boolean;
  low: CapabilitySupport;
  medium: CapabilitySupport;
  high: CapabilitySupport;
  max: CapabilitySupport;
  xhigh: CapabilitySupport;
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

export type EffortLevel = 'low' | 'medium' | 'high' | 'max' | 'xhigh';

const EFFORT_ORDER: EffortLevel[] = ['low', 'medium', 'high', 'max', 'xhigh'];

/// Return the effort levels supported by a given model, in canonical order.
/// Empty array means the model doesn't support `--effort` at all (e.g. Haiku 4.5).
export function supportedEffortLevels(model: ModelInfo | undefined): EffortLevel[] {
  if (!model?.capabilities?.effort?.supported) return [];
  const e = model.capabilities.effort;
  return EFFORT_ORDER.filter((lvl) => e[lvl]?.supported);
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

// Fable 5 has an Anthropic-defined billing cutover on 2026-06-23: included on
// Pro / Max / Team / seat-Enterprise through Jun 22; usage credits required
// after. The badge surfaces the current state directly in the model picker so
// users don't get surprised by their first credits charge.
export interface ModelBadge {
  label: string;
  textClass: string;
  bgClass: string;
  tooltip: string;
}

const FABLE5_CUTOFF_UTC = Date.parse('2026-06-23T00:00:00Z');

export function getFable5Badge(now: number = Date.now()): ModelBadge {
  if (now < FABLE5_CUTOFF_UTC) {
    return {
      label: 'FREE · through Jun 22',
      textClass: 'text-green-700 dark:text-green-300',
      bgClass: 'bg-green-100 dark:bg-green-900/40',
      tooltip:
        'Included on Pro, Max, Team, and seat-based Enterprise plans through June 22, 2026. After that, calls require usage credits ($10 / $50 per million tokens). The developer API is unaffected.',
    };
  }
  return {
    label: 'CREDITS REQUIRED',
    textClass: 'text-amber-700 dark:text-amber-300',
    bgClass: 'bg-amber-100 dark:bg-amber-900/40',
    tooltip:
      'No longer included on subscription plans. Calls bill against usage credits at $10/M input, $50/M output. Without credits, Claude falls back to your default model (typically Opus 4.8).',
  };
}

export function fable5OptionSuffix(modelId: string, now: number = Date.now()): string {
  if (modelId !== 'claude-fable-5') return '';
  return `  ·  ${getFable5Badge(now).label}`;
}
