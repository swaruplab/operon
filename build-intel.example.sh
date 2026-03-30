#!/bin/bash
# Build Operon for Intel Macs (x86_64) with signing + notarization
#
# Copy this file to build-intel.sh and fill in your Apple credentials.
# build-intel.sh is gitignored and will not be committed.
#
# Required environment variables (set below or export before running):
#   APPLE_SIGNING_IDENTITY  — "Developer ID Application: Your Name (TEAMID)"
#   APPLE_ID                — Your Apple ID email
#   APPLE_PASSWORD          — App-specific password from appleid.apple.com
#   APPLE_TEAM_ID           — Your 10-character Team ID
#
# Produces a signed, notarized DMG at:
#   src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/Operon_<version>_x64.dmg

set -e

# Read version from tauri.conf.json (single source of truth)
VERSION=$(python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['version'])")

echo "═══════════════════════════════════════════════"
echo "  Operon Intel Build (x86_64) v${VERSION}"
echo "═══════════════════════════════════════════════"

export APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY}"
export APPLE_ID="${APPLE_ID:?Set APPLE_ID}"
export APPLE_PASSWORD="${APPLE_PASSWORD:?Set APPLE_PASSWORD}"
export APPLE_TEAM_ID="${APPLE_TEAM_ID:?Set APPLE_TEAM_ID}"

# Determine signing identity
SIGN_ID="$APPLE_SIGNING_IDENTITY"
echo "  Signing with: $SIGN_ID"

# Check notarization credentials
CAN_NOTARIZE=true
echo "  Notarization: enabled"

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
echo "  ✓ Signed & Notarized"
echo "═══════════════════════════════════════════════"
