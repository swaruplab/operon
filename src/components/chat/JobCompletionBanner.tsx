import { CheckCircle2, XCircle, AlertTriangle, X, FileText, Play } from 'lucide-react';
import {
  type PendingCompletion,
  variantForState,
  formatElapsed,
  markCompletionSeen,
} from '../../lib/jobNotify';

interface Props {
  completion: PendingCompletion;
  onResume: (c: PendingCompletion) => void;
  onViewLog: (c: PendingCompletion) => void;
  onDismiss: (c: PendingCompletion) => void;
}

export function JobCompletionBanner({ completion, onResume, onViewLog, onDismiss }: Props) {
  const variant = variantForState(completion.state);

  const accent =
    variant === 'success'
      ? 'border-l-green-500 bg-green-950/20'
      : variant === 'failure'
        ? 'border-l-red-500 bg-red-950/20'
        : variant === 'timeout'
          ? 'border-l-yellow-500 bg-yellow-950/20'
          : 'border-l-zinc-500 bg-panel/40';

  const Icon =
    variant === 'success' ? CheckCircle2 : variant === 'failure' ? XCircle : AlertTriangle;
  const iconColor =
    variant === 'success'
      ? 'text-green-600 dark:text-green-400'
      : variant === 'failure'
        ? 'text-red-600 dark:text-red-400'
        : variant === 'timeout'
          ? 'text-yellow-600 dark:text-yellow-400'
          : 'text-secondary';

  const jobLabel = completion.job_name ? `${completion.job_name} (${completion.job_id})` : `job ${completion.job_id}`;

  const headline = (() => {
    const elapsed = formatElapsed(completion.elapsed_seconds);
    if (variant === 'success') {
      return `${completion.session_name} — ${jobLabel} finished in ${elapsed}`;
    }
    if (variant === 'failure') {
      const code = completion.exit_code && completion.exit_code !== '0:0' ? ` (exit ${completion.exit_code})` : '';
      return `${completion.session_name} — ${jobLabel} failed${code} at ${elapsed}`;
    }
    if (variant === 'timeout') {
      return `${completion.session_name} — ${jobLabel} hit walltime (${elapsed})`;
    }
    return `${completion.session_name} — ${jobLabel} cancelled at ${elapsed}`;
  })();

  const subline = (() => {
    if (variant === 'success') {
      if (completion.expected_output) {
        return `${completion.expected_output} is ready`;
      }
      return 'Job completed';
    }
    if (variant === 'failure') {
      return completion.last_error_line
        ? `Last error: ${completion.last_error_line}`
        : '(failed — see log)';
    }
    if (variant === 'timeout') {
      return 'Cancelled by SLURM · output may be partial';
    }
    return completion.state;
  })();

  const resumeLabel = variant === 'failure' ? 'Resume to debug' : 'Resume';

  const handleDismiss = () => {
    markCompletionSeen(completion.profile_id, completion.job_id, completion.terminal_at_ms).catch(
      () => {},
    );
    onDismiss(completion);
  };

  return (
    <div
      className={`mx-3 my-1.5 px-3 py-2 rounded-md border border-l-4 border-border-default ${accent} text-xs`}
    >
      <div className="flex items-start gap-2">
        <Icon className={`w-4 h-4 mt-0.5 shrink-0 ${iconColor}`} />
        <div className="flex-1 min-w-0">
          <div className="text-primary font-medium truncate">{headline}</div>
          <div className="text-secondary truncate">{subline}</div>
        </div>
        <button
          onClick={handleDismiss}
          className="p-0.5 text-muted hover:text-secondary transition-colors shrink-0"
          title="Dismiss"
        >
          <X className="w-3 h-3 pointer-events-none" />
        </button>
      </div>
      <div className="mt-2 flex items-center gap-1.5 pl-6">
        <button
          onClick={() => onResume(completion)}
          className="flex items-center gap-1 px-2 py-0.5 rounded bg-blue-600/80 hover:bg-blue-600 text-white text-[11px] font-medium transition-colors"
        >
          <Play className="w-3 h-3 pointer-events-none" />
          {resumeLabel}
        </button>
        {completion.log_path && (
          <button
            onClick={() => onViewLog(completion)}
            className="flex items-center gap-1 px-2 py-0.5 rounded bg-surface hover:bg-elevated text-secondary text-[11px] font-medium transition-colors"
          >
            <FileText className="w-3 h-3 pointer-events-none" />
            View log
          </button>
        )}
      </div>
    </div>
  );
}
