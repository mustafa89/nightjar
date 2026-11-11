//! Nightjar: a skinnable music player with live visualizers, as a native Rune window.
//!
//! `cargo run --release -- [files or folders]` opens the window (and plays what it's given).
//! Headless checks: `target/release/nightjar --snap out.png --scene deck` renders sample tracks
//! with made-up levels and no audio device.

mod analysis;
mod art;
mod audio;
mod bridge;
mod icons;
mod library;
mod settings;
mod skin;
mod state;
mod view;
mod viz;
mod youtube;

use rune::{AppSpec, Chrome, Key, KeyEvent, Tx, WindowOptions};
use state::{Play, State};

fn on_key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let shift = ev.mods.shift;
    match &ev.key {
        Key::Left if shift => s.prev(),
        Key::Right if shift => s.next(),
        Key::Left => s.seek_by(-5.0),
        Key::Right => s.seek_by(5.0),
        Key::Up if shift => s.move_selection(-1),
        Key::Down if shift => s.move_selection(1),
        Key::Up => {
            s.set_volume(s.volume + 0.05);
            s.save();
        }
        Key::Down => {
            s.set_volume(s.volume - 0.05);
            s.save();
        }
        Key::Enter => {
            if let Some(i) = s.selected {
                s.play_index(i);
            }
        }
        Key::Backspace | Key::Delete => s.remove_selected(),
        Key::Text(t) => match t.as_str() {
            " " => s.toggle_play(),
            "n" => s.next(),
            "p" => s.prev(),
            "s" => s.stop(),
            "h" => s.toggle_shuffle(),
            "r" => s.cycle_repeat(),
            "k" => s.cycle_skin(1, tx),
            "K" => s.cycle_skin(-1, tx),
            "v" => s.cycle_viz(1),
            "V" => s.cycle_viz(-1),
            "j" => s.move_selection(1),
            "y" => s.toggle_source(),
            "m" => s.toggle_layout(),
            "q" => tx.quit(),
            _ => return false,
        },
        _ => return false,
    }
    s.publish();
    true
}

/// Sample tracks and made-up levels: a playing deck without files or an audio device.
fn deck(s: &mut State) {
    use library::{Meta, Track};
    s.demo = true;
    let rows = [
        ("Night Drive", "The Lanterns", "Coastline", 247.0, "MP3"),
        ("Glass Garden", "Mira Okafor", "Greenhouse", 198.0, "FLAC"),
        ("Satellite Hearts", "Neon Tide", "Orbit", 312.0, "FLAC"),
        ("Paper Moons", "The Lanterns", "Coastline", 221.0, "MP3"),
        ("Low Tide", "Sola", "Fathoms", 265.0, "M4A"),
        ("Afterglow Avenue", "Kite Club", "Weekends", 189.0, "MP3"),
    ];
    s.tracks = rows
        .iter()
        .map(|r| Track {
            path: format!("/music/{}.mp3", r.0).into(),
            meta: Meta {
                title: Some(r.0.into()),
                artist: Some(r.1.into()),
                album: Some(r.2.into()),
                duration: Some(r.3),
                sample_rate: Some(44_100),
                channels: Some(2),
                codec: Some(r.4.into()),
            },
            read: true,
        })
        .collect();
    s.current = Some(2);
    s.selected = Some(2);
    s.play = Play::Playing;
    s.analyzer.levels = analysis::Analyzer::demo(1.3);
    s.publish();
    // Halfway through the track: the clock and seek bar show a position.
    s.scrub = Some(0.42);
    s.publish();
    s.clock.set("02:11");
}

fn empty(s: &mut State) {
    s.demo = true;
    s.tracks.clear();
    s.current = None;
    s.selected = None;
}

/// A scene per skin and visualizer mode, for snapshots: `--scene skin2`, `--scene viz3`.
macro_rules! variant {
    ($name:ident, $field:ident, $n:expr) => {
        fn $name(s: &mut State) {
            deck(s);
            s.$field = $n;
        }
    };
}
variant!(skin1, skin, 1);
variant!(skin2, skin, 2);
variant!(skin3, skin, 3);
variant!(viz1, viz, 1);
variant!(viz2, viz, 2);
variant!(viz3, viz, 3);

