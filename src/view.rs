//! The window: the deck (display, spectrum, seek bar, transport, volume) over the playlist on
//! the left, and the visualizer stage on the right. Every colour comes from the skin.

use crate::analysis::BANDS;
use crate::icons;
use crate::library::{self, EXTENSIONS};
use crate::skin::{Skin, c, skin};
use crate::state::{BAR_GAP, BAR_H, BAR_W, Play, Repeat, SEEK_W, Source, State, VIZ_MODES};
use crate::viz::VIZ_MARK;
use rune::tok::TextStyle;
use rune::{
    Color, Cursor, Cx, Drag, El, FileDialogOptions, Icon, Phase, ScrollSpec, Shadow, SurfaceFit,
    Tx, col, el, row, spacer, text,
};

pub const DECK_W: f32 = 376.0;
const PAD: f32 = 14.0;
/// Header inset from the deck's inner edge past the traffic lights, with a 12 pt gap.
const LIGHTS_CLEAR: f32 = 70.0;

const WORDMARK: TextStyle = TextStyle::new(11.0, 800.0).tracking(3.0);
const CLOCK: TextStyle = TextStyle::new(34.0, 500.0).mono().features("'tnum' 1");
const TITLE: TextStyle = TextStyle::new(15.0, 650.0);
const SMALL: TextStyle = TextStyle::new(11.5, 500.0);
const TINY: TextStyle = TextStyle::new(9.5, 700.0).tracking(1.2);
const ROW: TextStyle = TextStyle::new(12.5, 500.0);
const ROW_MONO: TextStyle = TextStyle::new(11.5, 500.0).mono().features("'tnum' 1");
const STAGE_TITLE: TextStyle = TextStyle::new(26.0, 750.0);

pub fn view(s: &State, _cx: &mut Cx<State>) -> El {
    let k = skin(s.skin);
    let (left, right) = if s.modern {
        (modern_left(s, k), up_next(s, k))
    } else {
        let left = col()
            .w(DECK_W)
            .h_full()
            .gap(12.0)
            .child(deck(s, k))
            .child(playlist(s, k));
        (left, stage(s, k))
    };
    row()
        .key("root")
        .w_full()
        .h_full()
        .padding(PAD)
        .gap(12.0)
        .gradient(c(k.bg[0]), c(k.bg[1]))
        // Any bare surface moves the window, as on a skinned player; controls keep their presses.
        .window_drag()
        .child(left)
        .child(right)
        .on_drop_files(|s: &mut State, files, _tx| {
            s.drop_hover = false;
            s.add_paths(files);
        })
        .accepts_files(|p| p.is_dir() || library::is_music(p))
        .on_drop_hover(|s: &mut State, over, _tx| s.drop_hover = over)
}

/// A raised card: skin face, bevelled edge and a soft drop shadow.
fn card(k: &Skin) -> El {
    col()
        .radius(16.0)
        .gradient(c(k.panel[0]), c(k.panel[1]))
        .border_gradient(1.0, c(k.bevel[0]), c(k.bevel[1]))
        .box_shadow(Shadow {
            x: 0.0,
            y: 10.0,
            blur: 28.0,
            spread: -6.0,
            color: Color::BLACK.alpha(if k.dark { 0.55 } else { 0.18 }),
            inset: false,
        })
}

