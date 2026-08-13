# Change record — HPC watchdog removal and follow-up audit

**Date:** 2026-08-13
**Completed:** 12:56 PDT
**Branch:** `cross-platform`
**Status:** uncommitted working-tree changes — nothing committed, nothing pushed
**Scope:** 24 files, +1,785 / −1,702

---

## 0. Why this happened

The user received automated notices from UCI RCIC that a process on their account
had been terminated on a login node:

```
PID: <pid>   Mode: cmd   Pattern: watch
Command: bash <home>/.operon/operon-watchdog.sh
Host: <login-node>
```

That process was Operon's own HPC job watchdog.

---

## 1. Root cause

Two prior fixes were in tension, and nobody noticed:

| Release | Change | Effect |
|---|---|---|
| v0.9.6 (`314d4eb`) | Watchdog overhaul added **auto-bootstrap on every SSH connect** | Installed + started a detached login-node daemon, unprompted |
| v1.0.0 (`096a963`) | `hpc_restrict_login_node` added | Gated **only** the ChatPanel `.claude` probes — zero backend consumers |

Fixing "the watcher never works" created "the watcher runs where it must not."
The policy setting shipped a release *later* and never covered the daemon.

**Why it recurred forever:** the reaper killed the bash process → the tmux session
died with it (single window) → the next SSH connect found `has-session` false and
started it again. The only guard was a module-level in-memory `Set` in
`SSHView.tsx`, empty on every app launch and cleared on any rejection. The Jobs
panel's Stop button killed the daemon but persisted nothing.

**Independent of the name:** `operon-watchdog.sh`'s `while :; do … sleep 30; done`
had no exit condition — no wall-clock cap, no idle timeout, no exit on an empty
watchlist. Its `squeue` self-discovery ran *before* the empty-watchlist guard, so
it queried the scheduler every 30 s even with zero jobs, and the watchlist only
ever grew.

---

## 2. The decisive finding

The user asked whether the watcher was needed at all. Investigation of its only
genuinely stateful feature:

```bash
slurm_resubmit() {
  out=$(sbatch "$sbatch" 2>/dev/null)   # the identical, unmodified script
```

`on_timeout_walltime_mult` (1.5×) and `on_oom_mem_mult` (2×) were rendered as
inputs in `JobsView.tsx`, persisted to `policy.json`, deserialized into the Rust
struct — **and read by nothing.** A TIMEOUT was resubmitted with the same
walltime and timed out again, twice by default; an OOM with the same `--mem`.
It burned allocation to reproduce identical failures while the settings panel
displayed multipliers implying otherwise.

That removed the last argument for keeping a resident process. The Jobs panel
needs *data*, not a daemon.

---

## 3. What was built to replace it

| Capability | Before | After |
|---|---|---|
| Live + historical job state | daemon → `~/.operon/jobs/*.jsonl` | `list_cluster_jobs` — one round-trip, `squeue` + `sacct -S now-7days`, only while the panel is open |
| Job logs | login-node `tail -f`, hours-long | `read_job_log_tail` on demand, line- **and** byte-bounded |
| Completion while Operon is closed | daemon (killed by the site) | SLURM `--mail-user` — needs nothing of ours running |
| Session ↔ job attribution | remote `~/.operon/watchlist` | local `job_registry.json` only |
| Upgrade path | none | one-shot `cleanup_legacy_watchdog` on connect |

**Removed:** `scripts/operon-watchdog.sh`, `install/start/stop/bootstrap_watchdog`,
`watchdog_status`, `register/unregister/list_watched_jobs`, `get/set_job_policy`,
`read_job_events`, `start_job_tail`, `stop_job_tail`, `WatchdogManager`,
`detect_scheduler` + the `Scheduler` enum, and the `hpc_watchdog_enabled` setting
(added mid-session to gate the daemon, then removed — nothing left to gate).

---

## 4. Audit findings fixed

A 19-agent adversarial audit (8 confirmed / 4 refuted) ran over the completed
work. Everything below was found *after* the refactor and fixed.

