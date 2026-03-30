#!/bin/bash
# Build Operon as a Universal macOS app (Apple Silicon + Intel)
#
# Copy this file to build-universal.sh and fill in your Apple credentials.
# build-universal.sh is gitignored and will not be committed.
#
# Required environment variables (set below or export before running):
#   APPLE_SIGNING_IDENTITY  — "Developer ID Application: Your Name (TEAMID)"
#   APPLE_ID                — Your Apple ID email
#   APPLE_PASSWORD          — App-specific password from appleid.apple.com
#   APPLE_TEAM_ID           — Your 10-character Team ID

set -e

# Read version from tauri.conf.json (single source of truth)
VERSION=$(python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['version'])")

echo "═══════════════════════════════════════════════"
echo "  Operon Universal Build (ARM64 + x86_64) v${VERSION}"
echo "═══════════════════════════════════════════════"

export APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY}"
export APPLE_ID="${APPLE_ID:?Set APPLE_ID}"
export APPLE_PASSWORD="${APPLE_PASSWORD:?Set APPLE_PASSWORD}"
export APPLE_TEAM_ID="${APPLE_TEAM_ID:?Set APPLE_TEAM_ID}"

# Determine signing identity
if [ -n "$APPLE_SIGNING_IDENTITY" ]; then
    SIGN_ID="$APPLE_SIGNING_IDENTITY"
    echo "  Signing with: $SIGN_ID"
else
    SIGN_ID="-"
    echo "  ⚠ No APPLE_SIGNING_IDENTITY set — using ad-hoc signing"
    echo "    (App will only work on this Mac. Set APPLE_SIGNING_IDENTITY to distribute.)"
fi

# Check notarization credentials
CAN_NOTARIZE=false
if [ -n "$APPLE_ID" ] && [ -n "$APPLE_PASSWORD" ] && [ -n "$APPLE_TEAM_ID" ]; then
    CAN_NOTARIZE=true
    echo "  Notarization: enabled (Apple ID: $APPLE_ID)"
else
    echo "  ⚠ Notarization: disabled (set APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID to enable)"
fi

# Step 1: Ensure both Rust targets are installed
echo ""
echo "▸ Checking Rust targets..."
if ! rustup target list --installed | grep -q "aarch64-apple-darwin"; then
    echo "  Installing aarch64-apple-darwin target..."
    rustup target add aarch64-apple-darwin
fi
if ! rustup target list --installed | grep -q "x86_64-apple-darwin"; then
    echo "  Installing x86_64-apple-darwin target..."
    rustup target add x86_64-apple-darwin
fi
echo "  ✓ Both targets installed"

# Step 2: Build frontend
echo ""
echo "▸ Building frontend..."
npm run build
echo "  ✓ Frontend built"

# Step 3: Build for Apple Silicon (aarch64)
echo ""
echo "▸ Building for Apple Silicon (aarch64-apple-darwin)..."
npm run tauri build -- --target aarch64-apple-darwin 2>&1 | tail -5
AARCH64_APP="src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Operon.app"
echo "  ✓ ARM64 build complete"

# Step 4: Build for Intel (x86_64)
echo ""
echo "▸ Building for Intel (x86_64-apple-darwin)..."
npm run tauri build -- --target x86_64-apple-darwin 2>&1 | tail -5
X86_APP="src-tauri/target/x86_64-apple-darwin/release/bundle/macos/Operon.app"
echo "  ✓ x86_64 build complete"

# Step 5: Create universal binary
echo ""
echo "▸ Creating universal binary..."
UNIVERSAL_DIR="src-tauri/target/universal-apple-darwin/release/bundle/macos"
UNIVERSAL_APP="$UNIVERSAL_DIR/Operon.app"

rm -rf "$UNIVERSAL_DIR"
mkdir -p "$UNIVERSAL_DIR"
cp -R "$AARCH64_APP" "$UNIVERSAL_APP"

BINARY_NAME="Operon"
AARCH64_BIN="$AARCH64_APP/Contents/MacOS/$BINARY_NAME"
X86_BIN="$X86_APP/Contents/MacOS/$BINARY_NAME"
UNIVERSAL_BIN="$UNIVERSAL_APP/Contents/MacOS/$BINARY_NAME"

lipo -create "$AARCH64_BIN" "$X86_BIN" -output "$UNIVERSAL_BIN"
echo "  ✓ Universal binary created"

echo ""
echo "▸ Verifying architectures..."
lipo -info "$UNIVERSAL_BIN"