/// Wordmark, layout switch and skin chip. `inset` is the room the traffic lights take from
/// the header's left edge (window x 28 to about 86, plus a 12 pt gap).
fn header(s: &State, k: &'static Skin, inset: f32) -> El {
    row()
        .w_full()
        .h(22.0)
        .pl(inset)
        .center_y()
        .gap(6.0)
        .child(text("NIGHTJAR").font(WORDMARK).color(c(k.accent)))
        .child(spacer())
        .child(
            el().key("layout")
                .size(22.0, 22.0)
                .radius(11.0)
                .center()
                .fill(c(k.accent).alpha(0.12))
                .hover(c(k.accent).alpha(0.22))
                .cursor(Cursor::Pointer)
                .child(rune::icon(&icons::LAYOUT, 12.0).color(c(k.accent)))
                .help(if s.modern {
                    "Classic deck (M)"
                } else {
                    "Modern layout (M)"
                })
                .on_press(|s: &mut State| s.toggle_layout()),
        )
        .child(
            row()
                .key("skin")
                .h(22.0)
                .px(8.0)
                .gap(5.0)
                .center_y()
                .radius(11.0)
                .fill(c(k.accent).alpha(0.12))
                .hover(c(k.accent).alpha(0.22))
                .cursor(Cursor::Pointer)
                .no_window_drag()
                .child(rune::icon(&icons::SKIN, 12.0).color(c(k.accent)))
                .child(text(k.name).font(SMALL).color(c(k.text)))
                .help("Next skin (K)")
                .on_press_tx(|s: &mut State, tx: &mut Tx| s.cycle_skin(1, tx)),
        )
}

fn deck(s: &State, k: &'static Skin) -> El {
    card(k)
        .key("deck")
        .w_full()
        .padding(PAD)
        .pt(10.0)
        .gap(12.0)
        .child(header(s, k, LIGHTS_CLEAR))
        .child(display(s, k))
        .child(seek(s, k))
        .child(transport(s, k))
}

