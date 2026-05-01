"""mitmproxy addon — scan Anthropic request bodies for canary tokens.

Usage (invoked by run-audit.sh):

    mitmweb --listen-port 8080 \
            --set audit_dir=./tmp/audit-out \
            -s scripts/audit/mitm-addon.py

For each request to api.anthropic.com, the full JSON body is dumped to
<audit_dir>/flows/<n>-request.json, and every match of the regex
CANARY_[A-Z0-9_]+ anywhere in the JSON tree is appended to
<audit_dir>/canaries.tsv with the JSON path where it was found.

We deliberately do NOT try to be clever about which fields matter. Everything
is walked. The analyst reads canaries.tsv afterwards and decides whether a
hit is expected (e.g. user explicitly asked Claude to read the file) or a
leak (e.g. sibling file whose contents were never requested).
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

from mitmproxy import ctx, http

CANARY_RE = re.compile(r"CANARY_[A-Z0-9_]+")
TARGET_HOST = "api.anthropic.com"


def _walk(node, path, hits):
    """Recursively walk a parsed JSON value, emitting (path, token) for
    every CANARY_* substring found in any string leaf."""
    if isinstance(node, dict):
        for k, v in node.items():
            _walk(v, f"{path}.{k}", hits)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            _walk(v, f"{path}[{i}]", hits)
    elif isinstance(node, str):
        for m in CANARY_RE.findall(node):
            hits.append((path, m))


class OperonAudit:
    def __init__(self) -> None:
        self.audit_dir: Path = Path("./tmp/audit-out")
        self.flows_dir: Path = self.audit_dir / "flows"
        self.tsv_path: Path = self.audit_dir / "canaries.tsv"
        self.counter: int = 0
        self.total_hits: int = 0
        self.distinct_tokens: set[str] = set()

    # ── mitmproxy lifecycle ────────────────────────────────────────────────
    def load(self, loader) -> None:
        loader.add_option(
            name="audit_dir",
            typespec=str,
            default="./tmp/audit-out",
            help="Directory to write captured flows and canary hits into.",
        )

    def configure(self, updates) -> None:
        if "audit_dir" in updates:
            self.audit_dir = Path(ctx.options.audit_dir).resolve()
            self.flows_dir = self.audit_dir / "flows"
            self.tsv_path = self.audit_dir / "canaries.tsv"
            self.flows_dir.mkdir(parents=True, exist_ok=True)
            # Fresh TSV header per mitmproxy run.
            with self.tsv_path.open("w", encoding="utf-8") as f:
                f.write("request_num\ttoken\tjson_path\n")
            ctx.log.info(f"[operon-audit] writing to {self.audit_dir}")

    def request(self, flow: http.HTTPFlow) -> None:
        if flow.request.pretty_host != TARGET_HOST:
            return
        self.counter += 1
        n = self.counter

        body_bytes = flow.request.raw_content or b""
        try:
            payload = json.loads(body_bytes.decode("utf-8"))
        except Exception:
            payload = None

        # Always dump the raw body so a human can eyeball it.
        out_json = self.flows_dir / f"{n}-request.json"
        meta = {
            "method": flow.request.method,
            "url": flow.request.pretty_url,
            "headers": dict(flow.request.headers),
            "body": payload if payload is not None else body_bytes.decode("utf-8", "replace"),
        }
        with out_json.open("w", encoding="utf-8") as f:
            json.dump(meta, f, indent=2, ensure_ascii=False, default=str)

        # Scan for canaries only if we got parseable JSON.
        if payload is None:
            ctx.log.warn(f"[operon-audit] #{n} non-JSON body (len={len(body_bytes)})")
            return

        hits: list[tuple[str, str]] = []
        _walk(payload, "$", hits)

        if not hits:
            ctx.log.info(f"[operon-audit] #{n} no canaries")
            return

        with self.tsv_path.open("a", encoding="utf-8") as f:
            for path, token in hits:
                f.write(f"{n}\t{token}\t{path}\n")
                self.distinct_tokens.add(token)
        self.total_hits += len(hits)
        ctx.log.alert(
            f"[operon-audit] #{n} {len(hits)} canary hit(s) — see {self.tsv_path}"
        )

    def done(self) -> None:
        summary = (
            f"[operon-audit] done. requests={self.counter} "
            f"total_canary_hits={self.total_hits} "
            f"distinct_tokens={len(self.distinct_tokens)}"
        )
        ctx.log.alert(summary)
        try:
            with (self.audit_dir / "summary.txt").open("w", encoding="utf-8") as f:
                f.write(summary + "\n")
                if self.distinct_tokens:
                    f.write("\nDistinct canary tokens seen:\n")
                    for t in sorted(self.distinct_tokens):
                        f.write(f"  {t}\n")
        except Exception:
            pass


addons = [OperonAudit()]
