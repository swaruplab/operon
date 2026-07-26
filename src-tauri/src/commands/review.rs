//! Light code reviewer — a cheap, checklist-driven second opinion on the code
//! the agent writes, run on a *different* model (Sonnet 5 by default) with a
//! *fresh* context.
//!
//! ## Why a different model, and why fresh context
//!
//! Asking the same conversation that just wrote the code to grade its own work
//! is the weakest possible check — it inherits its own rationalisations. Running
//! the review as a separate one-shot invocation on a different model means
//! different weights (different blind spots) and no exposure to the actor's
//! reasoning chain. It is cheap enough to run before every cluster submission.
//!
//! ## Why this is a *methods* reviewer, not a linter
//!
//! In computational biology the code IS the methods section. Feeding
//! log-normalised data to scVI, clustering then running DE on the same cells,
//! or testing a condition across cells when a donor column exists are all
//! *scientifically wrong results* that are plainly visible in the source. Those
//! are what this looks for. It deliberately does NOT report style, naming,
//! docstrings or micro-optimisations: noise is how a feature like this gets
//! switched off within a week.
//!
//! ## Design constraints
//!
//!   * **Advisory only.** Findings never hard-block; the caller can always
//!     proceed. We never silently rewrite the user's analysis code — a silent
//!     edit to a pipeline is an irreproducibility hazard.
//!   * **Hard severity gate + cap.** Only `blocking` / `high`, max 3 findings.
//!     Enforced here in Rust as well as in the prompt (defence in depth).
//!   * **No tools.** The reviewer gets the code as text and may not touch the
//!     filesystem or shell — keeps it fast, cheap, and side-effect free.
//!   * **Bias to false negatives.** A noisy reviewer is worse than none.

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

/// How long we let the reviewer think before giving up. A review that outlives
/// this is worse than no review — the whole point is that it's cheap next to
/// the job it guards.
const REVIEW_TIMEOUT_SECS: u64 = 120;

/// Never send more than this much code — protects context, cost and latency.
const MAX_CODE_BYTES: usize = 160_000;