fn display(s: &State, k: &'static Skin) -> El {
    let now = s.now();
    let play = now.play;
    let title = now.title;
    let sub = [now.artist.as_str(), now.album.as_str()]
        .iter()
        .filter(|x| !x.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
    let state = match play {
        Play::Playing => "PLAYING",
        Play::Paused => "PAUSED",
        Play::Stopped => "STOPPED",
    };
    let tags = now.tags;
    let lit = c(k.lcd_text);
    let tag = |t: String| {
        el().h(16.0)
            .px(5.0)
            .center()
            .radius(4.0)
            .border(1.0, c(k.lcd_dim))
            .child(text(t).font(TINY).color(lit.alpha(0.8)))
    };

    let readout = col()
        .w(118.0)
        .gap(2.0)
        .child(
            row()
                .gap(5.0)
                .center_y()
                .child(
                    rune::icon(
                        if play == Play::Playing {
                            &icons::PLAY
                        } else if play == Play::Paused {
                            &icons::PAUSE
                        } else {
                            &icons::STOP
                        },
                        9.0,
                    )
                    .color(lit),
                )
                .child(text(state).font(TINY).color(lit.alpha(0.75))),
        )
        .child(
            text("00:00")
                .key("clock")
                .live_text(&s.clock)
                .font(CLOCK)
                .color(lit)
                .cursor(Cursor::Pointer)
                .help("Elapsed / remaining")
                .on_press(|s: &mut State| s.remaining = !s.remaining),
        );
    let about = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(3.0)
        .pt(2.0)
        .child(
            text(title)
                .key("title")
                .font(TITLE)
                .color(lit)
                .truncate_fade(),
        )
        .child(
            text(if sub.is_empty() { " ".into() } else { sub })
                .font(SMALL)
                .color(lit.alpha(0.6))
                .truncate_fade(),
        )
        .child(row().gap(4.0).pt(3.0).children(tags.into_iter().map(tag)));

    col()
        .key("lcd")
        .w_full()
        .padding(12.0)
        .gap(10.0)
        .radius(10.0)
        .fill(c(k.lcd))
        .box_shadow(Shadow {
            x: 0.0,
            y: 2.0,
            blur: 8.0,
            spread: 0.0,
            color: Color::BLACK.alpha(0.6),
            inset: true,
        })
        .border(1.0, Color::BLACK.alpha(0.35))
        .child(row().w_full().gap(10.0).child(readout).child(about))
        .child(spectrum(s, k))
}

fn spectrum(s: &State, k: &'static Skin) -> El {
    let (top, bottom) = (c(k.bars[0]), c(k.bars[1]));
    let slot = |i: usize| {
        el().size(BAR_W, BAR_H)
            .radius(2.0)
            .gradient(top.alpha(0.10), bottom.alpha(0.16))
            .child(
                el().radius(2.0)
                    .gradient(top, bottom)
                    .live_rect(&s.bars[i])
                    .absolute(0.0, 0.0),
            )
            .child(
                el().radius(1.0)
                    .fill(c(k.lcd_text))
                    .live_rect(&s.peaks[i])
                    .absolute(0.0, 0.0),
            )
    };
    row()
        .key("spectrum")
        .w_full()
        .h(BAR_H)
        .gap(BAR_GAP)
        .justify_center()
        .children((0..BANDS).map(slot))
}

fn seek(s: &State, k: &'static Skin) -> El {
    let on = s.now_play() != Play::Stopped;
    let track = el().w(SEEK_W).h(4.0).radius(2.0).fill(c(k.lcd_dim)).child(
        el().radius(2.0)
            .linear_gradient(90.0, [(0.0, c(k.accent2)), (1.0, c(k.accent))])
            .live_rect(&s.progress)
            .absolute(0.0, 0.0),
    );
    let knob = el()
        .size(14.0, 14.0)
        .radius(7.0)
        .gradient(Color::WHITE, c(k.accent).mix(Color::WHITE, 0.4))
        .border(1.0, Color::BLACK.alpha(0.25))
        .box_shadow(Shadow {
            x: 0.0,
            y: 0.0,
            blur: 10.0,
            spread: 0.0,
            color: c(k.accent).alpha(0.7),
            inset: false,
        })
        .live_offset(&s.knob)
        .absolute(0.0, -5.0)
        .opacity(if on { 1.0 } else { 0.0 });
    el().key("seek")
        .w(SEEK_W)
        .h(14.0)
        .pt(5.0)
        .cursor(if on { Cursor::Pointer } else { Cursor::Default })
        .child(track.child(knob))
        .on_drag(|s: &mut State, d: &mut Drag, _tx| {
            let f = (d.x / d.w).clamp(0.0, 1.0);
            match d.phase {
                Phase::Down | Phase::Move if s.now_play() != Play::Stopped => s.scrub = Some(f),
                Phase::Up => {
                    if let (Some(f), Some(dur)) = (s.scrub.take(), s.duration()) {
                        s.seek_to(f as f64 * dur);
                    }
                }
                _ => {}
            }
            s.publish();
        })
}

/// A domed button with a pressed-in feel.
fn button(k: &Skin, key: &str, icon: &'static Icon, size: f32, lit: bool) -> El {
    let ink = if lit { c(k.accent) } else { c(k.text) };
    el().key(key)
        .size(size, size)
        .radius(size / 2.0)
        .center()
        .gradient(c(k.button[0]), c(k.button[1]))
        .border_gradient(1.0, c(k.bevel[0]), c(k.bevel[1]))
        .box_shadow(Shadow {
            x: 0.0,
            y: 3.0,
            blur: 8.0,
            spread: -2.0,
            color: Color::BLACK.alpha(if k.dark { 0.6 } else { 0.2 }),
            inset: false,
        })
        .hover(Color::WHITE.alpha(0.06))
        .press(Color::BLACK.alpha(0.12))
        .press_scale(0.92)
        .cursor(Cursor::Pointer)
        .child(rune::icon(icon, size * 0.42).color(ink))
}

fn transport(s: &State, k: &'static Skin) -> El {
    let playing = s.now_play() == Play::Playing;
    let vol_w = 78.0;
    let volume = row()
        .gap(6.0)
        .center_y()
        .child(
            rune::icon(
                if s.volume == 0.0 {
                    &icons::MUTED
                } else {
                    &icons::VOLUME
                },
                14.0,
            )
            .color(c(k.muted))
            .cursor(Cursor::Pointer)
            .on_press(|s: &mut State| {
                s.set_volume(if s.volume > 0.0 { 0.0 } else { 0.8 });
                s.save();
            }),
        )
        .child(
            el().key("volume")
                .w(vol_w)
                .h(14.0)
                .pt(5.0)
                .cursor(Cursor::Pointer)
                .help("Volume (↑ ↓)")
                .child(
                    el().w(vol_w)
                        .h(4.0)
                        .radius(2.0)
                        .fill(c(k.lcd_dim))
                        .child(
                            el().w(vol_w * s.volume)
                                .h(4.0)
                                .radius(2.0)
                                .fill(c(k.accent2)),
                        )
                        .child(
                            el().size(10.0, 10.0)
                                .radius(5.0)
                                .fill(c(k.text))
                                .absolute(vol_w * s.volume - 5.0, -3.0),
                        ),
                )
                .on_drag(|s: &mut State, d: &mut Drag, _tx| {
                    s.set_volume(d.x / d.w);
                    if d.phase == Phase::Up {
                        s.save();
                    }
                }),
        );
    row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            button(k, "prev", &icons::PREV, 34.0, false)
                .help("Previous (P)")
                .on_press(|s: &mut State| s.prev()),
        )
        .child(
            button(
                k,
                "play",
                if playing { &icons::PAUSE } else { &icons::PLAY },
                44.0,
                playing,
            )
            .help("Play / pause (Space)")
            .on_press(|s: &mut State| s.toggle_play()),
        )
        .child(
            button(k, "stop", &icons::STOP, 34.0, false)
                .help("Stop (S)")
                .on_press(|s: &mut State| s.stop()),
        )
        .child(
            button(k, "next", &icons::NEXT, 34.0, false)
                .help("Next (N)")
                .on_press(|s: &mut State| s.next()),
        )
        .child(spacer())
        .child(
            toggle(k, "shuffle", &icons::SHUFFLE, s.shuffle)
                .help("Shuffle (H)")
                .on_press(|s: &mut State| s.toggle_shuffle()),
        )
        .child(
            toggle(
                k,
                "repeat",
                if s.repeat == Repeat::One {
                    &icons::REPEAT_ONE
                } else {
                    &icons::REPEAT
                },
                s.repeat != Repeat::Off,
            )
            .help("Repeat (R)")
            .on_press(|s: &mut State| s.cycle_repeat()),
        )
        .child(volume)
}

