export interface SearchableProtocol {
  id: string;
  name: string;
  description: string;
  category: string;
  /** Human-readable category label (e.g. "Spatial transcriptomics"). Optional —
   *  when supplied it's included in the searchable text so a query like
   *  "spatial" surfaces every protocol in that category, not just ones whose
   *  name/description happen to contain the word. */
  categoryLabel?: string;
}

export interface ScoredProtocol<T extends SearchableProtocol> {
  protocol: T;
  score: number;
}

const RECENT_KEY = 'operon-recent-protocols';
const CATEGORIES_KEY = 'operon-active-protocol-categories';
const MAX_RECENT = 5;

function tokenize(query: string): string[] {
  return query
    .toLowerCase()
    .split(/\s+/)
    .map(t => t.trim())
    .filter(Boolean);
}

// Loose char-sequence match: every char in needle appears in haystack in order.
function looseCharMatch(haystack: string, needle: string): boolean {
  if (!needle) return false;
  let i = 0;
  for (let j = 0; j < haystack.length && i < needle.length; j++) {
    if (haystack[j] === needle[i]) i++;
  }
  return i === needle.length;
}

function countSubstring(haystack: string, needle: string): number {
  if (!needle) return 0;
  let count = 0;
  let pos = 0;
  while ((pos = haystack.indexOf(needle, pos)) !== -1) {
    count++;
    pos += needle.length;
  }
  return count;
}

// Shared-prefix "stem" match: token and a word share a long common prefix.
// Lets "transcription" match "transcriptomics" (prefix "transcript"), "annotat"
// match "annotation", etc. Requires ≥4 shared chars AND ≥70% of the token.
function stemMatch(word: string, tok: string): boolean {
  const n = Math.min(word.length, tok.length);
  if (n < 4) return false;
  let k = 0;
  while (k < n && word[k] === tok[k]) k++;
  return k >= 4 && k >= Math.ceil(tok.length * 0.7);
}

// Searchable fields with weights — name and slug/id rank highest, then the
// category label/key, then the description.
function fieldsOf<T extends SearchableProtocol>(p: T): Array<[string, number]> {
  return [
    [p.name.toLowerCase(), 6],
    [p.id.toLowerCase(), 4],
    [`${p.categoryLabel || ''} ${p.category}`.toLowerCase(), 4],
    [p.description.toLowerCase(), 3],
  ];
}

export function scoreProtocol<T extends SearchableProtocol>(p: T, tokens: string[]): number {
  if (tokens.length === 0) return 0;
  const fields = fieldsOf(p);
  let score = 0;
  let matched = 0;
  for (const tok of tokens) {
    let hit = 0;
    // Exact substring in any field — full field weight (capped at 3 occurrences).
    for (const [text, w] of fields) {
      const c = countSubstring(text, tok);
      if (c > 0) hit = Math.max(hit, w * Math.min(c, 3));
    }
    // Stem match against any word in any field — 60% of field weight.
    if (hit === 0) {
      for (const [text, w] of fields) {
        if (text.split(/[^a-z0-9]+/).some((word) => stemMatch(word, tok))) {
          hit = Math.max(hit, Math.ceil(w * 0.6));
        }
      }
    }
    // Loose subsequence on name+slug — last-resort weak hit.
    if (hit === 0 && looseCharMatch(`${fields[0][0]} ${fields[1][0]}`, tok)) hit = 1;
    if (hit > 0) {
      score += hit;
      matched++;
    }
  }
  if (matched === 0) return 0;
  // AND-boost: protocols matching every token rank well above partial matches.
  if (matched === tokens.length && tokens.length > 1) score += 12;
  return score;
}

export function searchProtocols<T extends SearchableProtocol>(
  protocols: T[],
  query: string,
): T[] {
  const tokens = tokenize(query);
  if (tokens.length === 0) {
    return [...protocols].sort((a, b) => a.name.localeCompare(b.name));
  }
  const scored: ScoredProtocol<T>[] = [];
  for (const p of protocols) {
    const score = scoreProtocol(p, tokens);
    if (score > 0) scored.push({ protocol: p, score });
  }
  scored.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.protocol.name.localeCompare(b.protocol.name);
  });
  return scored.map(s => s.protocol);
}

export function loadRecentProtocolIds(): string[] {
  try {
    const raw = window.localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is string => typeof x === 'string').slice(0, MAX_RECENT);
  } catch {
    return [];
  }
}

export function pushRecentProtocolId(id: string): string[] {
  const current = loadRecentProtocolIds().filter(x => x !== id);
  const next = [id, ...current].slice(0, MAX_RECENT);
  try {
    window.localStorage.setItem(RECENT_KEY, JSON.stringify(next));
  } catch {
    // ignore quota / unavailable storage
  }
  return next;
}

export function loadActiveCategories(): Set<string> {
  try {
    const raw = window.localStorage.getItem(CATEGORIES_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((x): x is string => typeof x === 'string'));
  } catch {
    return new Set();
  }
}

export function saveActiveCategories(cats: Set<string>): void {
  try {
    window.localStorage.setItem(CATEGORIES_KEY, JSON.stringify(Array.from(cats)));
  } catch {
    // ignore
  }
}
