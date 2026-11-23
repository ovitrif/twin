#!/bin/sh
# Wraps the menu bar app binary into Twin.app (menu bar agent, no Dock icon),
# with its helpers under Contents/Helpers: Twin Mirror.app (every mirror
# window, dmb, dmb-hid; built from apple/) and Google's adb (platform-tools).
# Usage: scripts/bundle-app.sh [app-binary] [output-dir]
# TWIN_SKIP_MIRROR=1 leaves Twin Mirror.app out (a quick tray-only build).
set -eu

APP_BIN="${1:-target/release/twin-app}"
OUT_DIR="${2:-target/bundle}"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"

if [ ! -x "$APP_BIN" ]; then
  echo "app binary not found at ${APP_BIN}; build it with: cargo build --release -p twin-app" >&2
  exit 1
fi


APP="${OUT_DIR}/Twin.app"
# Bundles of earlier releases, so a rebuild leaves one app for Spotlight
rm -rf "$APP" "${OUT_DIR}/Android Wifi Bridge.app" "${OUT_DIR}/AWB.app"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"

cp "$APP_BIN" "${APP}/Contents/MacOS/twin-app"

# The app icon, made with apple/scripts/make-icon.swift cast. The Icon
# Composer source, crates/twin-app/assets/AppIcon.icon, is not compiled in:
# actool needs Xcode, and release builds must not
cp "$(dirname "$0")/../apple/assets/Twin.icns" "${APP}/Contents/Resources/AppIcon.icns"

# The mirror windows, the dmb CLI and dmb-hid (macOS only, built with Swift)
if [ "${TWIN_SKIP_MIRROR:-}" != 1 ]; then
  mkdir -p "${APP}/Contents/Helpers"
  helpers="$(cd "${APP}/Contents/Helpers" && pwd)"
  (cd "$(dirname "$0")/../apple" && scripts/bundle-app.sh "$helpers") >/dev/null
fi

# Google's adb (platform-tools, pinned), for phones on a Mac without the
# Android SDK; an adb on PATH or in the SDK is used first
PT_VERSION="r37.0.1"
PT_SHA256="ee39ad5967e95c2a07f04dbcbde96b1a0c916ba376096db5d2f498b7727a5d1d"
pt_cache="${HOME}/.cache/twin"
pt_zip="${pt_cache}/platform-tools-${PT_VERSION}-darwin.zip"
mkdir -p "$pt_cache"
if ! echo "${PT_SHA256}  ${pt_zip}" | shasum -a 256 -c - >/dev/null 2>&1; then
  # Download beside the cache and move it in only once it checks out, so an
  # interrupted download never sits in the cache
  pt_part="${pt_zip}.$$.part"
  curl -fsSL "https://dl.google.com/android/repository/platform-tools_${PT_VERSION}-darwin.zip" -o "$pt_part"
  echo "${PT_SHA256}  ${pt_part}" | shasum -a 256 -c - >/dev/null || {
    rm -f "$pt_part"; echo "platform-tools checksum mismatch" >&2; exit 1; }
  mv -f "$pt_part" "$pt_zip"
fi
mkdir -p "${APP}/Contents/Helpers"
unzip -q -o "$pt_zip" platform-tools/adb platform-tools/NOTICE.txt platform-tools/source.properties \
  -d "${APP}/Contents/Helpers"

cat > "${APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>twin-app</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleIdentifier</key><string>tech.masivo.twin</string>
  <key>CFBundleName</key><string>Twin</string>
  <key>CFBundleDisplayName</key><string>Twin</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP" 2>/dev/null || true

echo "Bundled ${APP} (v${VERSION})"