fn toggle(k: &Skin, key: &str, icon: &'static Icon, on: bool) -> El {
    el().key(key)
        .size(26.0, 26.0)
        .radius(7.0)
        .center()
        .fill(if on {
            c(k.accent).alpha(0.16)
        } else {
            Color::TRANSPARENT
        })
        .hover(c(k.text).alpha(0.08))
        .cursor(Cursor::Pointer)
        .child(rune::icon(icon, 15.0).color(if on { c(k.accent) } else { c(k.muted) }))
}

fn small_button(k: &Skin, key: &str, icon: &'static Icon) -> El {
    el().key(key)
        .size(24.0, 24.0)
        .radius(6.0)
        .center()
        .hover(c(k.text).alpha(0.08))
        .cursor(Cursor::Pointer)
        .child(rune::icon(icon, 14.0).color(c(k.muted)))
}

fn playlist(s: &State, k: &'static Skin) -> El {
    if s.source == Source::YouTube {
        return youtube_card(s, k);
    }
    let total: f64 = s.tracks.iter().filter_map(|t| t.meta.duration).sum();
    let count = s.tracks.len();
    let header = row()
        .w_full()
        .h(26.0)
        .gap(4.0)
        .center_y()
        .child(source_tabs(s, k))
        .child(
            text(if count == 0 {
                String::new()
            } else {
                format!("{count} · {}", library::clock(total))
            })
            .font(SMALL)
            .color(c(k.muted))
            .pl(6.0),
        )
        .child(spacer())
        .child(
            small_button(k, "add-file", &icons::PLUS)
                .help("Add a file")
                .on_press_tx(|_: &mut State, tx: &mut Tx| {
                    let options = FileDialogOptions {
                        title: "Add music".into(),
                        ..Default::default()
                    }
                    .filter("Music", EXTENSIONS);
                    tx.open_file(options, |s: &mut State, result, _tx| match result {
                        Ok(Some(path)) => s.add_paths(&[path]),
                        Ok(None) => {}
                        Err(e) => s.say(e.to_string()),
                    });
                }),
        )
        .child(
            small_button(k, "add-folder", &icons::FOLDER)
                .help("Add a folder")
                .on_press_tx(|_: &mut State, tx: &mut Tx| {
                    let options = FileDialogOptions {
                        title: "Add a music folder".into(),
                        ..Default::default()
                    };
                    tx.open_folder(options, |s: &mut State, result, _tx| match result {
                        Ok(Some(path)) => s.add_paths(&[path]),
                        Ok(None) => {}
                        Err(e) => s.say(e.to_string()),
                    });
                }),
        )
        .child(
            small_button(k, "clear", &icons::TRASH)
                .help("Clear the playlist")
                .on_press(|s: &mut State| s.clear()),
        );

    let body = if s.tracks.is_empty() {
        col()
            .w_full()
            .grow(1.0)
            .center()
            .gap(6.0)
            .child(rune::icon(&icons::FOLDER, 28.0).color(c(k.muted).alpha(0.6)))
            .child(
                text("Drop files or folders anywhere")
                    .font(SMALL)
                    .color(c(k.muted)),
            )
    } else {
        col()
            .key("rows")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .clip_soft(8.0)
            .children(
                s.tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| track_row(s, k, i, t)),
            )
    };
    let footer = text(
        s.message
            .clone()
            .unwrap_or_else(|| "Space play · N next · K skin · V visual".into()),
    )
    .key("status")
    .font(SMALL)
    .color(c(k.muted))
    .truncate_fade();

    card(k)
        .key("playlist")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .padding(12.0)
        .pt(8.0)
        .gap(6.0)
        .border_gradient(
            1.0,
            if s.drop_hover {
                c(k.accent)
            } else {
                c(k.bevel[0])
            },
            if s.drop_hover {
                c(k.accent2)
            } else {
                c(k.bevel[1])
            },
        )
        .child(header)
        .child(body)
        .child(footer)
}

