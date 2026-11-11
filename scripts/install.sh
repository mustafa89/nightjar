#!/usr/bin/env bash
# Builds a release and installs it as ~/Applications/Nightjar.app (Launchpad, Spotlight, Dock),
# with Rune's own installer: it signs ad hoc, swaps the bundle atomically, relaunches the app and
# waits for it to report ready (rolling back if it doesn't).
set -euo pipefail
cd "$(dirname "$0")/.."
rev=$(sed -n 's/^rune = .*rev = "\([0-9a-f]*\)".*/\1/p' Cargo.toml)
cargo build --release
[ -f assets/icon.icns ] || python3 assets/make_icon.py
installer=$(ls -d "$HOME"/.cargo/git/checkouts/rune-*/"${rev:0:7}"/scripts/mac-app | head -1)
"$installer" --name "Nightjar" --id dev.mustafa.nightjar --exe nightjar \
  --bin target/release/nightjar --icon assets/icon.icns