### Ship blockers

| # | Problem | Fix |
|---|---|---|
| B1 | `claude.rs:3398` told **every** session (including local) that "Operon runs a remote watchdog that survives the app being closed" and to say "feel free to close Operon." | Rewrote `slurm_rule`. Points at `--mail-user` as the only closed-app path. Reconciled `ChatPanel.tsx:3350`, which told the agent to poll `squeue` while the rule said never poll. |
| B2 | Array/het jobs **never notified**. `sbatch --array` registers `12345`; `sacct` returns `12345_1`… and the lookup compared exactly. Entry then immortal, since `mark_completion_seen` is the only prune path. | `base_job_id()` normalisation on both the lookup and the prune; task rows aggregate to one card, a failing task winning over a completed one. |
| B3 | Status pill read "Watchdog running" (hardcoded true), counted 7 days of `sacct` history so one cancelled job pinned it red forever, and never cleared on unmount. | Live rows only; relabelled; zeroed tick on cleanup; `watchdogRunning` dropped. |
| B4 | `ActivityBar.tsx:21` advertised "auto-resubmit on failure". | Relabelled to what it does. |

### Correctness / robustness

- **`$USER` guard restored** (`slurm.rs`) — the deleted daemon used
  `${USER:-$(id -un)}`; the rewrite dropped it. With `USER` unset, `squeue -u ""`
  applies **no filter** and would list the whole cluster, every row with a
  working Cancel button. Now resolves defensively and refuses rather than
  over-reporting (`__OPERON_NOUSER__`, surfaced in the panel).
- **sacct scoped with `-u`** — account coordinators (normal for a PI's lab
  account) would otherwise see the whole lab's history beside their own jobs.
- **Pipe-in-job-name corruption** — `JobName` sat mid-record with `|` as the
  delimiter, and sbatch accepts `|` in `--job-name`; one pipe shifted every
  later field, so the row came back *wrong* rather than dropped. `JobName` moved
  last in both formats; parsed with `splitn`.
- **Per-row `date` forks eliminated** — the script forked `date` once per sacct
  row (thousands per poll on the shared login node — the exact load that got the
  daemon killed). The raw ISO stamp is passed through and sorts correctly as a
  string. The throwaway accounting-probe query was also removed.
- **`SPECIAL_EXIT` drift** — the remote `case` was a hand-written copy, narrower
  than the Rust list, so those jobs never notified. Both now generate from one
  `TERMINAL_STATES` const.
- **`scan_profile` was blocking a tokio worker** — synchronous `ssh_exec` inside
  an async command, sequentially over every profile; a couple of unreachable
  profiles made the poll outlast its own 30 s interval. Now `ssh_exec_async`,
  with an in-flight guard in ChatPanel. Errors propagate instead of being
  swallowed by `unwrap_or_default()`, which made a broken feature look identical
  to "nothing finished".
- **Registry growth** — only removable by dismissing a banner. Added a 14-day
  age sweep, a 500-entry cap, and a skip for non-SLURM profiles (which can never
  produce a `sacct` completion).
- **Per-poll log transfer** — `tail -n 30` on a progress-bar log is one huge
  `\r`-delimited line, megabytes +33 % base64, every 30 s. Now `tail -c 8000`
  first, and already-seen ids are filtered *before* the remote script is built.
- **Empty-hostname misclassification** (`remote_is_slurm_login_node`) — on
  BusyBox/minimal images `hostname -s` is absent, the grep pattern is empty, and
  a real compute node fell through to "login" and was refused. Now falls open on
  that specific case, and uses `grep -qxF` (a hostname is a literal, not a regex).
- **Login-node probe cached** per profile for the app's life — it was an extra
  blocking round-trip on every message, plus an `sinfo` RPC on a compute verdict.
- **Manual Reconnect** now resets the attempt budget; with the new 120 s
  stability gate it otherwise re-raised the banner the user had just dismissed
  and blocked auto-retry for two minutes.