/// The modern layout with a stand-in cover (a diagonal gradient).
fn modern(s: &mut State) {
    deck(s);
    s.modern = true;
    let store = art::ArtStore::new(std::sync::Arc::new(|| {}));
    let n = 256usize;
    let mut px = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let t = (x + y) as f32 / (2 * n) as f32;
            px.extend_from_slice(&[
                (255.0 * (1.0 - t)) as u8,
                (70.0 + 110.0 * t) as u8,
                (150.0 + 100.0 * t) as u8,
                255,
            ]);
        }
    }
    let image = store.rgba(n as u32, n as u32, px);
    let _ = image.wait();
    s.art = Some(image);
}

/// The modern layout on YouTube, with a queue.
fn youtube_queue(s: &mut State) {
    modern(s);
    let queue = [
        ("Glass Garden", "Mira Okafor", "3:18"),
        ("Paper Moons", "The Lanterns", "3:41"),
        ("Low Tide", "Sola", "4:25"),
    ]
    .iter()
    .map(|q| youtube::QueueItem {
        title: q.0.into(),
        artist: q.1.into(),
        dur: q.2.into(),
    })
    .collect();
    s.fake_youtube(youtube::Status {
        state: "playing".into(),
        title: "Night Drive".into(),
        artist: "The Lanterns".into(),
        album: "Coastline".into(),
        pos: 64.0,
        dur: 247.0,
        signed_in: true,
        tap: "ok".into(),
        queue,
        ..Default::default()
    });
}

/// The modern layout for a track without art.
fn modern_noart(s: &mut State) {
    deck(s);
    s.modern = true;
}

/// The YouTube source with no helper running.
fn youtube(s: &mut State) {
    deck(s);
    s.source = state::Source::YouTube;
}

