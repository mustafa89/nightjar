//! The window: the deck (display, spectrum, seek bar, transport, volume) over the playlist on
//! the left, and the visualizer stage on the right. Every colour comes from the skin.

use crate::analysis::BANDS;
use crate::icons;
use crate::library::{self, EXTENSIONS};
use crate::skin::{Skin, c, skin};
use crate::state::{BAR_GAP, BAR_H, BAR_W, Play, Repeat, SEEK_W, State, VIZ_MODES};
use crate::viz::VIZ_MARK;
use rune::tok::TextStyle;
use rune::{
    Color, Cursor, Cx, Drag, El, FileDialogOptions, Icon, Phase, ScrollSpec, Shadow, Tx, col, el,
    row, spacer, text,
};

pub const DECK_W: f32 = 376.0;
const PAD: f32 = 14.0;

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
    let left = col()
        .w(DECK_W)
        .h_full()
        .gap(12.0)
        .child(deck(s, k))
        .child(playlist(s, k));
    row()
        .key("root")
        .w_full()
        .h_full()
        .padding(PAD)
        .gap(12.0)
        .gradient(c(k.bg[0]), c(k.bg[1]))
        .child(left)
        .child(stage(s, k))
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

fn deck(s: &State, k: &'static Skin) -> El {
    // The traffic lights sit at the deck's top-left; the band beside them drags the window.
    let header = row()
        .h(22.0)
        .pl(62.0)
        .center_y()
        .gap(8.0)
        .window_drag()
        .child(text("NIGHTJAR").font(WORDMARK).color(c(k.accent)))
        .child(spacer())
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
        );
    card(k)
        .key("deck")
        .w_full()
        .padding(PAD)
        .pt(10.0)
        .gap(12.0)
        .child(header)
        .child(display(s, k))
        .child(seek(s, k))
        .child(transport(s, k))
}

fn display(s: &State, k: &'static Skin) -> El {
    let track = s
        .current_track()
        .or_else(|| s.selected.and_then(|i| s.tracks.get(i)));
    let (title, sub) = match track {
        Some(t) => {
            let sub = [t.artist(), t.meta.album.as_deref().unwrap_or("")]
                .iter()
                .filter(|x| !x.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · ");
            (t.title(), sub)
        }
        None => (
            "Drop music here".to_string(),
            "mp3 · flac · wav · m4a · ogg".to_string(),
        ),
    };
    let state = match s.play {
        Play::Playing => "PLAYING",
        Play::Paused => "PAUSED",
        Play::Stopped => "STOPPED",
    };
    let mut tags: Vec<String> = Vec::new();
    let meta = track.map(|t| &t.meta);
    let rate = s
        .info
        .as_ref()
        .filter(|_| s.current.is_some())
        .map(|i| i.sample_rate)
        .or(meta.and_then(|m| m.sample_rate));
    if let Some(r) = rate {
        tags.push(format!("{:.1} kHz", r as f32 / 1000.0).replace(".0 ", " "));
    }
    let ch = s
        .info
        .as_ref()
        .filter(|_| s.current.is_some())
        .map(|i| i.channels)
        .or(meta.and_then(|m| m.channels));
    if let Some(ch) = ch {
        tags.push(if ch == 1 {
            "MONO".into()
        } else {
            "STEREO".into()
        });
    }
    if let Some(codec) = meta.and_then(|m| m.codec.clone()) {
        tags.push(codec);
    }
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
                        if s.play == Play::Playing {
                            &icons::PLAY
                        } else if s.play == Play::Paused {
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
    let on = s.play != Play::Stopped;
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
                Phase::Down | Phase::Move if s.play != Play::Stopped => s.scrub = Some(f),
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
    let playing = s.play == Play::Playing;
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
    let total: f64 = s.tracks.iter().filter_map(|t| t.meta.duration).sum();
    let count = s.tracks.len();
    let header = row()
        .w_full()
        .h(26.0)
        .gap(4.0)
        .center_y()
        .child(text("PLAYLIST").font(TINY).color(c(k.accent)))
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
    let now = s.current_track().map(|t| {
        col()
            .key(format!("now-{}", t.path.display()))
            .absolute(22.0, 0.0)
            .pin(Some(22.0), None, Some(22.0), Some(20.0))
            .gap(2.0)
            .child(
                text(t.title())
                    .font(STAGE_TITLE)
                    .color(Color::WHITE)
                    .truncate_fade(),
            )
            .child(
                text(t.artist().to_string())
                    .font(TextStyle::new(14.0, 500.0))
                    .color(Color::WHITE.alpha(0.7))
                    .truncate_fade(),
            )
    });
    let playing = if s.play == Play::Playing { 1.0 } else { 0.0 };
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
