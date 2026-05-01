#!/usr/bin/env bash
# Launch mitmweb with the Operon canary-scanning addon.
#
# This is terminal A. Terminal B runs Operon itself with HTTPS_PROXY set —
# the script prints the exact export commands you need to paste into B.
#
# Docs: docs/audit/plan-mode-data-audit.md

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AUDIT_DIR="${REPO_ROOT}/tmp/audit-out"
ADDON="${REPO_ROOT}/scripts/audit/mitm-addon.py"
CA_CERT="${HOME}/.mitmproxy/mitmproxy-ca-cert.pem"

if ! command -v mitmweb >/dev/null 2>&1; then
  echo "error: mitmproxy is not installed." >&2
  echo "  macOS: brew install mitmproxy" >&2
  echo "  other: pipx install mitmproxy   (or)   pip install --user mitmproxy" >&2
  exit 1
fi

# mitmproxy only materializes its CA cert after it has run once. Bootstrap
# if needed by launching it briefly with --help.
if [[ ! -f "${CA_CERT}" ]]; then
  echo "[audit] bootstrapping mitmproxy CA at ~/.mitmproxy/ ..."
  mitmproxy --help >/dev/null 2>&1 || true
fi

if [[ ! -f "${CA_CERT}" ]]; then
  echo "error: expected CA cert not found at ${CA_CERT}" >&2
  echo "       run 'mitmproxy' once manually, then press q to quit, then re-run this." >&2
  exit 1
fi

mkdir -p "${AUDIT_DIR}/flows"
# Reset previous run's outputs so canaries.tsv is not appended across runs.
rm -f "${AUDIT_DIR}/canaries.tsv" "${AUDIT_DIR}/summary.txt"
rm -f "${AUDIT_DIR}/flows/"*.json 2>/dev/null || true

cat <<EOF

────────────────────────────────────────────────────────────────────
  Operon PHI canary audit — mitmproxy is starting.

  Listening on : http://127.0.0.1:8080
  Web UI       : http://127.0.0.1:8081
  CA cert      : ${CA_CERT}
  Output dir   : ${AUDIT_DIR}

  In a SECOND terminal (terminal B), run:

    cd "${REPO_ROOT}"
    export HTTPS_PROXY=http://127.0.0.1:8080
    export HTTP_PROXY=http://127.0.0.1:8080
    export NODE_EXTRA_CA_CERTS="${CA_CERT}"
    cargo tauri dev

  Then in Operon:
    1. Open folder: scripts/audit/canary-dataset/
    2. Switch chat mode to PLAN (not Agent)
    3. Send:  "Plan a differential-expression analysis comparing the
               AD and control groups using counts.csv and metadata.csv.
               Don't write code, just plan it."

  When the session finishes, inspect:
    cat ${AUDIT_DIR}/canaries.tsv
    ls  ${AUDIT_DIR}/flows/

  Ctrl-C this process to stop capture.
────────────────────────────────────────────────────────────────────

EOF

exec mitmweb \
  --listen-host 127.0.0.1 \
  --listen-port 8080 \
  --web-host 127.0.0.1 \
  --web-port 8081 \
  --set "audit_dir=${AUDIT_DIR}" \
  -s "${ADDON}"
