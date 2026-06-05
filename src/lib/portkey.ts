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

// ── Portkey slug parsing + family grouping ────────────────────────────────
// Portkey model slugs are `@workspace/model-id`. The workspace tells you
// which backend cloud (Bedrock, Vertex, Azure, etc.) and the model-id tells
// you which specific model. Users care about the model family (Anthropic,
// Google, etc.), not the cloud, so we parse + group by family.

export type ModelFamily =
  | 'Anthropic'
  | 'OpenAI'
  | 'Google'
  | 'Mistral'
  | 'Moonshot'
  | 'Meta'
  | 'Other';

export interface ParsedPortkeyModel {
  slug: string;            // raw @workspace/model-id
  workspace: string;       // e.g. "zotgpt-api-bedrock"
  model_id: string;        // e.g. "us.anthropic.claude-opus-4-8"
  family: ModelFamily;
  display_name: string;    // friendly label e.g. "Claude Opus 4.8"
  workspace_hint: string;  // short tag e.g. "bedrock"
  rank: number;            // lower = better (sort ascending for best-first)
}

// Family ordering for "best first" — Anthropic on top (Operon is Claude-first).
const FAMILY_ORDER: Record<ModelFamily, number> = {
  Anthropic: 0,
  OpenAI:    1,
  Google:    2,
  Meta:      3,
  Mistral:   4,
  Moonshot:  5,
  Other:     99,
};

const FAMILY_LABELS: Record<ModelFamily, string> = {
  Anthropic: 'Anthropic',
  OpenAI:    'OpenAI',
  Google:    'Google',
  Meta:      'Meta',
  Mistral:   'Mistral',
  Moonshot:  'Moonshot',
  Other:     'Other',
};

/** Strip the leading `zotgpt-api-` etc. prefix for a compact tag. */
function shortWorkspace(workspace: string): string {
  return workspace
    .replace(/^zotgpt-api-/, '')
    .replace(/^openai-/, '')
    .replace(/-prod$/, '');
}

export function parsePortkeySlug(slug: string): ParsedPortkeyModel {
  const m = slug.match(/^@([^/]+)\/(.+)$/);
  if (!m) {
    return {
      slug, workspace: '', model_id: slug,
      family: 'Other', display_name: slug, workspace_hint: '', rank: 100,
    };
  }
  const workspace = m[1];
  const model_id = m[2];
  const wsHint = shortWorkspace(workspace);
  const lower = model_id.toLowerCase();

  let family: ModelFamily = 'Other';
  let display_name = model_id;
  let rank = 50;

  // ── Anthropic Claude ────────────────────────────────────────────────────
  if (/anthropic\.claude|claude-/.test(lower)) {
    family = 'Anthropic';
    const tier = lower.includes('opus')   ? 'Opus'
              : lower.includes('sonnet') ? 'Sonnet'
              : lower.includes('haiku')  ? 'Haiku'
              : 'Claude';
    const verMatch = lower.match(/(?:opus|sonnet|haiku|claude)-(\d+(?:[-_]\d+)*)/);
    let version = '';
    if (verMatch) {
      version = verMatch[1]
        .replace(/[-_]/g, '.')
        .replace(/\.0$/, '');     // drop trailing .0 if any
    }
    display_name = version ? `Claude ${tier} ${version}` : `Claude ${tier}`;
    // Rank: Opus > Sonnet > Haiku; within tier, newest first.
    const tierRank = tier === 'Opus' ? 0 : tier === 'Sonnet' ? 100 : tier === 'Haiku' ? 200 : 300;
    const verNum = parseFloat(version) || 0;
    rank = tierRank - verNum * 5;     // higher version = better (lower number)
  }
  // ── OpenAI ──────────────────────────────────────────────────────────────
  else if (/^(?:gpt|o\d)/i.test(model_id) || /\bgpt-/.test(lower)) {
    family = 'OpenAI';
    const verMatch = lower.match(/gpt-?([\d.]+\w*)/);
    if (verMatch) {
      display_name = `GPT-${verMatch[1].toUpperCase()}`;
    } else if (/^o\d/.test(lower)) {
      display_name = model_id.toUpperCase();
    } else {
      display_name = model_id;
    }
    const verNum = parseFloat((verMatch?.[1] || '0').replace(/[^0-9.]/g, '')) || 0;
    rank = -verNum * 5;   // 5.5 < 4o < 4 → GPT-5.5 ranked first
  }
  // ── Google Gemini ───────────────────────────────────────────────────────
  else if (/gemini/.test(lower)) {
    family = 'Google';
    const verMatch = lower.match(/gemini[-_]?([\d.]+)/);
    const variant = lower.includes('flash-lite') ? 'Flash Lite'
                  : lower.includes('flash')      ? 'Flash'
                  : lower.includes('pro')        ? 'Pro'
                  : lower.includes('ultra')      ? 'Ultra'
                  : '';
    display_name = ['Gemini', verMatch?.[1] || '', variant].filter(Boolean).join(' ');
    const verNum = parseFloat(verMatch?.[1] || '0') || 0;
    // Pro > Flash > Lite; newer version first
    const variantRank = variant === 'Pro' ? 0 : variant === 'Ultra' ? -5 : variant === 'Flash' ? 20 : 30;
    rank = variantRank - verNum * 2;
  }
  // ── Meta Llama ──────────────────────────────────────────────────────────
  else if (/llama/.test(lower) || /^meta\./.test(lower)) {
    family = 'Meta';
    display_name = model_id.replace(/^meta\./, '').replace(/-v\d+:?\d*$/, '');
    rank = 50;
  }
  // ── Mistral ─────────────────────────────────────────────────────────────
  else if (/mistral|mixtral/.test(lower)) {
    family = 'Mistral';
    display_name = model_id
      .replace(/^mistral\./, '')
      .replace(/-v\d+:?\d*$/, '')
      .replace(/-instruct$/, '');
    rank = 50;
  }
  // ── Moonshot / Kimi ─────────────────────────────────────────────────────
  else if (/kimi|moonshot/.test(lower)) {
    family = 'Moonshot';
    display_name = model_id
      .replace(/^moonshotai\./, '')
      .replace(/^kimi-?/i, 'Kimi ');
    rank = 50;
  }

  // Dedicated-workspace boost: when the workspace name matches the family,
  // rank it slightly higher than the same model in a generic workspace.
  // Example: Gemini lives in both @zotgpt-api-gemini (dedicated) and
  // @zotgpt-api-vertex (generic) — prefer the dedicated.
  const dedicated =
    (family === 'Google'    && /gemini/.test(workspace.toLowerCase())) ||
    (family === 'Anthropic' && /bedrock|anthropic/.test(workspace.toLowerCase())) ||
    (family === 'OpenAI'    && /openai|azure|gpt/.test(workspace.toLowerCase())) ||
    (family === 'Meta'      && /llama|meta/.test(workspace.toLowerCase())) ||
    (family === 'Mistral'   && /mistral/.test(workspace.toLowerCase())) ||
    (family === 'Moonshot'  && /moonshot|kimi/.test(workspace.toLowerCase()));
  if (dedicated) rank -= 1; // small nudge — keeps duplicates visible but dedicated wins ties

  return { slug, workspace, model_id, family, display_name, workspace_hint: wsHint, rank };
}