/// Hard cap on surfaced findings. Alarm fatigue is the failure mode.
const MAX_FINDINGS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFinding {
    /// "blocking" | "high" — anything else is dropped.
    pub severity: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub line: Option<u32>,
    pub title: String,
    /// The concrete consequence: what will be wrong, and why.
    #[serde(default)]
    pub why_wrong: String,
    #[serde(default)]
    pub fix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewResult {
    pub findings: Vec<ReviewFinding>,
    /// Model that actually performed the review.
    pub model: String,
    /// True when the reviewer ran and found nothing on its checklist. Note this
    /// means "no known pitfalls detected", NOT "this code is correct".
    pub clean: bool,
}

// ─── Prompt ─────────────────────────────────────────────────────────────────

const PREAMBLE: &str = r#"You are a skeptical computational-biology METHODS reviewer.

You are reviewing code an AI agent just wrote for a real research analysis. Your ONLY job is to catch mistakes that would (a) silently produce SCIENTIFICALLY WRONG RESULTS, or (b) WASTE HOURS OF CLUSTER COMPUTE.

Return ONLY a JSON object. No prose, no explanation, no markdown fences:

{"findings":[{"severity":"blocking","category":"<short-kebab-case>","line":<int or null>,"title":"<one line>","why_wrong":"<the concrete consequence: what exactly will be wrong, and why>","fix":"<the specific change to make>"}]}

RULES — these matter more than finding something:
- Report ONLY violations of the CHECKLIST below. Nothing else.
- NEVER report style, naming, docstrings, type hints, formatting, import order, error handling, or micro-optimisations. That is noise, and noise gets this reviewer switched off.
- "blocking" = will silently produce wrong science, or will crash / waste hours of compute.
  "high"     = likely wrong, or a serious risk to the result.
  Use no other severities. If it is not one of these, do not report it.
- Maximum 3 findings, most severe first.
- If nothing on the checklist is violated, return exactly {"findings":[]}.
- STRONGLY prefer false negatives over false positives. If you are not confident the code ACTUALLY does the wrong thing, say nothing. An empty result is a good result.
- Cite the specific line number from the numbered listing.
- Do not speculate about code, data or files you cannot see. Judge only what is in front of you.
"#;

/// Single-cell / genomics analysis pitfalls. These are the errors that produce
/// a publishable-looking figure and a false conclusion.
const CHECKLIST_SCRIPT: &str = r#"CHECKLIST — analysis code (Python/R, single-cell & genomics):

1. RAW-COUNT INTEGRITY: scVI / scANVI / TotalVI / MultiVI / PeakVI trained on normalised or log1p'd data instead of raw counts. These models REQUIRE raw counts (e.g. `layer="counts"`). Fitting them on log-normalised `.X` produces a silently garbage latent space — no error, no warning, a normal-looking UMAP.
2. WRONG MODEL FOR THE MODALITIES: RNA+protein (CITE-seq) -> TotalVI. RNA+ATAC (multiome) -> MultiVI. ATAC alone -> PeakVI. Many samples/donors compared -> mrVI. MultiVI is NOT for protein data; TotalVI is NOT for ATAC.
3. CIRCULAR ANALYSIS / DOUBLE-DIPPING: clustering (leiden/louvain) and then testing differential expression BETWEEN THOSE SAME CLUSTERS on the SAME cells (e.g. `sc.tl.leiden` then `sc.tl.rank_genes_groups` on the same object) without count-splitting or selective inference. This guarantees "significant" genes and is not a valid test.
4. PSEUDOREPLICATION: testing a condition/disease/genotype effect ACROSS CELLS when a donor / sample / patient / subject column exists in `.obs`. n is the number of DONORS, not cells. 20,000 cells from 6 donors is n=6. Requires pseudobulk per donor or a mixed model. This massively inflates significance and is the single most common fatal error in scRNA-seq.
5. BATCH CONFOUNDED WITH CONDITION: the batch key is perfectly aligned with the biological condition of interest (all controls in one batch, all cases in another). No correction can rescue this — the effect is unidentifiable.
6. QC / NORMALISATION ORDERING: QC filtering applied AFTER normalisation; normalising or log1p-ing data that is already normalised/logged; scaling before subsetting in a way that leaks.
7. DOUBLETS / AMBIENT RNA not removed before clustering or annotation -> phantom "co-expressing" populations that get annotated as novel cell types.
8. HVG selection without `batch_key` when multiple batches exist -> HVGs driven by batch, not biology.
9. Writing to an AnnData VIEW (subsetting without `.copy()`) and then modifying it.
10. Missing multiple-testing correction, or interpreting raw p-values as significant.
11. Non-determinism where it matters: no seed / `random_state` for leiden, UMAP, or model training, in code whose output is being interpreted.
"#;

/// HPC batch-script pitfalls. The asymmetry here is the whole point: a few
/// seconds of review against hours of queued cluster time.
const CHECKLIST_SBATCH: &str = r#"CHECKLIST — HPC batch (sbatch) script:

1. NODE-LOCAL /tmp: writing results, checkpoints or outputs into /tmp (or $TMPDIR) on a compute node. /tmp is node-local on HPC clusters — those files are INVISIBLE from the login node and are destroyed when the job ends. Outputs must go to the shared filesystem (the working directory / home / scratch on shared storage).
2. RESOURCES IMPLAUSIBLE FOR THE WORK: GPU training (torch / scvi-tools / any deep model) with no `--gres=gpu:...` requested -> runs on CPU, 50x slower, times out. Or a large dataset with a `--mem` far too small -> OOM-killed hours in. Or a walltime obviously too short for the described work -> job is killed at the end and ALL progress is lost.
3. NO ENVIRONMENT ACTIVATION: no `conda activate` / `module load` / venv activation before invoking python/R -> the job either fails instantly or silently runs against the wrong interpreter and wrong package versions.
4. NO `set -euo pipefail` (or equivalent): a failing intermediate step is silently ignored and downstream steps run on partial or missing data, producing plausible-looking but wrong output.
5. FRAGILE PATHS: relative paths that assume a particular submit directory, or obvious placeholder paths (/path/to/..., TODO, CHANGEME).
6. GPU requested but never used, or an array job whose tasks all overwrite the same output file.
"#;

fn checklist_for(mode: &str) -> &'static str {
    match mode {
        "sbatch" => CHECKLIST_SBATCH,
        _ => CHECKLIST_SCRIPT,
    }
}

