#!/usr/bin/env bash
# Download ripgrep binaries for all supported Tauri targets and place them
# into src-tauri/binaries/ with the exact naming Tauri's sidecar loader
# expects: `rg-<rust-target-triple>[.exe]`.
#
# We also grab the linux-musl build (not wired into externalBin) — it's the
# portable binary we scp to remote HPC servers when the user asks Operon to
# install ripgrep for them.
#
# Run once. Idempotent. Re-run to upgrade the pinned version.

set -euo pipefail

VERSION="${RG_VERSION:-14.1.1}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/src-tauri/binaries"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p "$OUT_DIR"

download_and_extract() {
  local archive_name="$1"   # e.g. ripgrep-14.1.1-aarch64-apple-darwin.tar.gz
  local target_triple="$2"  # Tauri rust target triple for sidecar naming
  local ext="$3"            # "" or ".exe"
  local url="https://github.com/BurntSushi/ripgrep/releases/download/${VERSION}/${archive_name}"
  local out_name="rg-${target_triple}${ext}"

  echo "  Downloading $archive_name"
  curl -fsSL "$url" -o "$TMP_DIR/$archive_name"

  case "$archive_name" in
    *.tar.gz)
      tar -xzf "$TMP_DIR/$archive_name" -C "$TMP_DIR"
      local extracted_dir="${archive_name%.tar.gz}"
      cp "$TMP_DIR/$extracted_dir/rg" "$OUT_DIR/$out_name"
      ;;
    *.zip)
      unzip -q "$TMP_DIR/$archive_name" -d "$TMP_DIR"
      local extracted_dir="${archive_name%.zip}"
      cp "$TMP_DIR/$extracted_dir/rg.exe" "$OUT_DIR/$out_name"
      ;;
  esac

  chmod +x "$OUT_DIR/$out_name" || true
  echo "  wrote  $OUT_DIR/$out_name"
}

echo "Fetching ripgrep ${VERSION} into ${OUT_DIR}"

download_and_extract "ripgrep-${VERSION}-aarch64-apple-darwin.tar.gz"      "aarch64-apple-darwin"       ""
download_and_extract "ripgrep-${VERSION}-x86_64-apple-darwin.tar.gz"       "x86_64-apple-darwin"        ""
download_and_extract "ripgrep-${VERSION}-x86_64-pc-windows-msvc.zip"       "x86_64-pc-windows-msvc"     ".exe"
download_and_extract "ripgrep-${VERSION}-x86_64-unknown-linux-musl.tar.gz" "x86_64-unknown-linux-gnu"   ""

# Keep a copy of the musl build under its own name for remote-install deployment
cp "$OUT_DIR/rg-x86_64-unknown-linux-gnu" "$OUT_DIR/rg-x86_64-unknown-linux-musl"

echo
echo "Done. Vendored binaries:"
ls -lh "$OUT_DIR"/rg-*
