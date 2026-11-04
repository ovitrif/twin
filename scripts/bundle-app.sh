#!/bin/sh
# Wraps the menu bar app binary into Twin.app (menu bar agent, no Dock icon),
# with its mirrors under Contents/Helpers: Twin Mirror.app (the iPhone and
# side by side windows, dmb, dmb-hid; built from apple/) and Android Mirror.app
# (the patched scrcpy from scripts/build-scrcpy.sh, when built in target/scrcpy
# or $SCRCPY_DIR; otherwise the app uses scrcpy from PATH).
# Usage: scripts/bundle-app.sh [app-binary] [output-dir]
# TWIN_SKIP_MIRROR=1 leaves Twin Mirror.app out, for a cross-compiled build.
set -eu

APP_BIN="${1:-target/release/awb-app}"
OUT_DIR="${2:-target/bundle}"
SCRCPY_DIR="${SCRCPY_DIR:-target/scrcpy}"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"

if [ ! -x "$APP_BIN" ]; then
  echo "app binary not found at ${APP_BIN}; build it with: cargo build --release -p awb-app" >&2
  exit 1
fi


APP="${OUT_DIR}/Twin.app"
# Bundles of earlier releases, so a rebuild leaves one app for Spotlight
rm -rf "$APP" "${OUT_DIR}/Android Wifi Bridge.app" "${OUT_DIR}/AWB.app"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources"

cp "$APP_BIN" "${APP}/Contents/MacOS/awb-app"

# Liquid Glass icon: actool compiles the Icon Composer bundle into Assets.car
# (light, dark, clear and tinted variants) plus AppIcon.icns for older macOS.
# Without an actool that understands .icon files, fall back to the flat icon.
# Made with apple/scripts/make-icon.swift cast
cp "$(dirname "$0")/../apple/assets/Twin.icns" "${APP}/Contents/Resources/AppIcon.icns"

# The mirror windows, the dmb CLI and dmb-hid (macOS only, built with Swift)
if [ "${TWIN_SKIP_MIRROR:-}" != 1 ]; then
  mkdir -p "${APP}/Contents/Helpers"
  helpers="$(cd "${APP}/Contents/Helpers" && pwd)"
  (cd "$(dirname "$0")/../apple" && scripts/bundle-app.sh "$helpers") >/dev/null
fi

if [ -x "${SCRCPY_DIR}/scrcpy" ] \
  && [ "$(lipo -archs "${SCRCPY_DIR}/scrcpy")" != "$(lipo -archs "$APP_BIN")" ]; then
  echo "skipping ${SCRCPY_DIR}/scrcpy: not built for the app's architecture" >&2
elif [ -x "${SCRCPY_DIR}/scrcpy" ]; then
  MIRROR="${APP}/Contents/Helpers/Android Mirror.app"
  mkdir -p "${MIRROR}/Contents/MacOS" "${MIRROR}/Contents/Resources"
  cp "${SCRCPY_DIR}/scrcpy" "${MIRROR}/Contents/MacOS/scrcpy"
  cp "${SCRCPY_DIR}/scrcpy-server" "${SCRCPY_DIR}/scrcpy.png" \
    "${SCRCPY_DIR}/disconnected.png" "${SCRCPY_DIR}/LICENSE" \
    "${MIRROR}/Contents/Resources/"
  # Made with dmb's scripts/make-icon.swift android. scrcpy sets its Dock icon
  # from scrcpy.png at runtime, so that is this icon too
  cp "$(dirname "$0")/../scrcpy/AndroidMirror.icns" "${MIRROR}/Contents/Resources/AppIcon.icns"
  sips -s format png "${MIRROR}/Contents/Resources/AppIcon.icns" \
    --out "${MIRROR}/Contents/Resources/scrcpy.png" >/dev/null

  # awb starts mirrors through this launcher; opened with no arguments (Finder,
  # Raycast, Spotlight) it quits, since only awb knows which phone to mirror.
  # The patched scrcpy links Homebrew's ffmpeg, sdl3 and libusb; when it cannot
  # load them, the launcher falls back to the scrcpy on PATH.
  cat > "${MIRROR}/Contents/MacOS/Android Mirror" <<'LAUNCHER'
#!/bin/sh
[ "$#" -eq 0 ] && exit 0
dir="$(cd "$(dirname "$0")" && pwd)"
if ! "${dir}/scrcpy" --version >/dev/null 2>&1; then
  if ! command -v scrcpy >/dev/null 2>&1; then
    echo "Android Mirror: the bundled scrcpy cannot start and no scrcpy is on PATH" >&2
    exit 127
  fi
  echo "Android Mirror: the bundled scrcpy cannot start; using scrcpy from PATH" >&2
  exec scrcpy "$@"
fi
resources="$(cd "${dir}/../Resources" && pwd)"
export SCRCPY_SERVER_PATH="${resources}/scrcpy-server"
export SCRCPY_ICON_DIR="$resources"
exec "${dir}/scrcpy" "$@"
LAUNCHER
  chmod 755 "${MIRROR}/Contents/MacOS/Android Mirror"

  cat > "${MIRROR}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>Android Mirror</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleIdentifier</key><string>tech.masivo.twin.android</string>
  <key>CFBundleName</key><string>Android Mirror</string>
  <key>CFBundleDisplayName</key><string>Android Mirror</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
  codesign --force --sign - "${MIRROR}/Contents/MacOS/scrcpy" 2>/dev/null || true
  codesign --force --sign - "$MIRROR" 2>/dev/null || true
fi

cat > "${APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>awb-app</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleIdentifier</key><string>tech.masivo.twin</string>
  <key>CFBundleName</key><string>Twin</string>
  <key>CFBundleDisplayName</key><string>Twin</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP" 2>/dev/null || true

echo "Bundled ${APP} (v${VERSION})"
