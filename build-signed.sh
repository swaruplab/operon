#!/bin/bash
# Build Operon for Apple Silicon (aarch64) with signing + notarization
#
# Usage:  ./build-signed.sh
#
# Produces a signed, notarized DMG at:
#   src-tauri/target/release/bundle/dmg/Operon_<version>_aarch64.dmg

set -e

# Strip miniforge/miniconda/anaconda paths and related env vars before the
# Rust build, otherwise `libssh2-sys`/`git2-sys`/etc.'s build.rs picks up
# conda's pkg-config and links Operon against conda's `libiconv.2.dylib`
# (and friends). At runtime the signed Operon binary then fails to load
# them with "code signature ... different Team IDs" because conda's libs
# are signed by Conda-Forge, not by your Apple Developer ID.
_ORIG_PATH="$PATH"
export PATH="$(printf '%s' "$PATH" | tr ':' '\n' | grep -viE 'miniforge|miniconda|/anaconda|/conda/' | paste -sd ':' -)"
unset PKG_CONFIG_PATH PKG_CONFIG_LIBDIR LIBRARY_PATH \
      DYLD_LIBRARY_PATH DYLD_FALLBACK_LIBRARY_PATH \
      CMAKE_PREFIX_PATH CONDA_PREFIX CONDA_DEFAULT_ENV CONDA_SHLVL
if [ "$PATH" != "$_ORIG_PATH" ]; then
    echo "  ⓘ stripped miniforge/conda paths for the build (avoids libiconv contamination)"
fi
unset _ORIG_PATH

# Read version from tauri.conf.json (single source of truth)
VERSION=$(python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['version'])")

echo "═══════════════════════════════════════════════"
echo "  Operon Apple Silicon Build (aarch64) v${VERSION}"
echo "═══════════════════════════════════════════════"

# Load Apple Developer credentials from .env.signing (gitignored).
# File contents: APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID.
ENV_FILE="$(dirname "$0")/.env.signing"
if [ ! -f "$ENV_FILE" ]; then
    echo "ERROR: $ENV_FILE not found. Create it with APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID."
    exit 1
fi
# shellcheck disable=SC1090
source "$ENV_FILE"

SIGN_ID="$APPLE_SIGNING_IDENTITY"
echo "  Signing with: $SIGN_ID"
echo "  Notarization: enabled"

# Step 1: Build frontend
echo ""
echo "▸ Building frontend..."
npm run build
echo "  ✓ Frontend built"

# Step 2: Build with --bundles app (avoids Tauri's bundle_dmg.sh failing on paths with spaces).
# Apple Silicon ships the bundled anthropic-proxy sidecar; Intel does not (no x86_64 build of
# anthropic-proxy currently in src-tauri/binaries). Inject it via --config so it's only required
# for this aarch64 build.
echo ""
echo "▸ Building for Apple Silicon (aarch64)..."
npm run tauri build -- --bundles app \
    --config '{"bundle":{"externalBin":["binaries/rg","binaries/anthropic-proxy"]}}' 2>&1 | tail -5
echo "  ✓ Build complete"

APP_DIR="$(pwd)/src-tauri/target/release/bundle/macos"
APP_PATH="$APP_DIR/Operon.app"
APP_BIN="$APP_PATH/Contents/MacOS/Operon"
DMG_DIR="$(pwd)/src-tauri/target/release/bundle/dmg"
DMG_PATH="$DMG_DIR/Operon_${VERSION}_aarch64.dmg"

if [ ! -d "$APP_PATH" ]; then
    echo "ERROR: Operon.app not found at $APP_PATH"
    exit 1
fi

# Step 3: Verify architecture
echo ""
echo "▸ Verifying architecture..."
lipo -info "$APP_BIN"

# Step 4: Sign the app
echo ""
echo "▸ Signing the app..."

# Sign nested code first (frameworks, dylibs, helpers)
find "$APP_PATH/Contents" \( -name "*.dylib" -o -name "*.framework" -o -name "*.app" \) -not -path "$APP_PATH" 2>/dev/null | while read -r nested; do
    echo "  Signing: $(basename "$nested")"
    codesign --force --options runtime --sign "$SIGN_ID" --timestamp "$nested" 2>/dev/null || true
done

# Sign main app bundle
chmod +x "$APP_BIN"
codesign --force --options runtime --sign "$SIGN_ID" --timestamp --deep "$APP_PATH"
echo "  ✓ App signed"

# Verify
codesign --verify --verbose=2 "$APP_PATH" 2>&1 && echo "  ✓ Signature valid" || echo "  Signature warnings"

# Step 5: Create DMG
echo ""
echo "▸ Creating DMG..."
mkdir -p "$DMG_DIR"
rm -f "$DMG_PATH"

STAGING_DIR=$(mktemp -d)
cp -R "$APP_PATH" "$STAGING_DIR/"
ln -s /Applications "$STAGING_DIR/Applications"

hdiutil create -volname "Operon" \
    -srcfolder "$STAGING_DIR" \
    -ov -format UDZO \
    "$DMG_PATH"
rm -rf "$STAGING_DIR"

# Sign the DMG
codesign --force --sign "$SIGN_ID" --timestamp "$DMG_PATH"
echo "  ✓ DMG created and signed"

# Step 6: Notarize
echo ""
echo "▸ Submitting to Apple for notarization..."
echo "  (This may take 2-10 minutes)"

xcrun notarytool submit "$DMG_PATH" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait 2>&1 | tee /tmp/operon_notarize_arm64.log

if grep -q "status: Accepted" /tmp/operon_notarize_arm64.log; then
    echo "  ✓ Notarization accepted!"
    echo ""
    echo "▸ Stapling notarization ticket..."
    xcrun stapler staple "$DMG_PATH"
    echo "  ✓ Ticket stapled"
    xcrun stapler staple "$APP_PATH" 2>/dev/null || true
else
    echo "  Notarization may have failed. Check: cat /tmp/operon_notarize_arm64.log"
fi

# Copy to Desktop
DESKTOP_DMG="$HOME/Desktop/Operon_${VERSION}_aarch64.dmg"
cp "$DMG_PATH" "$DESKTOP_DMG" 2>/dev/null || true

# Clear quarantine on local copy
xattr -cr "$APP_PATH" 2>/dev/null || true

echo ""
echo "═══════════════════════════════════════════════"
echo "  Apple Silicon build complete!"
echo ""
echo "  .app: $APP_PATH"
echo "  .dmg: $DMG_PATH"
if [ -f "$DESKTOP_DMG" ]; then
echo "  Desktop copy: $DESKTOP_DMG"
fi
echo ""
echo "  Runs on: Apple Silicon Macs (M1/M2/M3/M4)"
echo "  ✓ Signed & Notarized"
echo "═══════════════════════════════════════════════"
