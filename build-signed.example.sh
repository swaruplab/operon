#!/bin/bash
# Signed + notarized build for Apple Silicon (aarch64)
#
# Copy this file to build-signed.sh and fill in your Apple credentials.
# build-signed.sh is gitignored and will not be committed.
#
# Required environment variables (set below or export before running):
#   APPLE_SIGNING_IDENTITY  — "Developer ID Application: Your Name (TEAMID)"
#   APPLE_ID                — Your Apple ID email
#   APPLE_PASSWORD          — App-specific password from appleid.apple.com
#   APPLE_TEAM_ID           — Your 10-character Team ID

set -e

export APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY}"
export APPLE_ID="${APPLE_ID:?Set APPLE_ID}"
export APPLE_PASSWORD="${APPLE_PASSWORD:?Set APPLE_PASSWORD}"
export APPLE_TEAM_ID="${APPLE_TEAM_ID:?Set APPLE_TEAM_ID}"

# Build with --no-bundle for DMG to avoid Tauri's bundle_dmg.sh failing on paths with spaces.
# Tauri still builds the .app and signs/notarizes it; we just create the DMG ourselves.
npm run tauri build -- --bundles app 2>&1 || true

APP_DIR="$(pwd)/src-tauri/target/release/bundle/macos"
APP_PATH="$APP_DIR/Operon.app"
DMG_DIR="$(pwd)/src-tauri/target/release/bundle/dmg"
VERSION=$(python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
DMG_PATH="$DMG_DIR/Operon_${VERSION}_aarch64.dmg"

if [ ! -d "$APP_PATH" ]; then
  echo "ERROR: Operon.app not found at $APP_PATH"
  exit 1
fi

echo "Creating DMG..."
mkdir -p "$DMG_DIR"

# Remove old DMG if it exists
rm -f "$DMG_PATH"

# Create a temporary directory for DMG contents
TEMP_DMG=$(mktemp -d)
cp -R "$APP_PATH" "$TEMP_DMG/"
ln -s /Applications "$TEMP_DMG/Applications"

# Create DMG
hdiutil create -volname "Operon" \
  -srcfolder "$TEMP_DMG" \
  -ov -format UDZO \
  "$DMG_PATH"

rm -rf "$TEMP_DMG"

# Sign the DMG
codesign --force --sign "$APPLE_SIGNING_IDENTITY" "$DMG_PATH"

# Notarize the DMG
echo "Notarizing DMG..."
xcrun notarytool submit "$DMG_PATH" \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait

# Staple
xcrun stapler staple "$DMG_PATH"

echo ""
echo "Done! DMG at: $DMG_PATH"
