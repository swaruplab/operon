#!/usr/bin/env bash
# Download anthropic-proxy-rs binaries for all supported Tauri targets and
# place them into src-tauri/binaries/ with the exact naming Tauri's sidecar
# loader expects: `anthropic-proxy-<rust-target-triple>[.exe]`.
#
# anthropic-proxy-rs is an Anthropic Messages API -> OpenAI Chat Completions
# translation proxy used by Operon to let Claude Code talk to Ollama,
# vLLM, LM Studio (pre-0.4.1), and any other OpenAI-compatible backend.
#
# Upstream publishes pre-built release binaries for darwin/linux but NOT for
# Windows, so on Windows we build from source via `cargo install --git`.
#
# Usage:
#   scripts/fetch-anthropic-proxy.sh                      # fetch all targets
#   scripts/fetch-anthropic-proxy.sh <rust-target-triple> # fetch one target

set -euo pipefail

VERSION="${ANTHROPIC_PROXY_VERSION:-1.1.1}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/src-tauri/binaries"

mkdir -p "$OUT_DIR"

fetch_tarball() {
  local target="$1"
  local url="https://github.com/m0n0x41d/anthropic-proxy-rs/releases/download/v${VERSION}/anthropic-proxy-${target}.tar.gz"
  local out_name="anthropic-proxy-${target}"
  local tmp
  tmp="$(mktemp -d)"

  echo "  Downloading $url"
  curl -fsSL "$url" -o "$tmp/bin.tgz"
  tar -xzf "$tmp/bin.tgz" -C "$tmp"
  local src
  src="$(find "$tmp" -type f -name 'anthropic-proxy' -perm -u+x | head -1)"
  if [[ -z "$src" ]]; then
    src="$(find "$tmp" -type f -name 'anthropic-proxy' | head -1)"
  fi
  if [[ -z "$src" ]]; then
    echo "  ERROR: anthropic-proxy binary not found in tarball" >&2
    rm -rf "$tmp"
    exit 1
  fi
  cp "$src" "$OUT_DIR/$out_name"
  chmod +x "$OUT_DIR/$out_name"
  rm -rf "$tmp"
  echo "  wrote  $OUT_DIR/$out_name"
}

build_windows_from_source() {
  # Upstream has no pre-built Windows binary; compile from source.
  local target="x86_64-pc-windows-msvc"
  local out_name="anthropic-proxy-${target}.exe"
  local tmp
  tmp="$(mktemp -d)"

  echo "  cargo install anthropic-proxy v${VERSION} from git (Windows)"
  cargo install \
    --git https://github.com/m0n0x41d/anthropic-proxy-rs \
    --tag "v${VERSION}" \
    --locked \
    --root "$tmp" \
    anthropic-proxy

  # cargo install places the binary at <root>/bin/<name>[.exe]
  local src="$tmp/bin/anthropic-proxy.exe"
  if [[ ! -f "$src" ]]; then
    src="$tmp/bin/anthropic-proxy"
  fi
  cp "$src" "$OUT_DIR/$out_name"
  rm -rf "$tmp"
  echo "  wrote  $OUT_DIR/$out_name"
}

fetch_target() {
  local t="$1"
  case "$t" in
    x86_64-pc-windows-msvc) build_windows_from_source ;;
    aarch64-apple-darwin | x86_64-apple-darwin | aarch64-unknown-linux-gnu | x86_64-unknown-linux-gnu)
      fetch_tarball "$t"
      ;;
    *)
      echo "  ERROR: unsupported target '$t'" >&2
      exit 1
      ;;
  esac
}

echo "Fetching anthropic-proxy v${VERSION} into ${OUT_DIR}"

if [[ $# -gt 0 ]]; then
  fetch_target "$1"
else
  # No arg: fetch every non-Windows target via pre-built tarballs.
  # Windows is skipped here — the Windows CI runner invokes this script
  # explicitly with its own target triple and compiles from source.
  fetch_tarball "aarch64-apple-darwin"
  fetch_tarball "x86_64-apple-darwin"
  fetch_tarball "aarch64-unknown-linux-gnu"
  fetch_tarball "x86_64-unknown-linux-gnu"
fi

echo
echo "Done. Vendored anthropic-proxy binaries:"
ls -lh "$OUT_DIR"/anthropic-proxy-* 2>/dev/null || true