- **Job log cached forever, including errors** — expanding a RUNNING job froze
  its log at first expansion, and a transient failure was cached permanently as
  content. Errors now live in a separate map, the expanded job refreshes on each
  poll tick, and running jobs always re-read.
- **Pending array ranges** (`12345_[5-9]`) fired a guaranteed-failing log request
  (the backend validator rejects `[`). Now shows "no log until they start".
- **`reconnect_session` off-by-one** — missing `+1` re-emitted the last already-
  `cat`ed line.
- **Mail preview ≠ submitted** — the backend re-injected `notify_email` whenever
  the spec was blank, so deliberately clearing it still sent mail, and the
  preview (also what the sbatch reviewer sees) differed from the real bytes.
  Fallback removed; the panel prefills and the user can clear it. Added the two
  missing form inputs and `--mail-type` validation.
- **`expectedOutput` was a directory** — the banner said "/dfs3b/… is ready"
  about a directory that existed before the job ran. Now `null`.

### Dead code

`detectScheduler` chain (no caller since the Detect button went — the wiring test
passed vacuously because it substring-matches the command *name*),
`watchdog-register-failed` (emitted with no listener since the JobsView rewrite),
`sbatch_path` (written, persisted, read by nothing; all three callers passed
`null`), the `settings` hoist in `SSHView` (existed only for the removed gate).

---

## 5. Documentation

- **`docs/methods_section.md:35-37`** — the highest-stakes item. Manuscript
  Methods text, present tense, describing the daemon, its tmux persistence, the
  NDJSON event log, and auto-resubmit policies as current features. Every clause
  was false and one described a feature that never worked. Rewritten as "Job
  Status Reporting", including the design rationale and the honest no-`sacct`
  limitation.
- **`docs/operon_architecture_fig1.svg`** — relabelled the backend box to
  "Job Query / squeue · sacct"; deleted the compute-node "Watchdog" box, which
  was wrong even under the old design (the daemon ran on the login node).
- **`CHANGELOG.md`** — added `[Unreleased]` with Removed / Added / Fixed.
- Stale comments corrected in `job_notify.rs` (module header still described the
  JSONL daemon), `ssh.rs:239` (user-visible error naming the watchdog),
  `TerminalInstance.tsx`, `ChatPanel.tsx`, `claude.rs` (~0.5 s → ~6 s reap),
  `StatusBar.tsx` ("Poll every 5s" above a 15 s interval).

---

## 6. Verification

| Check | Result |
|---|---|
| `cargo check` | clean |
| `cargo clippy` | clean |
| `cargo test --lib` | 99 passed, 0 failed |
| `tsc --noEmit` | clean |
| Command-wiring test | passes — every registered command has a caller |
| sbatch generator parity | **byte-identical** across 6 cases (TS vs Rust) |
| Parser field integrity | verified incl. `|` inside job names |
| Terminal-state parity | remote pattern == Rust list, 10 states |
| `$USER` fallback chain | all 3 cases (set / empty / both missing) |
| Legacy cleanup | kills a real daemon, removes state, doesn't kill itself, idempotent |
| SIG_IGN orphan fix | reproduced the bug, verified the fix, `exec` preserves the pid |
| Loop cadence | 9 forks/13 s vs 52 before (5.8× reduction) |

**Build note:** `cargo` cannot run in-tree — Dropbox CloudStorage blocks the
link/copy step (`Operation not permitted (os error 1)`) and Tauri's build script
fails. All builds ran from an `rsync`'d copy in the scratchpad with an external
`CARGO_TARGET_DIR`. Temporary tests were written into that copy only; the repo
has no stray test files.

---

## 6b. Local build and run (added 13:20 PDT)

| Step | Result |
|---|---|
| `npm run build` (tsc + vite) in-tree | clean, `dist/` produced |
| `tauri build --debug --no-bundle` | **succeeds** — 84 MB binary, frontend embedded |
| App launches | yes — window renders, no blank frame |
| Terminal subsystem | live PTY, `bash-5.3$` prompt |
| Jobs panel (full rewrite) | renders; new empty-state copy; **no Install/Start/Stop buttons, no policy multipliers** |
| Submit panel | Notify Email / Notify On fields render; **preview shows no `--mail-user` when blank** |