/** Group slugs by family, sorted best-first within each group. */
export function groupPortkeyModelsByFamily(
  slugs: string[],
): Array<[ModelFamily, ParsedPortkeyModel[]]> {
  const parsed = slugs.map(parsePortkeySlug);
  const grouped: Record<ModelFamily, ParsedPortkeyModel[]> = {
    Anthropic: [], OpenAI: [], Google: [], Meta: [], Mistral: [], Moonshot: [], Other: [],
  };
  for (const p of parsed) grouped[p.family].push(p);
  for (const k of Object.keys(grouped) as ModelFamily[]) {
    grouped[k].sort((a, b) => a.rank - b.rank);
  }
  // Drop empty families, return as ordered tuples
  return (Object.keys(FAMILY_ORDER) as ModelFamily[])
    .sort((a, b) => FAMILY_ORDER[a] - FAMILY_ORDER[b])
    .filter((f) => grouped[f].length > 0)
    .map((f) => [f, grouped[f]] as [ModelFamily, ParsedPortkeyModel[]]);
}

/** Family display label (for optgroup labels). */
export function familyLabel(f: ModelFamily): string {
  return FAMILY_LABELS[f];
}

/** Pick the best model from a list of slugs — best Claude first, then GPT, then Gemini. */
export function pickBestPortkeyModel(slugs: string[]): string | null {
  const grouped = groupPortkeyModelsByFamily(slugs);
  for (const [, models] of grouped) {
    if (models.length > 0) return models[0].slug;
  }
  return null;
}

/**
 * True if the Portkey slug refers to an Anthropic-family model (which Portkey
 * can serve via its native /v1/messages passthrough). Non-Anthropic models
 * must be routed via the bundled OpenAI-format translation proxy.
 *
 * Empty / unknown slugs default to true so the safe direct path is used until
 * the user explicitly picks a non-Anthropic model. Mirrors the Rust helper in
 * src-tauri/src/commands/claude.rs.
 */
export function isAnthropicPortkeyModel(slug: string): boolean {
  if (!slug.trim()) return true;
  return parsePortkeySlug(slug).family === 'Anthropic';
}

/**
 * True if the Portkey slug refers to an OpenAI-family reasoning model
 * (GPT-5+, o1/o3/o4 series). These force tool calls through OpenAI's newer
 * Responses API (/v1/responses), which our bundled anthropic-proxy can't
 * translate to — it only speaks /v1/chat/completions. So a request through
 * Operon for one of these models 400s with "Function tools with
 * reasoning_effort are not supported … Please use /v1/responses instead."
 *
 * Older OpenAI models (GPT-4, GPT-4o, GPT-4.1, GPT-4-turbo) still accept
 * tools on Chat Completions and work fine via the existing proxy path.
 */
export function isReasoningOpenAIModel(slug: string): boolean {
  if (!slug.trim()) return false;
  const parsed = parsePortkeySlug(slug);
  if (parsed.family !== 'OpenAI') return false;
  const id = parsed.model_id.toLowerCase();
  // GPT-5 and above.
  if (/(?:^|[^a-z0-9])gpt-?[5-9]/i.test(id)) return true;
  // o-series reasoning models (o1, o3, o4, …; o2 was never publicly released).
  if (/(?:^|[^a-z0-9])o[1-9](?:[-_.]|$)/i.test(id)) return true;
  return false;
}