fn main() {
    rune::prewarm_gpu();
    // Content runs under the title bar; the traffic lights sit in the deck's header.
    let chrome = Chrome {
        full_size_content: true,
        traffic_lights: Some((28.0, 25.0)),
        drag_regions: true,
    };
    rune::App::new(AppSpec::<State> {
        scenes: &[
            ("deck", deck),
            ("empty", empty),
            ("skin1", skin1),
            ("skin2", skin2),
            ("skin3", skin3),
            ("viz1", viz1),
            ("viz2", viz2),
            ("viz3", viz3),
            ("youtube", youtube),
            ("modern", modern),
            ("modern-noart", modern_noart),
            ("youtube-queue", youtube_queue),
        ],
        fonts: rune_theme_opencode::FONTS,
        families: rune_theme_opencode::FAMILIES,
        setup: Some(|s, waker| s.start(waker)),
        poll: Some(|s, t, tx| s.poll(t, tx)),
        on_key: Some(on_key),
        min_size: Some((820.0, 560.0)),
        ..AppSpec::new("Nightjar", (1040.0, 640.0), State::new, view::view)
    })
    .chrome(chrome)
    .compositor(viz::Viz::default())
    .window_options(WindowOptions {
        min_size: Some((820.0, 560.0)),
        ..WindowOptions::default()
    })
    .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn deck_runtime() -> Runtime<State> {
        // Never read or write the real settings file.
        unsafe { std::env::set_var("NIGHTJAR_CONFIG", "/nonexistent/nightjar/settings.json") };
        let mut s = State::new();
        deck(&mut s);
        let mut rt = Runtime::new(
            s,
            view::view,
            (1040.0, 640.0),
            Env {
                system_dark: true,
                scale: 2.0,
            },
        );
        rt.frame(0.0);
        rt
    }

    fn click(rt: &mut Runtime<State>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn the_skin_chip_and_the_mode_arrows_cycle() {
        let mut rt = deck_runtime();
        click(&mut rt, "skin");
        assert_eq!(rt.state.skin, 1);
        click(&mut rt, "viz-next");
        click(&mut rt, "viz-next");
        assert_eq!(rt.state.viz, 2);
        click(&mut rt, "viz-prev");
        click(&mut rt, "viz-prev");
        click(&mut rt, "viz-prev");
        assert_eq!(rt.state.viz, state::VIZ_MODES.len() - 1, "wraps backwards");
    }

    /// Whether a press at (x, y) starts moving the window.
    fn drags_window(rt: &mut Runtime<State>, x: f32, y: f32) -> bool {
        rt.pointer_down(x, y);
        let action = rt.take_titlebar_action();
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        matches!(action, Some(rune::TitlebarAction::Drag))
    }

    #[test]
    fn bare_surfaces_move_the_window_and_controls_do_not() {
        let mut rt = deck_runtime();
        assert!(drags_window(&mut rt, 6.0, 300.0), "window padding");
        let (lx, ly) = rt.find("layout").unwrap();
        assert!(
            drags_window(&mut rt, lx - 22.0, ly),
            "deck header between wordmark and buttons"
        );
        assert!(drags_window(&mut rt, 700.0, 300.0), "visualizer stage");
        let (x, y) = rt.find("skin").unwrap();
        assert!(!drags_window(&mut rt, x, y), "skin chip keeps its press");
        assert_eq!(rt.state.skin, 1, "and the press still switched the skin");
    }

    #[test]
    fn the_layout_button_switches_to_modern_and_back() {
        let mut rt = deck_runtime();
        click(&mut rt, "layout");
        assert!(rt.state.modern);
        assert!(
            rt.find("art").is_some() && rt.find("upnext").is_some(),
            "art and up next show"
        );
        assert!(rt.find("stage").is_none(), "the visualizer stage doesn't");
        click(&mut rt, "layout");
        assert!(!rt.state.modern && rt.find("stage").is_some());
    }

    #[test]
    fn up_next_follows_the_playlist_and_its_repeat_mode() {
        let mut rt = deck_runtime(); // six tracks, the third (index 2) playing
        let s = &mut rt.state;
        s.shuffle = false;
        s.repeat = state::Repeat::Off;
        assert_eq!(s.up_next(10), vec![3, 4, 5]);
        s.repeat = state::Repeat::All;
        assert_eq!(
            s.up_next(10),
            vec![3, 4, 5, 0, 1],
            "wraps, never repeats the current track"
        );
        assert_eq!(s.up_next(2), vec![3, 4]);
        s.repeat = state::Repeat::One;
        assert_eq!(s.up_next(10), vec![2]);
        s.repeat = state::Repeat::All;
        s.toggle_shuffle();
        let next = s.up_next(10);
        let mut seen = next.clone();
        seen.sort();
        assert_eq!(
            seen,
            vec![0, 1, 3, 4, 5],
            "shuffled: every other track once"
        );
        assert_eq!(s.up_next(10), next, "and the same order on every look");
    }

    #[test]
    fn skins_cycle_every_seven_seconds_until_the_toggle_stops_them() {
        let mut rt = deck_runtime();
        rt.state.skin_cycle = true;
        let start = rt.state.skin;
        let next = (start + 1) % skin::SKINS.len();
        // Poll at chosen app-clock times (seconds).
        let poll_at = |rt: &mut Runtime<State>, t: f64| {
            rt.update(|s, tx| {
                s.poll(t, tx);
            })
        };
        poll_at(&mut rt, 1.0);
        poll_at(&mut rt, 7.9);
        assert_eq!(rt.state.skin, start, "not before 7 s");
        poll_at(&mut rt, 8.1);
        assert_eq!(rt.state.skin, next, "the next skin after 7 s");
        click(&mut rt, "skin-cycle");
        assert!(!rt.state.skin_cycle);
        poll_at(&mut rt, 9.0);
        poll_at(&mut rt, 30.0);
        assert_eq!(rt.state.skin, next, "stays once stopped");
    }

    #[test]
    fn a_click_on_a_youtube_queue_song_sends_it_to_the_helper() {
        unsafe { std::env::set_var("NIGHTJAR_CONFIG", "/nonexistent/nightjar/settings.json") };
        let mut s = State::new();
        youtube_queue(&mut s); // modern layout, a connected (offline) helper with three queued songs
        let mut rt = Runtime::new(
            s,
            view::view,
            (1040.0, 640.0),
            Env {
                system_dark: true,
                scale: 2.0,
            },
        );
        rt.frame(0.0);
        assert!(rt.find("yt-next-2").is_some(), "the queue shows");
        click(&mut rt, "yt-next-1");
        assert_eq!(
            rt.state.message, None,
            "sent to the running helper; nothing tried to open it"
        );
    }

    #[test]
    fn a_click_selects_a_track_without_playing_it() {
        let mut rt = deck_runtime();
        click(&mut rt, "track-4");
        assert_eq!(rt.state.selected, Some(4));
        assert_eq!(rt.state.current, Some(2), "the playing track stays");
    }
}