/// Number the lines so the model can cite them accurately.
fn with_line_numbers(code: &str) -> String {
    code.lines()
        .enumerate()
        .map(|(i, l)| format!("{:>4} | {}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_prompt(code: &str, mode: &str, filename: Option<&str>) -> String {
    let mut code = code.to_string();
    if code.len() > MAX_CODE_BYTES {
        code.truncate(MAX_CODE_BYTES);
        code.push_str("\n... [truncated]");
    }
    let name = filename.unwrap_or(if mode == "sbatch" {
        "job.sbatch"
    } else {
        "analysis"
    });
    format!(
        "{preamble}\n{checklist}\nCODE UNDER REVIEW ({name}):\n\n{numbered}\n\nReturn only the JSON object.",
        preamble = PREAMBLE,
        checklist = checklist_for(mode),
        name = name,
        numbered = with_line_numbers(&code),
    )
}

// ─── Output parsing ─────────────────────────────────────────────────────────

/// Claude Code's `--output-format json` envelope. We only need `.result`, which
/// carries the assistant's text.
#[derive(Deserialize)]
struct ClaudeJsonEnvelope {
    #[serde(default)]
    result: String,
}

#[derive(Deserialize)]
struct FindingsPayload {
    #[serde(default)]
    findings: Vec<ReviewFinding>,
}

/// Pull the first balanced `{...}` object out of a string. The model is told to
/// emit bare JSON, but it may still wrap it in ``` fences or add a stray
/// sentence — be forgiving rather than failing the whole review over it.
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Severity gate + cap. Enforced here so a chatty model can't flood the UI even
/// if it ignores the prompt.
fn gate(mut findings: Vec<ReviewFinding>) -> Vec<ReviewFinding> {
    findings.retain(|f| {
        let s = f.severity.to_lowercase();
        (s == "blocking" || s == "high") && !f.title.trim().is_empty()
    });
    // blocking before high, otherwise preserve model order.
    findings.sort_by_key(|f| {
        if f.severity.to_lowercase() == "blocking" {
            0
        } else {
            1
        }
    });
    findings.truncate(MAX_FINDINGS);
    findings
}

// ─── Command ────────────────────────────────────────────────────────────────

/// Review a snippet of code and return gated findings.
///
/// `mode` is "sbatch" (HPC batch script) or "script" (analysis code). The caller
/// supplies `model`/`effort` from settings so this stays free of settings-state
/// coupling. Errors are returned verbatim — the frontend treats a failed review
/// as advisory-unavailable and MUST still let the user proceed.
#[tauri::command]
pub async fn review_code(
    code: String,
    mode: String,
    filename: Option<String>,
    model: Option<String>,
    effort: Option<String>,
) -> Result<ReviewResult, String> {
    if code.trim().is_empty() {
        return Err("Nothing to review".to_string());
    }

    let model = model.unwrap_or_else(|| "claude-sonnet-5".to_string());
    let effort = effort.unwrap_or_else(|| "low".to_string());
    let prompt = build_prompt(&code, &mode, filename.as_deref());

    // Absolute path + augmented PATH: a GUI app launched from Finder/Dock has a
    // minimal PATH and would otherwise hit "command not found: claude".
    let claude_path = crate::platform::resolve_claude_path()
        .ok_or_else(|| "Claude Code CLI not found — install it or check your PATH.".to_string())?;
    let (program, prefix_args) = crate::platform::spawn_resolve(&claude_path);

    let mut cmd = tokio::process::Command::new(&program);
    cmd.args(&prefix_args);
    cmd.arg("-p"); // prompt arrives on stdin
    cmd.args(["--output-format", "json"]);
    cmd.args(["--max-turns", "1"]);
    // Pure text analysis: no filesystem, no shell. Fast, cheap, side-effect free.
    cmd.args([
        "--disallowedTools",
        "Read,Write,Edit,Bash,Glob,Grep,WebFetch,WebSearch",
    ]);
    cmd.args(["--model", &model]);
    // Only pass --effort when this model actually supports the level, otherwise
    // the CLI rejects it (Haiku has no effort; Sonnet 4.6 caps at high).
    if !effort.is_empty() && super::models::model_supports_effort_level(&model, &effort) {
        cmd.args(["--effort", &effort]);
    }
    cmd.env("PATH", crate::platform::augmented_path());
    if let Some(gb) = crate::platform::find_git_bash_path() {
        cmd.env("CLAUDE_CODE_GIT_BASH_PATH", gb);
    }
    // The reviewer is Anthropic-direct by construction — it hardcodes a
    // `claude-*` model id that no gateway would accept — so it must run on the
    // CLI's own credential. Clear the inherited auth vars rather than depend on
    // how Operon was launched: a stale ANTHROPIC_API_KEY in the environment
    // (Operon started from a terminal whose profile exports one) outranks the
    // user's claude.ai login, and because stderr is discarded below the failure
    // surfaces only as a silent "reviewer unavailable".
    for var in super::claude::MANAGED_AUTH_VARS {
        cmd.env_remove(var);
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());
    // If we time out and drop the future, don't leave a claude process behind.
    cmd.kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = cmd.spawn().map_err(|e| format!("spawn reviewer: {}", e))?;

    // Feed the prompt over stdin rather than argv: large scripts would blow the
    // 32KB Windows CreateProcess limit, and shell-quoting arbitrary code is a
    // minefield.
    {
        let mut stdin = child.stdin.take().ok_or("reviewer stdin unavailable")?;
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|e| format!("write prompt: {}", e))?;
        stdin.shutdown().await.ok();
    } // dropped here -> EOF

    let out = tokio::time::timeout(
        std::time::Duration::from_secs(REVIEW_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| format!("Reviewer timed out after {}s", REVIEW_TIMEOUT_SECS))?
    .map_err(|e| format!("reviewer failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.trim().is_empty() {
        return Err("Reviewer returned no output".to_string());
    }

    // Unwrap the CLI envelope, then find the model's JSON inside `.result`.
    let text = serde_json::from_str::<ClaudeJsonEnvelope>(&stdout)
        .map(|e| e.result)
        .unwrap_or_else(|_| stdout.to_string());

    let payload = extract_json_object(&text)
        .and_then(|j| serde_json::from_str::<FindingsPayload>(j).ok())
        .ok_or_else(|| "Reviewer returned an unparseable response".to_string())?;

    let findings = gate(payload.findings);
    Ok(ReviewResult {
        clean: findings.is_empty(),
        findings,
        model,
    })
}

// ─── Review events (visible chip channel) ──────────────────────────────────
//
// PreToolUse hook decisions never appear in Claude Code's `stream-json` output
// (verified), so the sbatch-review hook APPENDS one JSON record per review to a
// per-session file `~/.operon/reviews/<session>.jsonl` (shared home on HPC →
// readable from the login node). Operon polls this to render a review chip in
// the chat, so a CLEAN review is as visible as a blocked one.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewEvent {
    #[serde(default)]
    pub ts: i64,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub script: String,
    /// "clean" | "blocked" | "unavailable"
    pub outcome: String,
    #[serde(default)]
    pub n: u32,
    #[serde(default)]
    pub findings: Vec<serde_json::Value>,
}

/// Read the per-session review-events file the guard hook appends to. Local:
/// read `~/.operon/reviews/<session>.jsonl`. Remote: `cat` it over SSH (shared
/// home). Never errors on a missing file — an empty vec means "no reviews yet".
#[tauri::command]
pub async fn read_review_events(
    ssh_state: tauri::State<'_, crate::commands::ssh::SSHManager>,
    session_id: String,
    profile_id: Option<String>,
) -> Result<Vec<ReviewEvent>, String> {
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe.is_empty() {
        return Ok(vec![]);
    }
    let content = match profile_id {
        Some(pid) => {
            let profile = {
                let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
                profiles.iter().find(|p| p.id == pid).cloned()
            };
            match profile {
                Some(p) => crate::commands::ssh::ssh_exec_async(
                    p,
                    format!(
                        "cat \"$HOME/.operon/reviews/{}.jsonl\" 2>/dev/null || true",
                        safe
                    ),
                )
                .await
                .unwrap_or_default(),
                None => String::new(),
            }
        }
        None => match dirs::home_dir() {
            Some(h) => std::fs::read_to_string(
                h.join(".operon")
                    .join("reviews")
                    .join(format!("{}.jsonl", safe)),
            )
            .unwrap_or_default(),
            None => String::new(),
        },
    };
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(ev) = serde_json::from_str::<ReviewEvent>(line) {
            out.push(ev);
        }
    }
    Ok(out)
}

/// Runtime on/off for the sbatch review, driven by the chat-box checkbox.
///
/// The reviewer's persisted default lives in settings (`reviewer_auto_sbatch`,
/// baked into the per-session env at start). This flips it mid-session without
/// a restart: the guard hook skips review when `~/.operon/guard/review-off`
/// exists, so `off=true` drops that marker and `off=false` removes it — on the
/// same host the hook runs (local fs, or the remote shared home over SSH).
/// Best-effort and fail-safe: any error is swallowed (worst case the toggle
/// applies from the next session, per the setting).
#[tauri::command]
pub async fn set_review_marker(
    ssh_state: tauri::State<'_, crate::commands::ssh::SSHManager>,
    off: bool,
    profile_id: Option<String>,
) -> Result<(), String> {
    // Local marker (the hook that runs on this machine reads it).
    if let Some(h) = dirs::home_dir() {
        let dir = h.join(".operon").join("guard");
        let marker = dir.join("review-off");
        if off {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(&marker, b"");
        } else {
            let _ = std::fs::remove_file(&marker);
        }
    }
    // Remote marker (an active remote session's hook runs on the cluster).
    if let Some(pid) = profile_id {
        let profile = {
            let profiles = ssh_state.profiles.lock().map_err(|e| e.to_string())?;
            profiles.iter().find(|p| p.id == pid).cloned()
        };
        if let Some(p) = profile {
            let cmd = if off {
                "mkdir -p \"$HOME/.operon/guard\" 2>/dev/null && : > \"$HOME/.operon/guard/review-off\""
            } else {
                "rm -f \"$HOME/.operon/guard/review-off\""
            };
            let _ = crate::commands::ssh::ssh_exec_async(p, cmd.to_string()).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(sev: &str, title: &str) -> ReviewFinding {
        ReviewFinding {
            severity: sev.to_string(),
            category: String::new(),
            line: None,
            title: title.to_string(),
            why_wrong: String::new(),
            fix: String::new(),
        }
    }

    #[test]
    fn gate_drops_non_blocking_severities() {
        let out = gate(vec![
            f("low", "nit: rename this"),
            f("medium", "add a docstring"),
            f("high", "real problem"),
        ]);
        assert_eq!(out.len(), 1, "only blocking/high survive");
        assert_eq!(out[0].title, "real problem");
    }

    #[test]
    fn gate_sorts_blocking_first_and_caps_at_three() {
        let out = gate(vec![
            f("high", "h1"),
            f("high", "h2"),
            f("blocking", "b1"),
            f("high", "h3"),
            f("blocking", "b2"),
        ]);
        assert_eq!(out.len(), MAX_FINDINGS);
        assert_eq!(out[0].severity, "blocking");
        assert_eq!(out[1].severity, "blocking");
    }

    #[test]
    fn gate_drops_empty_titles() {
        assert!(gate(vec![f("blocking", "   ")]).is_empty());
    }

    #[test]
    fn extract_json_object_handles_markdown_fences_and_prose() {
        let s = "Sure!\n```json\n{\"findings\": [{\"a\": 1}]}\n```\nhope that helps";
        let j = extract_json_object(s).unwrap();
        assert_eq!(j, "{\"findings\": [{\"a\": 1}]}");
    }

    #[test]
    fn extract_json_object_ignores_braces_inside_strings() {
        let s = r#"{"findings":[{"title":"has a } brace"}]}"#;
        let j = extract_json_object(s).unwrap();
        assert_eq!(j, s, "must not terminate early on a brace inside a string");
    }

    #[test]
    fn parses_a_clean_empty_review() {
        let p: FindingsPayload = serde_json::from_str(r#"{"findings":[]}"#).unwrap();
        assert!(gate(p.findings).is_empty());
    }

    #[test]
    fn sbatch_mode_uses_the_hpc_checklist() {
        let p = build_prompt("echo hi", "sbatch", None);
        assert!(p.contains("NODE-LOCAL /tmp"));
        assert!(!p.contains("RAW-COUNT INTEGRITY"));
    }

    #[test]
    fn script_mode_uses_the_single_cell_checklist() {
        let p = build_prompt("import scanpy", "script", Some("a.py"));
        assert!(p.contains("PSEUDOREPLICATION"));
        assert!(p.contains("a.py"));
        assert!(!p.contains("NODE-LOCAL /tmp"));
    }

    #[test]
    fn code_is_line_numbered_so_findings_can_cite_lines() {
        assert_eq!(with_line_numbers("a\nb"), "   1 | a\n   2 | b");
    }

    // ── End-to-end (hits the real Claude CLI) ───────────────────────────────
    // Ignored so CI never pays for it. Run locally with:
    //   cargo test commands::review -- --ignored --nocapture
    //
    // These two are the regression suite that keeps the reviewer honest:
    // `catches_seeded_bugs` measures catch rate, `stays_quiet_on_good_code`
    // measures the false-positive rate. A reviewer that cries wolf on correct
    // code is worse than no reviewer — it gets switched off.

    const BAD: &str = r#"
import scanpy as sc
import scvi
adata = sc.read_h5ad("pbmc.h5ad")   # .obs has donor_id, condition
sc.pp.normalize_total(adata, target_sum=1e4)
sc.pp.log1p(adata)
scvi.model.SCVI.setup_anndata(adata, batch_key="batch")
model = scvi.model.SCVI(adata)
model.train()
adata.obsm["X_scVI"] = model.get_latent_representation()
sc.pp.neighbors(adata, use_rep="X_scVI")
sc.tl.leiden(adata, key_added="leiden")
sc.tl.rank_genes_groups(adata, "leiden", method="wilcoxon")
sc.tl.rank_genes_groups(adata, "condition", method="wilcoxon")
"#;

    const GOOD: &str = r#"
import scanpy as sc
import scvi
import decoupler as dc
adata = sc.read_h5ad("pbmc.h5ad")
adata.layers["counts"] = adata.X.copy()
sc.pp.filter_cells(adata, min_genes=200)
sc.pp.scrublet(adata, batch_key="batch")
adata = adata[~adata.obs.predicted_doublet].copy()
sc.pp.normalize_total(adata, target_sum=1e4)
sc.pp.log1p(adata)
sc.pp.highly_variable_genes(adata, n_top_genes=2000, batch_key="batch")
scvi.settings.seed = 0
scvi.model.SCVI.setup_anndata(adata, layer="counts", batch_key="batch")
model = scvi.model.SCVI(adata)
model.train()
adata.obsm["X_scVI"] = model.get_latent_representation()
sc.pp.neighbors(adata, use_rep="X_scVI")
sc.tl.leiden(adata, key_added="leiden", random_state=0)
pdata = dc.get_pseudobulk(adata, sample_col="donor_id", groups_col="leiden",
                          layer="counts", mode="sum")
"#;

    #[tokio::test]
    #[ignore = "hits the real Claude CLI"]
    async fn e2e_catches_seeded_bugs() {
        let r = review_code(
            BAD.to_string(),
            "script".into(),
            Some("bad.py".into()),
            None,
            None,
        )
        .await
        .expect("reviewer ran");
        println!("\n=== BAD fixture -> {:#?}\n", r);
        assert!(
            !r.findings.is_empty(),
            "must catch scVI-on-lognorm / double-dipping / pseudoreplication"
        );
    }

    #[tokio::test]
    #[ignore = "hits the real Claude CLI"]
    async fn e2e_stays_quiet_on_good_code() {
        let r = review_code(
            GOOD.to_string(),
            "script".into(),
            Some("good.py".into()),
            None,
            None,
        )
        .await
        .expect("reviewer ran");
        println!("\n=== GOOD fixture -> {:#?}\n", r);
        assert!(r.clean, "false positive on correct code: {:?}", r.findings);
    }
}
