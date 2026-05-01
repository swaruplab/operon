#!/usr/bin/env bash
# operon-watchdog.sh — HPC job watchdog.
#
# Long-running agent that survives the user logging out of Operon. Polls the
# batch scheduler for every job listed in ~/.operon/watchlist, appends an event
# to ~/.operon/jobs/<jobid>.jsonl, and applies the policy in ~/.operon/policy.json
# (auto-resubmit on TIMEOUT / OOM, up to a retry budget).
#
# Designed to be launched inside a detached tmux session by the Operon app:
#     tmux new-session -d -s operon-watchdog "bash ~/.operon/operon-watchdog.sh"
#
# Files:
#   ~/.operon/watchlist          one record per line: JOBID<TAB>SCHEDULER<TAB>SUBMIT_TIME<TAB>SBATCH<TAB>RETRIES
#   ~/.operon/jobs/<jobid>.jsonl NDJSON event log (one event per line)
#   ~/.operon/policy.json        { on_timeout: {...}, on_oom: {...}, max_retries: N }
#   ~/.operon/watchdog.pid       pidfile
#   ~/.operon/watchdog.log       stderr log
#
# This script has no dependencies beyond coreutils + the scheduler CLI
# (sacct / qstat / bjobs). It's intentionally simple and idempotent — kill
# it and restart anytime.

set -u
umask 077

OPERON_DIR="${OPERON_DIR:-$HOME/.operon}"
WATCHLIST="$OPERON_DIR/watchlist"
JOBS_DIR="$OPERON_DIR/jobs"
POLICY="$OPERON_DIR/policy.json"
PIDFILE="$OPERON_DIR/watchdog.pid"
LOGFILE="$OPERON_DIR/watchdog.log"
POLL_SECONDS="${OPERON_WATCHDOG_POLL:-30}"

mkdir -p "$JOBS_DIR"
[ -f "$WATCHLIST" ] || : > "$WATCHLIST"

# Record our pid. If another watchdog is already running, exit quietly.
if [ -f "$PIDFILE" ]; then
  existing=$(cat "$PIDFILE" 2>/dev/null)
  if [ -n "${existing:-}" ] && kill -0 "$existing" 2>/dev/null; then
    echo "watchdog already running as pid $existing" >&2
    exit 0
  fi
fi
echo $$ > "$PIDFILE"

log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >> "$LOGFILE"; }

now_ms() { date +%s%3N 2>/dev/null || echo "$(($(date +%s) * 1000))"; }

emit_event() {
  # emit_event <jobid> <json-payload>
  local jobid="$1"; shift
  local payload="$1"
  printf '%s\n' "$payload" >> "$JOBS_DIR/$jobid.jsonl"
}

# ── policy helpers ────────────────────────────────────────────────────────
# Read a top-level field from policy.json with a default. No jq dependency;
# tolerant of missing file.
policy_field() {
  local key="$1" default="$2"
  if [ -f "$POLICY" ]; then
    # extract "key": <value>  — string or number, stop at comma/brace
    awk -v k="\"$key\"" '
      BEGIN { RS="," }
      $0 ~ k {
        sub(/.*:/, "")
        gsub(/[{}\"[:space:]]/, "")
        print
        exit
      }
    ' "$POLICY"
  fi || true
  # awk may output nothing — fall back
  :
}

# ── scheduler: SLURM ──────────────────────────────────────────────────────
slurm_state() {
  # echoes "STATE|EXITCODE|ELAPSED|MAXRSS"
  local jobid="$1"
  local out
  out=$(sacct -j "$jobid" -n -X -P -o State,ExitCode,ElapsedRaw,MaxRSS 2>/dev/null | head -n1)
  if [ -z "$out" ]; then
    # fall back to squeue while job is still pending/running
    local sq
    sq=$(squeue -h -j "$jobid" -o '%T' 2>/dev/null | head -n1)
    if [ -n "$sq" ]; then
      out="$sq|0:0|0|"
    fi
  fi
  printf '%s' "$out"
}

slurm_resubmit() {
  # slurm_resubmit <sbatch_path> — echoes new job id or empty
  local sbatch="$1"
  [ -f "$sbatch" ] || { echo ""; return; }
  local out
  out=$(sbatch "$sbatch" 2>/dev/null)
  # "Submitted batch job 12345"
  echo "$out" | awk '/Submitted batch job/ {print $NF}'
}

terminal_state() {
  case "$1" in
    COMPLETED|FAILED|CANCELLED*|TIMEOUT|OUT_OF_MEMORY|NODE_FAIL|BOOT_FAIL|DEADLINE|PREEMPTED)
      return 0 ;;
    *) return 1 ;;
  esac
}