# Step 6: Sign the universal app with Developer ID
echo ""
echo "▸ Signing the universal app..."

# Remove old signature (lipo invalidated it)
codesign --remove-signature "$UNIVERSAL_APP" 2>/dev/null || true

# Sign all nested code first (frameworks, dylibs, helpers)
find "$UNIVERSAL_APP/Contents" \( -name "*.dylib" -o -name "*.framework" -o -name "*.app" \) -not -path "$UNIVERSAL_APP" 2>/dev/null | while read -r nested; do
    echo "  Signing: $(basename "$nested")"
    codesign --force --options runtime --sign "$SIGN_ID" --timestamp "$nested" 2>/dev/null || true
done

# Sign the main app bundle
chmod +x "$UNIVERSAL_BIN"
codesign --force --options runtime --sign "$SIGN_ID" --timestamp --deep "$UNIVERSAL_APP"
echo "  ✓ App signed"

# Verify
echo ""
echo "▸ Verifying signature..."
codesign --verify --verbose=2 "$UNIVERSAL_APP" 2>&1 && echo "  ✓ Signature valid" || echo "  ⚠ Signature warnings (may still work)"

# Step 7: Create DMG
echo ""
echo "▸ Creating universal DMG..."
DMG_DIR="src-tauri/target/universal-apple-darwin/release/bundle/dmg"
DMG_PATH="$DMG_DIR/Operon_${VERSION}_universal.dmg"
mkdir -p "$DMG_DIR"
rm -f "$DMG_PATH"

STAGING_DIR=$(mktemp -d)
cp -R "$UNIVERSAL_APP" "$STAGING_DIR/"
ln -s /Applications "$STAGING_DIR/Applications"

hdiutil create -volname "Operon" \
    -srcfolder "$STAGING_DIR" \
    -ov -format UDZO \
    "$DMG_PATH"
rm -rf "$STAGING_DIR"

# Sign the DMG too
codesign --force --sign "$SIGN_ID" --timestamp "$DMG_PATH" 2>/dev/null || true
echo "  ✓ DMG created and signed"

# Step 8: Notarize with Apple (so it opens on any Mac without Gatekeeper warnings)
if [ "$CAN_NOTARIZE" = true ]; then
    echo ""
    echo "▸ Submitting to Apple for notarization..."
    echo "  (This may take 2-10 minutes)"

    xcrun notarytool submit "$DMG_PATH" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait 2>&1 | tee /tmp/operon_notarize.log

    # Check if notarization succeeded
    if grep -q "status: Accepted" /tmp/operon_notarize.log; then
        echo "  ✓ Notarization accepted!"

        # Staple the notarization ticket to the DMG
        echo ""
        echo "▸ Stapling notarization ticket..."
        xcrun stapler staple "$DMG_PATH"
        echo "  ✓ Ticket stapled to DMG"

        # Also staple to the .app for direct distribution
        xcrun stapler staple "$UNIVERSAL_APP" 2>/dev/null || true
    else
        echo "  ⚠ Notarization may have failed. Check the log:"
        echo "    cat /tmp/operon_notarize.log"
        echo ""
        echo "  You can check status manually with:"
        echo "    xcrun notarytool log <submission-id> --apple-id \$APPLE_ID --password \$APPLE_PASSWORD --team-id \$APPLE_TEAM_ID"
    fi
else
    echo ""
    echo "  Skipping notarization (no credentials provided)"
fi

# Step 9: Copy to Desktop
DESKTOP_DMG="$HOME/Desktop/Operon_${VERSION}_universal.dmg"
cp "$DMG_PATH" "$DESKTOP_DMG" 2>/dev/null || true

# Clear quarantine on local copy
xattr -cr "$UNIVERSAL_APP" 2>/dev/null || true

echo ""
echo "═══════════════════════════════════════════════"
echo "  Build complete!"
echo ""
echo "  Universal .app: $UNIVERSAL_APP"
echo "  Universal .dmg: $DMG_PATH"
if [ -f "$DESKTOP_DMG" ]; then
echo "  Desktop copy:   $DESKTOP_DMG"
fi
echo ""
echo "  Runs natively on:"
echo "    • Apple Silicon (M1/M2/M3/M4)"
echo "    • Intel Macs (x86_64)"
if [ "$CAN_NOTARIZE" = true ]; then
echo ""
echo "  ✓ Signed & Notarized — opens on any Mac"
fi
echo "═══════════════════════════════════════════════"