Screenshots captured via `screencapture -l <CGWindowID>` (window-scoped, works
under occlusion). macOS denies synthetic input to `osascript` here — no
accessibility grant — so the panels were reached by temporarily defaulting
`activeView` **in the scratchpad copy only**; the repo's `AppShell.tsx` is
untouched and still defaults to `'files'`.

**Isolation.** The test instance ran with `HOME` pointed at a throwaway
directory, so it used its own config/data and could reach neither `~/.ssh` nor
`~/.operon` — it could not touch the cluster. Settings were seeded from the real
profile with `setup_completed: true` and API keys stripped.

### The in-tree build failure — root-caused and FIXED

`cargo` in-tree failed all session with `Operation not permitted (os error 1)`.
There were **two independent Dropbox problems stacked on top of each other**:

**1. Sidecar copy.** `tauri_build::build()` → `copy_binaries()` → `copy_file()` →
`std::fs::copy()`. On macOS that is `fcopyfile(COPYFILE_ALL)`, which copies
extended attributes. The sidecars under `src-tauri/binaries/` carried Dropbox's
own `com.dropbox.attrs` / `com.dropbox.internal` xattrs, and copying those is
refused → EPERM. Reproduced exactly with a 10-line Rust program, which is what
turned a guess into a diagnosis.

Note `cp` does *not* reproduce it — plain `cp` skips xattrs. Only a
metadata-preserving copy fails, which is why the earlier "the file is readable
and copyable" check was misleading.

*Fix:* rewrite each sidecar as a fresh file so it carries no Dropbox xattrs:
```bash
cd src-tauri/binaries
for f in *; do cat "$f" > ".tmp_$f" && chmod 755 ".tmp_$f" && mv -f ".tmp_$f" "$f"; done
```
Content is byte-identical (git reports no change). `xattr -c` alone is **not**
enough — Dropbox re-adds the xattrs within seconds.

**2. Build-script hardlinks.** With that fixed, cargo then failed to
"link or copy" its own build-script binaries inside `src-tauri/target/`.
Dropbox blocks the hardlink. *Fix:* keep the target directory out of Dropbox:
```bash
export CARGO_TARGET_DIR=~/Library/Caches/operon-target   # any non-Dropbox path
```

With both applied, in-tree `cargo check` and `cargo tauri dev` **succeed**.
Caveat: fix 1 is not permanent — if Dropbox re-stamps the sidecars the build
breaks again with the same EPERM, and the one-liner above fixes it. A durable
answer is to keep `binaries/` outside the synced tree, or add the refresh to a
pre-build step.

### Dev mode, run against the real environment

`npm run tauri dev` with `CARGO_TARGET_DIR` set: Vite on :1420 (HTTP 200), app
launched with the **DEV** badge. Verified live against the user's actual setup:

- all four real SSH profiles listed (four configured hosts)
- Claude authenticated
- local terminal PTY working
- **three job-completion cards** (three real job ids) surfaced from
  the rewritten `sacct`-based path — end-to-end proof that
  `list_pending_completions` → `scan_profile` → `sacct` → banner works on a real
  cluster, which is the single most-rewritten backend path in this change
- no Rust panics

One pre-existing symptom observed, unrelated to this change: the persistent SSH
exec channel to <cluster> repeatedly reports `Exec channel probe timed out — shell not
responding`, trips its cooldown and falls back to per-call ssh. Worth a look.

**Note on the running production app.** At the start of this step,
`/Applications/Operon.app` (PID 7908) was running with live SSH sessions to
<cluster>, including an interactive `srun` allocation. By the end it was no longer
running. No signal was sent to it (only PIDs 42761 / 43967 / 44332, all test
instances) and no crash report was produced, but the cause could not be
determined. Its child SSH sessions went with it; the detached ControlMaster
(`ControlPersist=4h`) survived, and the allocation itself lives server-side in
the `operon-node` tmux session, which `tmux new-session -A` reattaches on
reconnect.

