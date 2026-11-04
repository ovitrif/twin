#!/bin/sh
# Bundle the release build as `Twin Mirror.app` in OUT (default build/), the
# mirror windows Twin's menu bar app opens: one iPhone, or an Android phone
# beside it. The `dmb` CLI and `dmb-hid` ride inside it, and dmb opens the
# mirror beside itself. Every mirror is this one app, so macOS asks camera
# access (the capture fallback before iOS 27) once.
# It runs scrcpy's own server on the Android phone (scrcpy-server 5.0.1,
# Apache-2.0), pinned as Android Mirror pins it.
set -eu
out=${1:-build}
swift build -c release --product Twin >/dev/null
swift build -c release --product dmb >/dev/null
(cd hid && cargo build --release --quiet)
bin=$(swift build -c release --show-bin-path)
version=$("$bin/dmb" --version | awk '{print $2}')

SERVER_SHA256="764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536"
cache="${HOME}/.cache/twin"
server="${cache}/scrcpy-server-v5.0.1"
mkdir -p "$cache"
if ! echo "${SERVER_SHA256}  ${server}" | shasum -a 256 -c - >/dev/null 2>&1; then
  curl -fsSL "https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/scrcpy-server-v5.0.1" -o "$server"
  echo "${SERVER_SHA256}  ${server}" | shasum -a 256 -c - >/dev/null || { echo "scrcpy-server checksum mismatch" >&2; exit 1; }
fi
[ -f "${cache}/scrcpy-LICENSE" ] || curl -fsSL "https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/LICENSE" -o "${cache}/scrcpy-LICENSE"

app="$out/Twin Mirror.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin/Twin" "$app/Contents/MacOS/Twin"
cp "$bin/dmb" "$app/Contents/MacOS/dmb"
cp hid/target/release/dmb-hid "$app/Contents/MacOS/dmb-hid"
cp "$server" "$app/Contents/Resources/scrcpy-server"
cp "${cache}/scrcpy-LICENSE" "$app/Contents/Resources/scrcpy-LICENSE"
# Made with scripts/make-icon.swift cast
cp assets/Twin.icns "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>Twin</string>
  <key>CFBundleIdentifier</key><string>tech.masivo.twin.mirror</string>
  <key>CFBundleName</key><string>Twin</string>
  <key>CFBundleDisplayName</key><string>Twin</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSCameraUsageDescription</key><string>Twin reads your iPhone's screen over USB, the way QuickTime does.</string>
  <key>NSMicrophoneUsageDescription</key><string>Twin reads your iPhone's screen and sound over USB, the way QuickTime does.</string>
</dict>
</plist>
PLIST
codesign --force --sign - "$app/Contents/MacOS/dmb-hid" >/dev/null 2>&1
codesign --force --sign - "$app/Contents/MacOS/dmb" >/dev/null 2>&1
codesign --force --sign - "$app" >/dev/null 2>&1
echo "$app"
