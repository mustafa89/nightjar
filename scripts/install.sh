#!/usr/bin/env bash
# Builds a release and installs it as ~/Applications/Nightjar.app (Launchpad, Spotlight, Dock).
# The bundle is built here, so its Info.plist can carry the System Audio Recording usage string
# the YouTube spectrum needs; Rune's installer then swaps it in atomically, relaunches the app and
# waits for it to report ready (rolling back if it doesn't).
set -euo pipefail
cd "$(dirname "$0")/.."
rev=$(sed -n 's/^rune = .*rev = "\([0-9a-f]*\)".*/\1/p' Cargo.toml)
cargo build --release
[ -f assets/icon.icns ] || python3 assets/make_icon.py

stage=$(mktemp -d)/Nightjar.app
mkdir -p "$stage/Contents/MacOS" "$stage/Contents/Resources"
cp target/release/nightjar "$stage/Contents/MacOS/nightjar"
cp assets/icon.icns "$stage/Contents/Resources/icon.icns"
cat > "$stage/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key><string>Nightjar</string>
    <key>CFBundleExecutable</key><string>nightjar</string>
    <key>CFBundleIconFile</key><string>icon</string>
    <key>CFBundleIdentifier</key><string>dev.mustafa.nightjar</string>
    <key>CFBundleName</key><string>Nightjar</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.2.0</string>
    <key>LSMinimumSystemVersion</key><string>14.2</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSAudioCaptureUsageDescription</key><string>Nightjar draws its spectrum and visualizer from the music its YouTube Music window plays.</string>
</dict>
</plist>
PLIST
# MAC_APP_SIGN="<identity>" keeps macOS's permission answers across rebuilds; ad hoc otherwise.
codesign --force --sign "${MAC_APP_SIGN:--}" "$stage"

installer=$(ls -d "$HOME"/.cargo/git/checkouts/rune-*/"${rev:0:7}"/scripts/mac-app | head -1)
"$installer" --name "Nightjar" --id dev.mustafa.nightjar --exe nightjar --bundle "$stage"