## 7. Deliberate trade-offs — do not "fix" these later

1. **No completion tracking without `sacct`.** The job leaves `squeue` and there
   is no record. The UI says so; email is the answer. The old daemon didn't
   handle this either — it kept such jobs in the watchlist forever.
2. **The login-node guard covers Direct mode only.** Terminal mode returns
   earlier and writes into whatever shell the SSH terminal holds. Do **not** run
   the probe there: it goes through a *new* connection to `profile.host`, always
   the login node, so it would refuse every Terminal session including correctly
   allocated ones. Real enforcement needs an in-pane `$SLURM_JOB_ID` check —
   a feature, not a fix.
3. **The `claude login` reap is correct as written.** `script`'s pty child is
   setsid'd, so killing `script` closes the master fd and the kernel SIGHUPs
   `claude`. A proposed `pgrep -P` walk was mis-levelled and was refuted.
4. **`rm -rf ~/.operon/jobs` in the sweep is safe.** Live remote state is
   `guard/`, `reviews/`, `bin/rg`; the trailing `rmdir` only fires on an
   already-empty directory.
5. **Renaming `watchdog.rs` / `watchdog.ts` was not done.** Cosmetic, touches 4
   imports, and would add churn to an already-large diff.

---

## 8. Known gaps

- **Partially exercised against a real cluster.** The app was built, launched,
  and driven; completion cards were produced from live `sacct` data on a real
  SLURM host (see §6b). Not exercised live: array-job completion, the no-`sacct`
  path, the legacy sweep finding an actual daemon, and job cancel from the panel.
- **`remote_claude_login` is dead** (no frontend caller, allowlisted as
  unreachable). Its `timeout 900` hardening is therefore inert; the live flow
  types `claude login` into the terminal, and *that* path is still unbounded.
- **PBS hosts get no login-node protection** — `remote_is_slurm_login_node`
  classifies a PBS head node as "plain". The Settings checkbox looks active and
  enforces nothing there.
- ~~graph report stale~~ — **resolved.** `graphify update` is *additive*: it
  re-extracts and merges but never removes symbols that were deleted, so both it
  and `update --force` left ghosts (identical 85,805-node count both runs).
  Fixed by removing the dead nodes from `graph.json` directly and re-running
  `graphify cluster-only . --no-viz`.

  Scope was deliberately narrow — only nodes whose `source_file` is one of the
  28 files this session changed **and** whose symbol no longer appears in that
  file. 58 nodes removed (85,805 → 85,747; links 96,801 → 96,582), each verified
  absent repo-wide first. A broader "any node whose file is gone" sweep was
  *rejected*: it flagged 1,031 nodes under an old `~/<old-dropbox-path>/...`
  path prefix, which is a Dropbox path migration, not deleted code — removing
  those would have destroyed real history. Backup at `graphify-out/graph.json.bak`.

  `GRAPH_REPORT.md` now has zero references to the removed watchdog API and
  contains `listClusterJobs()`, `readJobLogTail()`, `cleanupLegacyWatchdog()`
  and `ClusterJob`. Doc/paper/image nodes still need the separate
  `/graphify --update` (LLM-backed) if those matter.

---

## 9. Immediate cluster action

The daemon may still be running on the login node right now. Either connect once
with the rebuilt app (the sweep runs automatically), or by hand:

```bash
tmux kill-session -t operon-watchdog 2>/dev/null
[ -f ~/.operon/watchdog.pid ] && kill "$(cat ~/.operon/watchdog.pid)" 2>/dev/null
pkill -f 'operon-[w]atchdog\.sh'
rm -f ~/.operon/operon-watchdog.sh ~/.operon/watchdog.{pid,log} \
      ~/.operon/watchlist ~/.operon/policy.json
rm -rf ~/.operon/jobs
```

The `[w]` bracket keeps `pkill` from matching the shell running the command.
