export interface SearchableProtocol {
  id: string;
  name: string;
  description: string;
  category: string;
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

export function scoreProtocol<T extends SearchableProtocol>(p: T, tokens: string[]): number {
  if (tokens.length === 0) return 0;
  const name = p.name.toLowerCase();
  const desc = p.description.toLowerCase();
  let score = 0;
  let anyHit = false;
  for (const tok of tokens) {
    const nameHits = countSubstring(name, tok);
    const descHits = countSubstring(desc, tok);
    if (nameHits > 0) {
      score += 5 * nameHits;
      anyHit = true;
    }
    if (descHits > 0) {
      score += 3 * descHits;
      anyHit = true;
    }
    if (nameHits === 0 && looseCharMatch(name, tok)) {
      score += 1;
      anyHit = true;
    }
  }
  return anyHit ? score : 0;
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