/// LIBRARY | YOUTUBE: which source the deck plays (Y switches).
fn source_tabs(s: &State, k: &Skin) -> El {
    let tab = |key: &str, label: &str, source: Source| {
        let on = s.source == source;
        el().key(key)
            .h(22.0)
            .px(9.0)
            .center()
            .radius(11.0)
            .fill(if on {
                c(k.accent).alpha(0.16)
            } else {
                Color::TRANSPARENT
            })
            .hover(c(k.text).alpha(0.06))
            .cursor(Cursor::Pointer)
            .child(text(label.to_string()).font(TINY).color(if on {
                c(k.accent)
            } else {
                c(k.muted)
            }))
            .on_press(move |s: &mut State| s.set_source(source))
    };
    row()
        .gap(2.0)
        .child(tab("src-local", "LIBRARY", Source::Local))
        .child(tab("src-youtube", "YOUTUBE", Source::YouTube))
        .help("Source (Y)")
}

/// The playlist card in YouTube mode: the helper's state and a button to its window, where
/// music is searched and picked.
fn youtube_card(s: &State, k: &'static Skin) -> El {
    let y = s.yt();
    let line = |label: &str, value: String, good: bool| {
        row()
            .w_full()
            .gap(8.0)
            .child(text(label.to_string()).font(TINY).color(c(k.muted)).w(78.0))
            .child(
                text(value)
                    .font(SMALL)
                    .color(if good { c(k.text) } else { c(k.accent2) })
                    .truncate_fade()
                    .grow(1.0)
                    .min_w(0.0),
            )
    };
    let (window, signed, spectrum) = match y {
        None => (
            ("Not open".to_string(), false),
            ("—".to_string(), true),
            ("—".to_string(), true),
        ),
        Some(y) => (
            ("Open".to_string(), true),
            if y.signed_in {
                ("Signed in".to_string(), true)
            } else {
                ("Signed out: sign in in its window".to_string(), false)
            },
            match y.tap.as_str() {
                t if t.starts_with("ok") => ("Live".to_string(), true),
                t if t.starts_with("error") => (
                    "Allow System Audio Recording in System Settings › Privacy".to_string(),
                    false,
                ),
                "waiting for audio" => ("Starts with playback".to_string(), true),
                _ => ("Waiting for audio permission".to_string(), false),
            },
        ),
    };
    let open = row()
        .key("yt-open")
        .h(32.0)
        .px(14.0)
        .gap(8.0)
        .center_y()
        .radius(16.0)
        .gradient(c(k.button[0]), c(k.button[1]))
        .border_gradient(1.0, c(k.bevel[0]), c(k.bevel[1]))
        .hover(Color::WHITE.alpha(0.06))
        .press_scale(0.96)
        .cursor(Cursor::Pointer)
        .child(rune::icon(&icons::PLAY, 12.0).color(c(k.accent)))
        .child(
            text(if y.is_some() {
                "Show YouTube Music"
            } else {
                "Open YouTube Music"
            })
            .font(SMALL)
            .color(c(k.text)),
        )
        .on_press(|s: &mut State| s.open_youtube());
    let body = col()
        .w_full()
        .grow(1.0)
        .gap(8.0)
        .pt(6.0)
        .child(line("WINDOW", window.0, window.1))
        .child(line("ACCOUNT", signed.0, signed.1))
        .child(line("SPECTRUM", spectrum.0, spectrum.1))
        .child(el().h(6.0))
        .child(open)
        .child(
            text("Search and pick music in its window; the deck, the visualizer and the notch follow it.")
                .font(SMALL)
                .color(c(k.muted))
                .max_lines(3),
        );
    let footer = text(
        s.message
            .clone()
            .unwrap_or_else(|| "Y switches source · Space play · N next".into()),
    )
    .key("status")
    .font(SMALL)
    .color(c(k.muted))
    .truncate_fade();
    card(k)
        .key("playlist")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .padding(12.0)
        .pt(8.0)
        .gap(6.0)
        .child(row().w_full().h(26.0).center_y().child(source_tabs(s, k)))
        .child(body)
        .child(footer)
}

