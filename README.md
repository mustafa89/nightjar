# Nightjar

A skinnable music player with live, music-driven visualizers, built with
[Rune](https://github.com/anomalyco/rune) (GPU-rendered native UI in Rust).

```sh
cargo run --release -- ~/Music/some-album     # files or folders; the first plays
scripts/install.sh                             # ~/Applications/Nightjar.app
```

## Window

- **Deck**: a display with play state, a clock (click it for remaining time), title,
  artist · album, rate / channels / codec tags, and a 20-band spectrum with falling peak caps.
  Below it are the seek bar (drag to scrub), the transport buttons, shuffle, repeat
  (off → playlist → track) and volume. Shuffle fixes a shuffled order when it's turned on, so
  up next is always what plays. Drag any bare surface (the gaps, the card faces or the
  visualizer) to move the window.
- **Playlist**: drop files or folders anywhere in the window, or use `+` (file) and the folder
  button. Click a row to select it; double-click or `↵` plays it. `⌫` removes the selected row,
  and the bin clears the list. The playlist, skin, mode, volume, shuffle and repeat are kept in
  `~/.config/nightjar/settings.json` (`NIGHTJAR_CONFIG` overrides the path).
- **Stage**: a full-height visualizer with four modes: Aurora Tunnel, Spectrum Halo, Scope
  Ribbon and Plasma Bloom. Bass, mids, treble, beats, a 64-bin spectrum and the waveform drive
  it. Its clock runs faster when the music is louder, and it stops when playback stops.
- **Skins**: Midnight Chrome, Ember Vinyl, Phosphor and Candy Pop. Each skin sets the colours of
  every surface and the visualizer palette, and switching skins fades every colour on a spring.

- **Modern layout** (the layout button in the header, or `M`): large album art with the live
  spectrum along its bottom edge, the title, a seek bar with times, and the transport; beside
  it, **Now playing** and **Up next** from the playlist (repeat-aware; with shuffle on it says
  the shuffled order with shuffle on). Behind it all, the cover is blurred across the window
  (`NIGHTJAR_BACKDROP=0` turns that off). On YouTube, the right side shows YouTube Music's own
  queue. Art comes from the cover embedded in the file's tags, else a
  `cover`/`folder`/`front` image in its folder, or YouTube's artwork. The layout has no shader
  stage, so it's the lighter of the two.

Formats: mp3, flac, wav, m4a/aac, ogg/vorbis.

## YouTube Music

`ytmusic/` is a small Swift helper app, **Nightjar YouTube**: a background app with no Dock
icon that quits when Nightjar quits. It plays music.youtube.com in a
WebKit window with YouTube's own player, so you sign in, search and pick music there. Nightjar
switches to it with the **Library | YouTube** tabs (or `Y`):

- the deck shows its title, artist, time and seek bar, and the transport, seek and volume
  control it;
- the spectrum and visualizer come from a Core Audio process tap on the helper's own WebKit
  processes (macOS 14.2+). The first capture asks for **System Audio Recording** permission.
  WebKit doesn't feed YouTube's streams into Web Audio, so a page-level analyser only reads
  silence;
- the notch shows and controls it like local playback.

```sh
ytmusic/build.sh          # builds and installs ~/Applications/Nightjar YouTube.app
```

The helper talks to Nightjar over UDP on `127.0.0.1`: status JSON to port 47813, commands
(`toggle`, `next`, `prev`, `show`, `seek <s>`, `volume <0..1>`) to port 47814. It is ad-hoc
signed, so macOS asks for audio permission again after each rebuild. Test aids (all off by
default): `NJYT_SILENT=1` (no autoplay, no audio tap, a throwaway web session),
`NJYT_HIDDEN=1` (no window), `NJYT_STATUS_PORT` / `NJYT_COMMAND_PORT` (other ports).

## Notch link

Nightjar talks to NotchSwitcher (`~/code/notch-switcher`) over UDP on `127.0.0.1` only:

- It sends a JSON status to port 47811: state, title, artist, position, length, the 20
  spectrum bands and the skin's accents. It sends 30 times a second while playing and once a
  second otherwise.
- It accepts `toggle`, `next` or `prev` on port 47812.

The notch shows a Nightjar button with a now-playing panel, and a Live Activity while music
plays. Without the notch app, the packets are dropped.

## How it works

| File | Role |
|---|---|
| `src/audio.rs` | rodio playback. A `Tap` source mixes every decoded frame to mono and copies it into a lock-free ring (`Scope`). The audio device opens on the first play. |
| `src/analysis.rs` | A 2048-point Hann-windowed FFT (rustfft) on the main thread, about 60 times a second: deck bands, peaks, 64 bins, a 64-point waveform, bass/mid/treble and a beat pulse. |
| `src/state.rs` | Playlist and transport actions. `poll` reads tags, moves to the next track at the end, and publishes levels into `rune::Live` values, so the bars, seek bar and clock move without re-running the view. |
| `src/viz.rs` | A Rune `Compositor`. It draws the UI with `Renderer::draw_with` and paints one WGSL shader into the stage's `El::effect` slot. A uniform buffer feeds the shader the levels and the skin palette. |
| `src/bridge.rs` | The notch link: status out, commands in. |
| `src/youtube.rs` | The YouTube Music source: helper status in, commands out. |
| `src/art.rs` | Album art: embedded covers, cover files and YouTube artwork, fetched on a worker and decoded by Rune's image cache. |
| `src/library.rs` | Folder scans, plus tags and durations read by symphonia on a background thread. |
| `src/skin.rs`, `src/view.rs`, `src/icons.rs` | Skins, layout and icons. |

## Keys

| Key | Action |
|---|---|
| `space` | play / pause |
| `←` `→` | seek 5 s |
| `⇧←` `⇧→` / `p` `n` | previous / next track |
| `↑` `↓` | volume |
| `⇧↑` `⇧↓` / `j` | move the selection |
| `↵` | play the selected track |
| `s` | stop |
| `h` | shuffle |
| `r` | repeat: off → playlist → track |
| `k` / `K` | next / previous skin |
| `v` / `V` | next / previous visualizer |
| `y` | switch source: library ↔ YouTube Music |
| `m` | switch layout: classic deck ↔ modern |
| `⌫` | remove the selected track |
| `q` | quit |

## Checks

```sh
cargo test --release                       # analyser and headless UI tests
cargo test --release -- --ignored          # real playback through the output device, at volume 0
target/release/nightjar --snap out/deck.png --scene deck        # also: empty, skin1-3, viz1-3, youtube, modern, modern-noart
```

Snapshot scenes use sample tracks and made-up levels. They need no music files and no audio
device.
