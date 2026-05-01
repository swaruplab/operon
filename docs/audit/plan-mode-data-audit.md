# Plan-Mode Data Audit — Operon + Claude Code

**Status:** active reference doc
**Scope:** local laptop only. No SSH, no MCP, Plan mode only.
**Goal:** determine exactly what Operon/Claude Code transmit to the Anthropic
Messages API when the user asks Claude to plan analysis of a synthetic,
PHI-shaped genomics dataset.

This is a dev-mode observability exercise, not a pen-test. The harness uses
canary strings seeded throughout a small fake dataset, intercepts HTTPS with
mitmproxy, and scans every outbound JSON body for those canaries. A canary that
shows up in a request body tells us exactly which piece of data crossed the
wire, and in which field of the payload.

## 1 — Threat model / questions we want answered

For a researcher about to use Operon on real patient-derived data:

1. Does Plan mode send file **contents** or only file **paths**?
2. If Claude reads a file during planning, is the full body uploaded, or a
   tool-result excerpt?
3. Does the `system` prompt contain anything sensitive about the project
   (cwd, env, directory listings)?
4. Does Operon auto-inject files (e.g. `implementation_plan.md`) without an
   explicit user ask?
5. Do files the user never opened (e.g. a sibling `patient_identifiers.csv`)
   end up in a payload anyway via directory-listing tools?

## 2 — Canary inventory

All canaries match `CANARY_[A-Z0-9_]+` so the addon can find them with one
regex. They are embedded in `scripts/audit/canary-dataset/`.

| Canary token | Where it lives | What a hit means |
|---|---|---|
| `CANARY_GENE_SECRET` | last row of `counts.csv` | Counts matrix contents were sent |
| `CANARY_AGE_73` .. `CANARY_AGE_72` | `age` column of `metadata.csv` | Per-sample demographics transmitted |
| `CANARY_DX_AD`, `CANARY_DX_CTRL` | `diagnosis` column | Diagnosis field transmitted |
| `CANARY_MRN_88471` .. `CANARY_MRN_88478` | `mrn` column | MRN-like PHI transmitted |
| `CANARY_NOTE_clinical_42` .. `_49` | `note` column | Free-text clinical notes transmitted |
| `CANARY_FILE_SHOULD_NEVER_LEAVE_*` | `patient_identifiers.csv` | Sidecar file we never asked Claude to touch was read/transmitted anyway |
| `CANARY_PLAN_FILE_CONTENT_header_abc123` | `implementation_plan.md` | Plan-file contents auto-injected as context |

## 3 — What the harness does

- `scripts/audit/mitm-addon.py` — mitmproxy addon. Filters requests to
  `api.anthropic.com`, writes each request body to
  `<audit_dir>/flows/<n>-request.json`, walks the JSON, logs every canary
  hit to `<audit_dir>/canaries.tsv` with the JSON path where it was found.
- `scripts/audit/run-audit.sh` — convenience launcher: verifies mitmproxy is
  installed, bootstraps its CA cert if missing, prints the env-export commands
  you need in a second terminal, then starts `mitmweb` with the addon.
- `scripts/audit/canary-dataset/` — the fake project you open in Operon.

## 4 — How to execute (precise)

### 4a. One-time setup

```bash
# macOS:
brew install mitmproxy

# Start mitmproxy once so it writes its CA to ~/.mitmproxy/
mitmproxy --help >/dev/null
```

(Optional) Trust the mitmproxy CA system-wide if you want Safari/Chrome to
stop warning — not required for this audit because Node (which Claude Code
runs under) ignores the system store anyway and uses `NODE_EXTRA_CA_CERTS`.

### 4b. Terminal A — start the intercepting proxy

```bash
cd "/path/to/operon_crossplatform"
./scripts/audit/run-audit.sh
```

This opens mitmweb at http://127.0.0.1:8081 and listens on 127.0.0.1:8080.
Leave it running. Capture output goes to `tmp/audit-out/`.

### 4c. Terminal B — start Operon with the proxy env vars

**You must launch Operon from a terminal**, not from Finder, or these env
vars will not reach the `claude` subprocess.

```bash
cd "/path/to/operon_crossplatform"
export HTTPS_PROXY=http://127.0.0.1:8080
export HTTP_PROXY=http://127.0.0.1:8080
export NODE_EXTRA_CA_CERTS="$HOME/.mitmproxy/mitmproxy-ca-cert.pem"
cargo tauri dev
```

### 4d. In Operon

1. Settings → AI provider → Anthropic (not custom). Enter a real API key.
2. File → Open Folder → select
   `scripts/audit/canary-dataset/` inside this repo.
3. In the chat panel, set mode to **Plan** (not Agent).
4. Send this prompt verbatim:

   > I have a counts matrix and sample metadata in this directory. Plan a
   > differential-expression analysis comparing the AD and control groups.
   > Don't write code, just plan it.

5. Wait for Claude to finish. You should see an `implementation_plan.md` get
   written (or updated).

### 4e. Inspect

```bash
# Per-canary hit log (one line per canary appearance per request)
cat tmp/audit-out/canaries.tsv

# Full request bodies, human-readable
ls tmp/audit-out/flows/
jq . tmp/audit-out/flows/1-request.json | less
```

## 5 — Pass / fail criteria

| Canary class | Expected | Concerning |
|---|---|---|
| `CANARY_PLAN_FILE_CONTENT_*` | **Hit** (plan file is auto-injected by design) | No hit → plan injection broken |
| `CANARY_GENE_SECRET` | Hit only if Claude explicitly Read the file | Hit on turn 1 without a Read tool call → content uploaded as bulk context |
| `CANARY_AGE_*`, `CANARY_DX_*` | Same as above | Demographics uploaded without an explicit read is the PHI leak we're hunting |
| `CANARY_MRN_*` | Hit ⇒ MRN-shaped strings are crossing the wire | This is the loudest failure mode |
| `CANARY_FILE_SHOULD_NEVER_LEAVE_*` | **No hit** | Any hit = sibling file read without ask (e.g. via directory listing + read) |
| `CANARY_NOTE_*` | Hit iff Claude Read `metadata.csv` | Same rule as demographics |

Also sanity-check the `system` field of each request body (in
`flows/<n>-request.json`, path `$.system`) to see what "system context"
Claude Code is injecting — cwd, OS, git status, etc.

## 6 — Known limitations of this harness

- Plan mode ≤ 3 turns, so payload count is tiny. Extend turns by running
  follow-up prompts if you want more surface area.
- This audits `api.anthropic.com` only. When `ai_provider == "custom"` and
  the translation proxy is on, the Anthropic hostname is never contacted —
  you'd point `HTTPS_PROXY` at the proxy or log at the proxy instead.
- mitmproxy can't decrypt traffic that uses cert-pinning. Claude Code does
  not pin, but if Anthropic ever adds pinning this harness breaks silently.
- This only covers what leaves the host. It says nothing about Anthropic's
  retention policy once bytes arrive there — that's a separate policy
  question, not an observability question.

## 7 — Extensions (out of scope for v1)

- Replace the single-prompt script with a pytest-style matrix: Plan vs Agent
  vs `--resume`, with/without MCP servers, with/without a seeded plan file.
- Add a second addon that scans **responses** too, to see whether Anthropic
  ever echoes PHI back (it shouldn't, but cheap to verify).
- Wire the translation-proxy sidecar into the loop so we can audit what
  Claude sends to a local Ollama and compare.
