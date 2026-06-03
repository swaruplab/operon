import { Zap } from 'lucide-react';

interface Props {
  /** Tokens currently used in the conversation context (latest input_tokens). */
  used: number;
  /** Maximum context window size for the active model. 0/undefined → bar hidden. */
  max: number;
  /** cache_read_input_tokens — shown as a ⚡ when high (>50% of used). */
  cacheRead?: number;
  /** Compact mode for tight headers. */
  compact?: boolean;
}

function formatTokens(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(2)}M`;
}

export function TokenUsageBar({ used, max, cacheRead = 0, compact }: Props) {
  if (!max || max <= 0) return null;
  const pct = Math.max(0, Math.min(100, (used / max) * 100));
  const cacheRatio = used > 0 ? cacheRead / used : 0;
  const showCacheIcon = cacheRatio > 0.5;

  // Threshold colours: green → yellow at 60% → red at 85%.
  const barColor =
    pct >= 85 ? 'bg-rose-500' : pct >= 60 ? 'bg-amber-400' : 'bg-emerald-500';
  const textColor =
    pct >= 85 ? 'text-rose-400' : pct >= 60 ? 'text-amber-400' : 'text-zinc-400';

  const width = compact ? 'w-24' : 'w-36';
  const height = compact ? 'h-1' : 'h-1.5';

  return (
    <div
      className="flex items-center gap-1.5 shrink-0"
      title={`${used.toLocaleString()} / ${max.toLocaleString()} tokens (${pct.toFixed(0)}%) used in context window${cacheRead > 0 ? ` · ${cacheRead.toLocaleString()} from cache` : ''}`}
    >
      <div
        className={`${width} ${height} bg-zinc-800 rounded-full overflow-hidden border border-zinc-700/50`}
      >
        <div
          className={`h-full ${barColor} transition-all duration-500`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className={`text-[10px] font-mono tabular-nums ${textColor}`}>
        {formatTokens(used)}/{formatTokens(max)}
      </span>
      {showCacheIcon && (
        <Zap
          className="w-3 h-3 text-blue-400 pointer-events-none"
          aria-label="majority of context is cache-read"
        />
      )}
    </div>
  );
}
