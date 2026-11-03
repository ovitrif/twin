#!/bin/sh
# Bundle the release build as `iOS Mirror.app` next to the `imb` CLI in OUT
# (default build/): imb finds the app beside itself.
set -eu
out=${1:-build}
swift build -c release --product IOSMirror >/dev/null
swift build -c release --product imb >/dev/null
bin=$(swift build -c release --show-bin-path)
version=$("$bin/imb" --version | awk '{print $2}')
app="$out/iOS Mirror.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
# Made with scripts/make-icon.swift iphone
cp "assets/iOS Mirror.icns" "$app/Contents/Resources/AppIcon.icns"
cp "$bin/IOSMirror" "$app/Contents/MacOS/iOS Mirror"
(cd hid && cargo build --release --quiet)
cp hid/target/release/imb-hid "$app/Contents/MacOS/imb-hid"
cp "$bin/imb" "$out/imb"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>iOS Mirror</string>
  <key>CFBundleIdentifier</key><string>tech.masivo.imb.mirror</string>
  <key>CFBundleName</key><string>iOS Mirror</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSCameraUsageDescription</key><string>iOS Mirror reads your iPhone's screen over USB, the way QuickTime does.</string>
  <key>NSMicrophoneUsageDescription</key><string>iOS Mirror reads your iPhone's screen and sound over USB, the way QuickTime does.</string>
</dict>
</plist>
PLIST
codesign --force --sign - "$app/Contents/MacOS/imb-hid" >/dev/null 2>&1
codesign --force --sign - "$app" >/dev/null 2>&1
echo "$app"
