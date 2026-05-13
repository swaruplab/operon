#!/bin/bash
# Build Operon for Intel Macs (x86_64) with signing + notarization
#
# Usage:  npm run build:intel:dmg
#
# Produces a signed, notarized DMG at:
#   src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/Operon_<version>_x64.dmg

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
echo "  Operon Intel Build (x86_64) v${VERSION}"
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

# Determine signing identity
if [ -n "$APPLE_SIGNING_IDENTITY" ]; then
    SIGN_ID="$APPLE_SIGNING_IDENTITY"
    echo "  Signing with: $SIGN_ID"
else
    SIGN_ID="-"
    echo "  No APPLE_SIGNING_IDENTITY set - using ad-hoc signing"
fi

# Check notarization credentials
CAN_NOTARIZE=false
if [ -n "$APPLE_ID" ] && [ -n "$APPLE_PASSWORD" ] && [ -n "$APPLE_TEAM_ID" ]; then
    CAN_NOTARIZE=true
    echo "  Notarization: enabled"
else
    echo "  Notarization: disabled"
fi

# Step 1: Ensure x86_64 target is installed
echo ""
echo "▸ Checking Rust target..."
if ! rustup target list --installed | grep -q "x86_64-apple-darwin"; then
    echo "  Installing x86_64-apple-darwin target..."
    rustup target add x86_64-apple-darwin
fi
echo "  ✓ x86_64 target ready"

# Step 2: Build frontend
echo ""
echo "▸ Building frontend..."
npm run build
echo "  ✓ Frontend built"

# Step 3: Build for Intel
echo ""
echo "▸ Building for Intel (x86_64-apple-darwin)..."
echo "  This cross-compiles on Apple Silicon - may take a few minutes."
npm run tauri build -- --target x86_64-apple-darwin 2>&1 | tail -5
echo "  ✓ Intel build complete"

X86_APP="src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Operon.app"
X86_BIN="$X86_APP/Contents/MacOS/Operon"

# Verify it's actually x86_64
echo ""
echo "▸ Verifying architecture..."
lipo -info "$X86_BIN"

# Step 4: Sign the app
echo ""
echo "▸ Signing the app..."

# Sign nested code first
find "$X86_APP/Contents" \( -name "*.dylib" -o -name "*.framework" -o -name "*.app" \) -not -path "$X86_APP" 2>/dev/null | while read -r nested; do
    echo "  Signing: $(basename "$nested")"
    codesign --force --options runtime --sign "$SIGN_ID" --timestamp "$nested" 2>/dev/null || true
done

# Sign main app
chmod +x "$X86_BIN"
codesign --force --options runtime --sign "$SIGN_ID" --timestamp --deep "$X86_APP"
echo "  ✓ App signed"

# Verify
codesign --verify --verbose=2 "$X86_APP" 2>&1 && echo "  ✓ Signature valid" || echo "  Signature warnings"

# Step 5: Create DMG
echo ""
echo "▸ Creating Intel DMG..."
DMG_DIR="src-tauri/target/x86_64-apple-darwin/release/bundle/dmg"
DMG_PATH="$DMG_DIR/Operon_${VERSION}_x64.dmg"
mkdir -p "$DMG_DIR"
rm -f "$DMG_PATH"

STAGING_DIR=$(mktemp -d)
cp -R "$X86_APP" "$STAGING_DIR/"
ln -s /Applications "$STAGING_DIR/Applications"

hdiutil create -volname "Operon" \
    -srcfolder "$STAGING_DIR" \
    -ov -format UDZO \
    "$DMG_PATH"
rm -rf "$STAGING_DIR"

# Sign DMG
codesign --force --sign "$SIGN_ID" --timestamp "$DMG_PATH" 2>/dev/null || true
echo "  ✓ DMG created and signed"

# Step 6: Notarize
if [ "$CAN_NOTARIZE" = true ]; then
    echo ""
    echo "▸ Submitting to Apple for notarization..."
    echo "  (This may take 2-10 minutes)"

    xcrun notarytool submit "$DMG_PATH" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait 2>&1 | tee /tmp/operon_notarize_intel.log

    if grep -q "status: Accepted" /tmp/operon_notarize_intel.log; then
        echo "  ✓ Notarization accepted!"
        echo ""
        echo "▸ Stapling notarization ticket..."
        xcrun stapler staple "$DMG_PATH"
        echo "  ✓ Ticket stapled"
        xcrun stapler staple "$X86_APP" 2>/dev/null || true
    else
        echo "  Notarization may have failed. Check: cat /tmp/operon_notarize_intel.log"
    fi
fi

# Copy to Desktop
DESKTOP_DMG="$HOME/Desktop/Operon_${VERSION}_x64.dmg"
cp "$DMG_PATH" "$DESKTOP_DMG" 2>/dev/null || true

# Clear quarantine
xattr -cr "$X86_APP" 2>/dev/null || true

echo ""
echo "═══════════════════════════════════════════════"
echo "  Intel build complete!"
echo ""
echo "  .app: $X86_APP"
echo "  .dmg: $DMG_PATH"
if [ -f "$DESKTOP_DMG" ]; then
echo "  Desktop copy: $DESKTOP_DMG"
fi
echo ""
echo "  Runs on: Intel Macs (x86_64)"
if [ "$CAN_NOTARIZE" = true ]; then
echo "  ✓ Signed & Notarized"
fi
echo "═══════════════════════════════════════════════"
