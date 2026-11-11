#!/bin/bash
# Builds "Nightjar YouTube.app" with swiftc (no SwiftPM or Xcode) and installs it in
# ~/Applications. `--no-install` leaves it in build/.
set -euo pipefail
cd "$(dirname "$0")"

APP="build/Nightjar YouTube.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
swiftc -O -swift-version 5 -target arm64-apple-macosx14.2 \
    Sources/*.swift -framework Accelerate -framework CoreAudio -framework AudioToolbox -o "$APP/Contents/MacOS/NightjarYouTube"
cp inject.js "$APP/Contents/Resources/"
[ -f icon.icns ] || python3 -I make_icon.py
cp icon.icns "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Nightjar YouTube</string>
    <key>CFBundleDisplayName</key><string>Nightjar YouTube</string>
    <key>CFBundleIdentifier</key><string>dev.mustafa.nightjar.youtube</string>
    <key>CFBundleExecutable</key><string>NightjarYouTube</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>LSMinimumSystemVersion</key><string>14.2</string>
    <key>NSAudioCaptureUsageDescription</key><string>Nightjar draws its spectrum and visualizer from the music this window plays.</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP"
echo "Built $APP"
if [ "${1:-}" != "--no-install" ]; then
    rm -rf "$HOME/Applications/Nightjar YouTube.app"
    cp -R "$APP" "$HOME/Applications/"
    echo "Installed ~/Applications/Nightjar YouTube.app"
fi