fn track_row(s: &State, k: &Skin, i: usize, t: &library::Track) -> El {
    let current = s.current == Some(i);
    let selected = s.selected == Some(i);
    let ink = if current { c(k.accent) } else { c(k.text) };
    let line = if t.artist().is_empty() {
        t.title()
    } else {
        format!("{} — {}", t.artist(), t.title())
    };
    row()
        .key(format!("track-{i}"))
        .w_full()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(7.0)
        .fill(if selected {
            c(k.accent).alpha(0.14)
        } else {
            Color::TRANSPARENT
        })
        .hover(c(k.text).alpha(0.05))
        .child(
            text(format!("{:>2}", i + 1))
                .font(ROW_MONO)
                .color(c(k.muted))
                .w(20.0),
        )
        .child(
            text(line)
                .font(ROW)
                .color(ink)
                .truncate_fade()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(
            text(t.meta.duration.map(library::clock).unwrap_or_default())
                .font(ROW_MONO)
                .color(c(k.muted)),
        )
        .on_press(move |s: &mut State| s.selected = Some(i))
        .on_double_press(move |s: &mut State| s.play_index(i))
}

fn stage(s: &State, k: &'static Skin) -> El {
    let l = s.levels();
    let mode = row()
        .key("mode")
        .absolute(14.0, 14.0)
        .h(28.0)
        .px(4.0)
        .gap(2.0)
        .center_y()
        .radius(14.0)
        .fill(Color::BLACK.alpha(0.35))
        .backdrop_blur(12.0)
        .child(small_button(k, "viz-prev", &icons::LEFT).on_press(|s: &mut State| s.cycle_viz(-1)))
        .child(
            text(VIZ_MODES[s.viz])
                .font(SMALL)
                .color(Color::WHITE.alpha(0.9))
                .px(4.0),
        )
        .child(small_button(k, "viz-next", &icons::RIGHT).on_press(|s: &mut State| s.cycle_viz(1)));
    let n = s.now();
    let now = (n.play != Play::Stopped).then(|| {
        col()
            .key(format!("now-{}", n.title))
            .absolute(22.0, 0.0)
            .pin(Some(22.0), None, Some(22.0), Some(20.0))
            .gap(2.0)
            .child(
                text(n.title.clone())
                    .font(STAGE_TITLE)
                    .color(Color::WHITE)
                    .truncate_fade(),
            )
            .child(
                text(n.artist.clone())
                    .font(TextStyle::new(14.0, 500.0))
                    .color(Color::WHITE.alpha(0.7))
                    .truncate_fade(),
            )
    });
    let playing = if s.now_play() == Play::Playing {
        1.0
    } else {
        0.0
    };
    card(k)
        .key("stage")
        .grow(1.0)
        .h_full()
        .min_w(0.0)
        .clip_rounded(16.0)
        .fill(Color::BLACK)
        .child(
            rune::effect(VIZ_MARK)
                .key("viz")
                .w_full()
                .h_full()
                .effect_data([s.viz as f32, playing, l.beat, 0.0]),
        )
        .child(mode)
        .children(now)
}

// ── Modern layout ────────────────────────────────────────────────────────────

const MODERN_TITLE: TextStyle = TextStyle::new(20.0, 700.0);
const TIME: TextStyle = TextStyle::new(11.0, 500.0).mono().features("'tnum' 1");

/// Album art over the title, the seek bar with times, and the transport: no card, straight on
/// the window, as music apps do.
fn modern_left(s: &State, k: &'static Skin) -> El {
    let n = s.now();
    let sub = [n.artist.as_str(), n.album.as_str()]
        .iter()
        .filter(|x| !x.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
    let length = s
        .duration()
        .map(library::clock)
        .unwrap_or_else(|| "--:--".into());
    col()
        .key("modern")
        .w(DECK_W)
        .h_full()
        .pt(10.0)
        .gap(12.0)
        // The column starts at the window padding, so the header clears the lights by that much more.
        .child(row().w_full().px(PAD).child(header(s, k, LIGHTS_CLEAR)))
        .child(
            row()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .justify_center()
                .items_center()
                .child(art_box(s, k)),
        )
        .child(
            col()
                .w_full()
                .px(PAD)
                .gap(3.0)
                .child(
                    text(n.title)
                        .key("modern-title")
                        .font(MODERN_TITLE)
                        .color(c(k.text))
                        .truncate_fade(),
                )
                .child(
                    text(if sub.is_empty() { " ".into() } else { sub })
                        .font(SMALL)
                        .color(c(k.muted))
                        .truncate_fade(),
                ),
        )
        .child(
            col().w_full().px(PAD).gap(4.0).child(seek(s, k)).child(
                row()
                    .w_full()
                    .child(
                        text("00:00")
                            .live_text(&s.clock)
                            .font(TIME)
                            .color(c(k.muted)),
                    )
                    .child(spacer())
                    .child(text(length).font(TIME).color(c(k.muted))),
            ),
        )
        .child(row().w_full().px(PAD).child(transport(s, k)))
}

/// The cover, square and as large as the column allows, with the live spectrum along its
/// bottom edge. Without art: the skin's accents and a note.
fn art_box(s: &State, k: &'static Skin) -> El {
    let face = match &s.art {
        Some(image) => el()
            .w_full()
            .h_full()
            .pixels_fit(image.pixels(), SurfaceFit::Fill),
        None => el()
            .w_full()
            .h_full()
            .center()
            .linear_gradient(135.0, [(0.0, c(k.accent2)), (1.0, c(k.accent))])
            .child(rune::icon(&icons::NOTE, 96.0).color(Color::WHITE.alpha(0.85))),
    };
    let fade = col()
        .absolute(0.0, 0.0)
        .pin(Some(0.0), None, Some(0.0), Some(0.0))
        .h(BAR_H + 28.0)
        .pb(12.0)
        .justify_end()
        .gradient(Color::TRANSPARENT, Color::BLACK.alpha(0.55))
        .child(spectrum(s, k));
    el().key("art")
        .aspect_fit(1.0)
        .max_w(DECK_W - 2.0 * PAD)
        .radius(18.0)
        .clip_rounded(18.0)
        .box_shadow(Shadow {
            x: 0.0,
            y: 14.0,
            blur: 36.0,
            spread: -8.0,
            color: Color::BLACK.alpha(if k.dark { 0.6 } else { 0.25 }),
            inset: false,
        })
        .child(face)
        .child(fade)
}

/// The tracks that play next, in a card beside the art; YouTube's card in YouTube mode.
fn up_next(s: &State, k: &'static Skin) -> El {
    let wrap = |inner: El| col().grow(1.0).h_full().min_w(0.0).child(inner);
    if s.source == Source::YouTube {
        return wrap(youtube_card(s, k));
    }
    let next = s.up_next(200);
    let now_row = s.current.and_then(|i| s.tracks.get(i).map(|t| (i, t)));
    let mut body = col()
        .key("upnext-rows")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .clip_soft(8.0);
    if let Some((i, t)) = now_row {
        body = body
            .child(
                text("NOW PLAYING")
                    .font(TINY)
                    .color(c(k.muted))
                    .px(8.0)
                    .pb(2.0),
            )
            .child(track_row(s, k, i, t))
            .child(
                text("UP NEXT")
                    .font(TINY)
                    .color(c(k.muted))
                    .px(8.0)
                    .pt(10.0)
                    .pb(2.0),
            );
    }
    if let Some(note) = s.up_next_note() {
        body = body.child(text(note).font(SMALL).color(c(k.accent2)).px(8.0).pb(4.0));
    }
    body = body.children(next.iter().map(|&i| track_row(s, k, i, &s.tracks[i])));
    if s.tracks.is_empty() {
        body = col()
            .w_full()
            .grow(1.0)
            .center()
            .gap(6.0)
            .child(rune::icon(&icons::FOLDER, 28.0).color(c(k.muted).alpha(0.6)))
            .child(
                text("Drop files or folders anywhere")
                    .font(SMALL)
                    .color(c(k.muted)),
            );
    } else if next.is_empty() && s.up_next_note().is_none() {
        body = body.child(
            text("End of the playlist")
                .font(SMALL)
                .color(c(k.muted))
                .px(8.0)
                .pt(4.0),
        );
    }
    let footer = text(
        s.message
            .clone()
            .unwrap_or_else(|| "M classic deck · Space play · N next".into()),
    )
    .key("status")
    .font(SMALL)
    .color(c(k.muted))
    .truncate_fade();
    wrap(
        card(k)
            .key("upnext")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .padding(12.0)
            .pt(8.0)
            .gap(6.0)
            .child(row().w_full().h(26.0).center_y().child(source_tabs(s, k)))
            .child(body)
            .child(footer),
    )
}