handle_terminal() {
  # handle_terminal <jobid> <scheduler> <state> <sbatch> <retries_left>
  local jobid="$1" sched="$2" state="$3" sbatch="$4" retries="$5"
  local policy_action="none"
  local new_jobid=""

  # Budget: if retries left and state matches a policy-driven reason, resubmit.
  if [ "$retries" -gt 0 ] && [ -n "$sbatch" ] && [ -f "$sbatch" ]; then
    case "$state" in
      TIMEOUT|DEADLINE)
        policy_action="resubmit_timeout"
        new_jobid=$(slurm_resubmit "$sbatch")
        ;;
      OUT_OF_MEMORY|NODE_FAIL)
        policy_action="resubmit_oom_or_nodefail"
        new_jobid=$(slurm_resubmit "$sbatch")
        ;;
    esac
  fi

  local ts; ts=$(now_ms)
  emit_event "$jobid" "$(printf '{"ts":%s,"type":"terminal","state":"%s","action":"%s","resubmitted_as":"%s"}' \
    "$ts" "$state" "$policy_action" "${new_jobid:-}")"

  # If we resubmitted, append a new watchlist entry for the new id with a decremented retry budget.
  if [ -n "$new_jobid" ]; then
    printf '%s\t%s\t%s\t%s\t%s\n' "$new_jobid" "$sched" "$(now_ms)" "$sbatch" "$((retries - 1))" >> "$WATCHLIST"
    emit_event "$new_jobid" "$(printf '{"ts":%s,"type":"registered","from_job":"%s","retries_left":%s}' \
      "$ts" "$jobid" "$((retries - 1))")"
  fi
}

# ── main loop ─────────────────────────────────────────────────────────────
log "watchdog started pid=$$ poll=${POLL_SECONDS}s"
emit_event "watchdog" "$(printf '{"ts":%s,"type":"watchdog_start","pid":%s}' "$(now_ms)" "$$")"

cleanup() {
  log "watchdog exiting"
  emit_event "watchdog" "$(printf '{"ts":%s,"type":"watchdog_stop"}' "$(now_ms)")"
  rm -f "$PIDFILE"
}
trap cleanup EXIT INT TERM

while :; do
  # Swap the watchlist: we rebuild a fresh copy that excludes terminal jobs.
  if [ -s "$WATCHLIST" ]; then
    tmp="$WATCHLIST.tmp.$$"
    : > "$tmp"
    while IFS=$'\t' read -r jobid scheduler submit_ts sbatch retries; do
      [ -z "${jobid:-}" ] && continue
      case "$scheduler" in
        slurm|"")
          info=$(slurm_state "$jobid")
          ;;
        *)
          # unknown scheduler — keep polling but don't apply policy
          info=""
          ;;
      esac
      state="${info%%|*}"
      state="${state// /}"
      if [ -z "$state" ]; then
        # sacct hasn't seen it yet — keep it.
        printf '%s\t%s\t%s\t%s\t%s\n' "$jobid" "$scheduler" "$submit_ts" "$sbatch" "$retries" >> "$tmp"
        continue
      fi

      emit_event "$jobid" "$(printf '{"ts":%s,"type":"poll","state":"%s","raw":"%s"}' \
        "$(now_ms)" "$state" "$info")"

      if terminal_state "$state"; then
        handle_terminal "$jobid" "$scheduler" "$state" "$sbatch" "${retries:-0}"
        # drop from watchlist (don't write to $tmp)
      else
        printf '%s\t%s\t%s\t%s\t%s\n' "$jobid" "$scheduler" "$submit_ts" "$sbatch" "$retries" >> "$tmp"
      fi
    done < "$WATCHLIST"
    mv "$tmp" "$WATCHLIST"
  fi

  sleep "$POLL_SECONDS"
done
