//! App state and every player action. The view reads it; handlers and `poll` change it.

use crate::analysis::{Analyzer, BANDS, Levels};
use crate::art::{self, ArtStore};
use crate::audio::{Engine, Scope, StreamInfo};
use crate::bridge::{Bridge, Command};
use crate::library::{self, MetaReader, Track};
use crate::settings::{self, Settings};
use crate::skin::{SKINS, skin};
use crate::youtube::{self, YouTube};
use rune::{Live, LiveText, Tx};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Spectrum geometry on the deck, in points.
pub const BAR_W: f32 = 13.0;
pub const BAR_GAP: f32 = 3.0;
pub const BAR_H: f32 = 44.0;
/// Seek track width is the deck's inner width; the view reports it on layout.
pub const SEEK_W: f32 = 348.0;

pub const VIZ_MODES: &[&str] = &[
    "Aurora Tunnel",
    "Spectrum Halo",
    "Scope Ribbon",
    "Plasma Bloom",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Play {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

/// Where the music comes from: files Nightjar plays itself, or the YouTube Music helper app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Local,
    YouTube,
}

/// What the deck, stage and notch show, from whichever source is on.
pub struct Now {
    pub play: Play,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub tags: Vec<String>,
}

impl Repeat {
    fn from(n: u8) -> Repeat {
        match n {
            1 => Repeat::All,
            2 => Repeat::One,
            _ => Repeat::Off,
        }
    }
    fn code(self) -> u8 {
        self as u8
    }
}

pub struct State {
    pub tracks: Vec<Track>,
    /// The loaded track (playing, paused, or stopped at its start).
    pub current: Option<usize>,
    pub selected: Option<usize>,
    pub play: Play,
    pub info: Option<StreamInfo>,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub skin: usize,
    pub viz: usize,
    pub source: Source,
    /// The modern layout: album art and up next instead of the classic deck.
    pub modern: bool,
    /// Cover of what's playing (or selected); None shows the placeholder.
    pub art: Option<rune::Image>,
    pub message: Option<String>,
    pub drop_hover: bool,
    /// While the seek knob is held: the fraction it sits at.
    pub scrub: Option<f32>,
    /// Shows remaining time instead of elapsed (click the clock).
    pub remaining: bool,
    /// Headless snapshots: draw made-up levels instead of reading the scope.
    pub demo: bool,

    pub scope: Arc<Scope>,
    pub analyzer: Analyzer,
    engine: Option<Engine>,
    reader: Option<MetaReader>,
    bridge: Option<Bridge>,
    /// Play order as playlist rows: the playlist's own order, or a shuffle with the current
    /// track first. Rebuilt when shuffle toggles or the playlist changes.
    order: Vec<usize>,
    youtube: Option<YouTube>,
    arts: Option<ArtStore>,
    art_source: Option<art::Source>,
    /// When the notch app last got a status, and what it said, to resend on any change.
    sent_at: Option<std::time::Instant>,
    sent_key: String,
    last_t: f64,
    seed: u64,

    // Values the screen draws without re-running the view.
    pub bars: Vec<Live<[f32; 4]>>,
    pub peaks: Vec<Live<[f32; 4]>>,
    pub progress: Live<[f32; 4]>,
    pub knob: Live<[f32; 2]>,
    pub clock: LiveText,
    /// Flipped every poll while playing: a 0.001 pt nudge on the visualizer slot that draws
    /// one frame per poll (about 30 a second) instead of one per display refresh.
    pub tick: Live<[f32; 2]>,
}

impl State {
    pub fn new() -> State {
        let s = settings::load();
        let mut st = State {
            tracks: Vec::new(),
            current: None,
            selected: None,
            play: Play::Stopped,
            info: None,
            volume: s.volume.clamp(0.0, 1.0),
            shuffle: s.shuffle,
            repeat: Repeat::from(s.repeat),
            skin: s.skin % SKINS.len(),
            viz: s.viz % VIZ_MODES.len(),
            source: if s.source == 1 {
                Source::YouTube
            } else {
                Source::Local
            },
            modern: s.modern,
            art: None,
            message: None,
            drop_hover: false,
            scrub: None,
            remaining: false,
            demo: false,
            scope: Scope::new(),
            analyzer: Analyzer::new(),
            engine: None,
            reader: None,
            bridge: None,
            order: Vec::new(),
            youtube: None,
            arts: None,
            art_source: None,
            sent_at: None,
            sent_key: String::new(),
            last_t: 0.0,
            seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(7)
                | 1,
            bars: (0..BANDS)
                .map(|_| Live::new([0.0, BAR_H, BAR_W, 0.0]))
                .collect(),
            peaks: (0..BANDS)
                .map(|_| Live::new([0.0, BAR_H - 2.0, BAR_W, 2.0]))
                .collect(),
            progress: Live::new([0.0, 0.0, 0.0, 4.0]),
            knob: Live::new([0.0, 0.0]),
            clock: LiveText::new("00:00"),
            tick: Live::new([0.0, 0.0]),
        };
        st.tracks = s
            .playlist
            .into_iter()
            .filter(|p| p.exists())
            .map(Track::new)
            .collect();
        if !st.tracks.is_empty() {
            st.selected = Some(0);
        }
        st.rebuild_order();
        st
    }

    /// Window start: the tag reader, and tags for the restored playlist. Paths on the command
    /// line join the playlist and the first of them plays.
    pub fn start(&mut self, waker: rune::Waker) {
        self.bridge = Some(Bridge::start(Some(waker.clone())));
        if self.youtube.is_none() {
            self.youtube = Some(YouTube::start(Some(waker.clone())));
        }
        self.arts = Some(ArtStore::new(waker.clone()));
        let reader = MetaReader::spawn(Some(waker));
        for t in &self.tracks {
            reader.request(t.path.clone());
        }
        self.reader = Some(reader);
        let args: Vec<PathBuf> = rune::extra_args()
            .iter()
            .filter(|a| !a.starts_with("--"))
            .map(PathBuf::from)
            .collect();
        if !args.is_empty() {
            let first = self.tracks.len();
            self.add_paths(&args);
            if first < self.tracks.len() {
                self.play_index(first);
            }
        }
    }

    pub fn levels(&self) -> &Levels {
        &self.analyzer.levels
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    /// The YouTube helper's latest status while it runs.
    pub fn yt(&self) -> Option<&youtube::Status> {
        self.youtube.as_ref().and_then(|y| y.status())
    }

    pub fn youtube_connected(&self) -> bool {
        self.yt().is_some()
    }

    /// Play state of the source that's on.
    pub fn now_play(&self) -> Play {
        match self.source {
            Source::Local => self.play,
            Source::YouTube => match self.yt().map(|y| y.state.as_str()) {
                Some("playing") => Play::Playing,
                Some("paused") => Play::Paused,
                _ => Play::Stopped,
            },
        }
    }

    /// Title, artist, album and display tags of what's playing (or selected) on this source.
    pub fn now(&self) -> Now {
        let play = self.now_play();
        if self.source == Source::YouTube {
            let Some(y) = self.yt() else {
                return Now {
                    play,
                    title: "YouTube Music".into(),
                    artist: "Press play to open it".into(),
                    album: String::new(),
                    tags: vec!["YOUTUBE".into()],
                };
            };
            let mut tags = vec!["YOUTUBE".to_string()];
            if !y.signed_in {
                tags.push("SIGNED OUT".into());
            }
            if y.tap.starts_with("error") {
                tags.push("NO SPECTRUM".into());
            }
            return match y.title.is_empty() {
                true => Now {
                    play,
                    title: "YouTube Music".into(),
                    artist: "Pick something in its window".into(),
                    album: String::new(),
                    tags,
                },
                false => Now {
                    play,
                    title: y.title.clone(),
                    artist: y.artist.clone(),
                    album: y.album.clone(),
                    tags,
                },
            };
        }
        let Some(t) = self
            .current_track()
            .or_else(|| self.selected.and_then(|i| self.tracks.get(i)))
        else {
            return Now {
                play,
                title: "Drop music here".into(),
                artist: "mp3 · flac · wav · m4a · ogg".into(),
                album: String::new(),
                tags: Vec::new(),
            };
        };
        // The decoder's figures for the loaded track, else the tag reader's.
        let info = self.info.as_ref().filter(|_| self.current.is_some());
        let mut tags = Vec::new();
        if let Some(r) = info.map(|i| i.sample_rate).or(t.meta.sample_rate) {
            tags.push(format!("{:.1} kHz", r as f32 / 1000.0).replace(".0 ", " "));
        }
        if let Some(ch) = info.map(|i| i.channels).or(t.meta.channels) {
            tags.push(if ch == 1 {
                "MONO".into()
            } else {
                "STEREO".into()
            });
        }
        if let Some(codec) = t.meta.codec.clone() {
            tags.push(codec);
        }
        Now {
            play,
            title: t.title(),
            artist: t.artist().to_string(),
            album: t.meta.album.clone().unwrap_or_default(),
            tags,
        }
    }

    /// Length of the loaded track: the decoder's figure, else the tag reader's.
    pub fn duration(&self) -> Option<f64> {
        if self.source == Source::YouTube {
            return self.yt().map(|y| y.dur).filter(|d| *d > 0.0);
        }
        self.info
            .as_ref()
            .and_then(|i| i.duration)
            .or_else(|| self.current_track().and_then(|t| t.meta.duration))
    }

    pub fn position(&self) -> f64 {
        if let (Some(f), Some(d)) = (self.scrub, self.duration()) {
            return f as f64 * d;
        }
        if self.source == Source::YouTube {
            return self.yt().map_or(0.0, |y| y.pos);
        }
        match (&self.engine, self.play) {
            (Some(e), Play::Playing | Play::Paused) => e.position(),
            _ => 0.0,
        }
    }

    pub fn say(&mut self, m: impl Into<String>) {
        self.message = Some(m.into());
    }

    pub fn save(&self) {
        if self.demo {
            return;
        }
        settings::save(&Settings {
            skin: self.skin,
            viz: self.viz,
            volume: self.volume,
            shuffle: self.shuffle,
            repeat: self.repeat.code(),
            source: (self.source == Source::YouTube) as u8,
            modern: self.modern,
            playlist: self.tracks.iter().map(|t| t.path.clone()).collect(),
        });
    }

    // ── Playlist ─────────────────────────────────────────────────────────────

    pub fn add_paths(&mut self, paths: &[PathBuf]) {
        let found = library::expand(paths);
        if found.is_empty() {
            self.say("No music in that drop (mp3, flac, wav, m4a, ogg)");
            return;
        }
        let n = found.len();
        for p in found {
            if let Some(r) = &self.reader {
                r.request(p.clone());
            }
            self.tracks.push(Track::new(p));
        }
        self.selected.get_or_insert(self.tracks.len() - n);
        self.rebuild_order();
        self.say(format!("Added {n} track{}", if n == 1 { "" } else { "s" }));
        self.save();
    }

    pub fn remove_selected(&mut self) {
        let Some(i) = self.selected.filter(|&i| i < self.tracks.len()) else {
            return;
        };
        self.tracks.remove(i);
        match self.current {
            Some(c) if c == i => {
                self.stop_local();
                self.current = None;
            }
            Some(c) if c > i => self.current = Some(c - 1),
            _ => {}
        }
        self.selected = if self.tracks.is_empty() {
            None
        } else {
            Some(i.min(self.tracks.len() - 1))
        };
        self.rebuild_order();
        self.save();
    }

    pub fn clear(&mut self) {
        self.stop_local();
        self.tracks.clear();
        self.current = None;
        self.selected = None;
        self.order.clear();
        self.say("Playlist cleared");
        self.save();
    }

    pub fn move_selection(&mut self, d: isize) {
        if self.tracks.is_empty() {
            return;
        }
        let i = self.selected.unwrap_or(0) as isize + d;
        self.selected = Some(i.clamp(0, self.tracks.len() as isize - 1) as usize);
    }

    // ── Transport ────────────────────────────────────────────────────────────

    fn engine(&mut self) -> Option<&Engine> {
        if self.engine.is_none() {
            match Engine::open() {
                Ok(e) => self.engine = Some(e),
                Err(e) => self.say(e),
            }
        }
        self.engine.as_ref()
    }

    pub fn play_index(&mut self, i: usize) {
        let Some(path) = self.tracks.get(i).map(|t| t.path.clone()) else {
            return;
        };
        let (scope, volume) = (self.scope.clone(), self.volume);
        let Some(engine) = self.engine() else { return };
        match engine.load(&path, &scope, volume) {
            Ok(info) => {
                self.info = Some(info);
                self.current = Some(i);
                self.selected = Some(i);
                self.play = Play::Playing;
                self.message = None;
            }
            Err(e) => {
                self.play = Play::Stopped;
                self.say(e);
            }
        }
    }

    pub fn toggle_play(&mut self) {
        if self.source == Source::YouTube {
            return self.yt_send("toggle");
        }
        match self.play {
            Play::Playing => {
                if let Some(e) = &self.engine {
                    e.pause();
                }
                self.play = Play::Paused;
            }
            Play::Paused => {
                if let Some(e) = &self.engine {
                    e.play();
                }
                self.play = Play::Playing;
            }
            Play::Stopped => {
                if let Some(i) = self
                    .selected
                    .or(self.current)
                    .or((!self.tracks.is_empty()).then_some(0))
                {
                    self.play_index(i);
                }
            }
        }
    }

    /// Stop: local playback stops; YouTube pauses (its web player has no stop).
    pub fn stop(&mut self) {
        match self.source {
            Source::YouTube if self.now_play() == Play::Playing => self.yt_send("toggle"),
            Source::YouTube => {}
            Source::Local => self.stop_local(),
        }
    }

    fn stop_local(&mut self) {
        if let Some(e) = &self.engine {
            e.stop();
        }
        self.play = Play::Stopped;
        self.scrub = None;
    }

    fn random(&mut self, n: usize) -> usize {
        // xorshift64*: plenty for picking the next song.
        self.seed ^= self.seed >> 12;
        self.seed ^= self.seed << 25;
        self.seed ^= self.seed >> 27;
        (self.seed.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as usize % n
    }

    /// The playlist's order, or with shuffle on a fresh shuffle (Fisher-Yates) that starts at
    /// the current (else selected) track, so what plays next is known in advance.
    pub fn rebuild_order(&mut self) {
        let n = self.tracks.len();
        self.order = (0..n).collect();
        if !self.shuffle || n < 2 {
            return;
        }
        for i in (1..n).rev() {
            let j = self.random(i + 1);
            self.order.swap(i, j);
        }
        if let Some(first) = self.current.or(self.selected)
            && let Some(p) = self.order.iter().position(|&i| i == first)
        {
            self.order.swap(0, p);
        }
    }

    /// The play order, falling back to playlist order if it's out of date.
    fn play_order(&self) -> std::borrow::Cow<'_, [usize]> {
        if self.order.len() == self.tracks.len() {
            std::borrow::Cow::Borrowed(&self.order)
        } else {
            std::borrow::Cow::Owned((0..self.tracks.len()).collect())
        }
    }

    /// The track `step` places from `current` in play order, or None past the end of a
    /// non-repeating list.
    fn following(&mut self, step: isize) -> Option<usize> {
        let order = self.play_order();
        let n = order.len() as isize;
        if n == 0 {
            return None;
        }
        let Some(cur) = self.current else {
            return Some(order[0]);
        };
        let pos = order.iter().position(|&i| i == cur).unwrap_or(0) as isize + step;
        if (0..n).contains(&pos) {
            Some(order[pos as usize])
        } else if self.repeat == Repeat::All {
            Some(order[pos.rem_euclid(n) as usize])
        } else {
            None
        }
    }

    pub fn next(&mut self) {
        if self.source == Source::YouTube {
            return self.yt_send("next");
        }
        self.next_local();
    }

    fn next_local(&mut self) {
        match self.following(1) {
            Some(i) => self.play_index(i),
            None => self.stop_local(),
        }
    }

    pub fn prev(&mut self) {
        if self.source == Source::YouTube {
            return self.yt_send("prev");
        }
        if self.position() > 3.0 {
            self.seek_to(0.0);
            return;
        }
        if let Some(i) = self.following(-1) {
            self.play_index(i);
        }
    }

    pub fn seek_to(&mut self, seconds: f64) {
        if self.source == Source::YouTube {
            return self.yt_send(&format!("seek {:.1}", seconds.max(0.0)));
        }
        if self.play == Play::Stopped {
            return;
        }
        let d = self.duration().unwrap_or(f64::MAX);
        if let Some(e) = &self.engine
            && let Err(err) = e.seek(seconds.clamp(0.0, (d - 0.25).max(0.0)))
        {
            self.say(err);
        }
    }

    pub fn seek_by(&mut self, delta: f64) {
        self.seek_to(self.position() + delta);
    }

    /// Changes the volume at once; callers save when the gesture ends, not on every step.
    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(e) = &self.engine {
            e.set_volume(self.volume);
        }
        // YouTube follows the same slider, but a volume change never opens its window.
        if let (Source::YouTube, Some(y)) =
            (self.source, self.youtube.as_ref().filter(|y| y.connected()))
        {
            y.send(&format!("volume {:.2}", self.volume));
        }
    }

    // ── Sources ──────────────────────────────────────────────────────────────

    /// Sends a command to the YouTube helper, opening it first when it isn't running.
    fn yt_send(&mut self, command: &str) {
        match &self.youtube {
            Some(y) if y.connected() => y.send(command),
            _ => self.open_youtube(),
        }
    }

    /// Opens the YouTube Music window (or brings it forward).
    pub fn open_youtube(&mut self) {
        let result = self
            .youtube
            .as_ref()
            .map_or(Err("YouTube link not started".to_string()), |y| y.launch());
        match result {
            Ok(()) if !self.youtube_connected() => self.say("Opening YouTube Music…"),
            Ok(()) => {}
            Err(e) => self.say(e),
        }
    }

    /// Switches source; whatever was playing on the other one pauses.
    pub fn set_source(&mut self, source: Source) {
        if source == self.source {
            return;
        }
        if self.now_play() == Play::Playing {
            match self.source {
                Source::Local => {
                    if let Some(e) = &self.engine {
                        e.pause();
                    }
                    self.play = Play::Paused;
                }
                Source::YouTube => self.yt_send("toggle"),
            }
        }
        self.source = source;
        self.scrub = None;
        if source == Source::YouTube && !self.youtube_connected() {
            self.open_youtube();
        }
        self.save();
    }

    /// Headless scenes: YouTube as if the helper had just reported `status`.
    pub fn fake_youtube(&mut self, status: youtube::Status) {
        self.source = Source::YouTube;
        self.youtube = Some(YouTube::offline(status));
    }

    pub fn toggle_layout(&mut self) {
        self.modern = !self.modern;
        self.save();
    }

    // ── Modern layout ────────────────────────────────────────────────────────

    /// Where the cover of what's playing (or selected) comes from.
    fn art_source_now(&self) -> Option<art::Source> {
        match self.source {
            Source::YouTube => self
                .yt()
                .map(|y| y.art.clone())
                .filter(|u| !u.is_empty())
                .map(art::Source::Remote),
            Source::Local => self
                .current_track()
                .or_else(|| self.selected.and_then(|i| self.tracks.get(i)))
                .map(|t| art::Source::Local(t.path.clone())),
        }
    }

    /// Follows the cover to the current track. While the next cover loads the last one stays,
    /// so tracks of one album don't flash the placeholder. Returns true when it changed.
    fn update_art(&mut self) -> bool {
        let source = self.art_source_now();
        let Some(arts) = self.arts.as_mut() else {
            return false;
        };
        let arrived = arts.poll();
        if source == self.art_source && !arrived {
            return false;
        }
        self.art_source = source.clone();
        let before = self.art.as_ref().map(|i| i.id());
        match source.map(|s| arts.get(&s)) {
            None => self.art = None,
            Some(None) => {} // still looking
            Some(Some(found)) => self.art = found,
        }
        before != self.art.as_ref().map(|i| i.id())
    }

    /// Up to `n` playlist rows after the current track (from the selection when nothing is
    /// loaded), wrapping when the playlist repeats; the track itself when it repeats.
    /// Up to `n` rows that play after the current track, in play order (shuffled when shuffle
    /// is on), wrapping when the playlist repeats; the track itself when it repeats. With
    /// nothing loaded: from the selection.
    pub fn up_next(&self, n: usize) -> Vec<usize> {
        let order = self.play_order();
        let len = order.len();
        let Some(cur) = self.current else {
            let start = self
                .selected
                .and_then(|s| order.iter().position(|&i| i == s))
                .unwrap_or(0)
                .min(len);
            return order[start..].iter().copied().take(n).collect();
        };
        if self.repeat == Repeat::One {
            return vec![cur];
        }
        let at = order.iter().position(|&i| i == cur).unwrap_or(0);
        let mut out = Vec::new();
        for k in 1..len {
            if out.len() == n || (at + k >= len && self.repeat != Repeat::All) {
                break;
            }
            out.push(order[(at + k) % len]);
        }
        out
    }

    /// A line over the up-next list when the list alone doesn't say it.
    pub fn up_next_note(&self) -> Option<&'static str> {
        match (self.shuffle, self.repeat) {
            (_, Repeat::One) => Some("Repeating this track"),
            (true, _) => Some("Shuffled"),
            _ => None,
        }
    }

    pub fn toggle_source(&mut self) {
        self.set_source(match self.source {
            Source::Local => Source::YouTube,
            Source::YouTube => Source::Local,
        });
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.rebuild_order();
        self.say(if self.shuffle {
            "Shuffle on"
        } else {
            "Shuffle off"
        });
        self.save();
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
        self.say(match self.repeat {
            Repeat::Off => "Repeat off",
            Repeat::All => "Repeat playlist",
            Repeat::One => "Repeat this track",
        });
        self.save();
    }

    pub fn cycle_skin(&mut self, d: isize, tx: &mut Tx) {
        // Every colour glides to the new skin.
        tx.color_motion(rune::Spring::new(0.45, 0.0));
        self.skin = (self.skin as isize + d).rem_euclid(SKINS.len() as isize) as usize;
        self.say(format!("Skin: {}", SKINS[self.skin].name));
        self.save();
    }

    pub fn cycle_viz(&mut self, d: isize) {
        self.viz = (self.viz as isize + d).rem_euclid(VIZ_MODES.len() as isize) as usize;
        self.save();
    }

    // ── Frame loop ───────────────────────────────────────────────────────────

    /// Tags that arrived, the end of a track, and everything drawn from live values. Returns
    /// true when the view must run again.
    pub fn poll(&mut self, t: f64, tx: &mut Tx) -> bool {
        let dt = if self.last_t == 0.0 {
            1.0 / 60.0
        } else {
            (t - self.last_t) as f32
        };
        self.last_t = t;
        let mut changed = false;

        if let Some(r) = &self.reader {
            for (path, meta) in r.rx.try_iter() {
                for track in self
                    .tracks
                    .iter_mut()
                    .filter(|tr| tr.path == path && !tr.read)
                {
                    track.meta = meta.clone();
                    track.read = true;
                }
                changed = true;
            }
        }

        let commands: Vec<Command> = self
            .bridge
            .as_ref()
            .map(|b| b.commands().collect())
            .unwrap_or_default();
        for cmd in commands {
            match cmd {
                Command::Toggle => self.toggle_play(),
                Command::Next => self.next(),
                Command::Prev => self.prev(),
            }
            changed = true;
        }

        if let Some(y) = &mut self.youtube {
            changed |= y.poll();
        }
        changed |= self.update_art();

        if self.play == Play::Playing && self.engine.as_ref().is_some_and(|e| e.finished()) {
            match (self.repeat, self.current) {
                (Repeat::One, Some(i)) => self.play_index(i),
                _ => self.next_local(),
            }
            changed = true;
        }

        let moving = if self.demo {
            self.analyzer.levels = Analyzer::demo(t as f32);
            false
        } else if self.source == Source::YouTube {
            let spectrum = self.youtube.as_mut().and_then(|y| y.take_spectrum());
            self.analyzer.update_external(spectrum, dt)
        } else {
            self.analyzer.update(&self.scope, dt)
        };
        self.publish();
        self.notify();
        if self.now_play() == Play::Playing || moving {
            // About 30 frames a second: bars, the clock and the visualizer all follow this.
            tx.wake_after(Duration::from_millis(33));
        } else if self.bridge.is_some() {
            // The notch app's once-a-second heartbeat.
            tx.wake_after(Duration::from_secs(1));
        }
        changed
    }

    /// Sends the notch app a status: 30 a second while playing, once a second otherwise, and at
    /// once when the state or track changes.
    fn notify(&mut self) {
        let Some(bridge) = &self.bridge else { return };
        let now = self.now();
        let key = format!("{:?}|{}|{}", now.play, now.title, now.artist);
        let every = if now.play == Play::Playing {
            1.0 / 30.0
        } else {
            1.0
        };
        if key == self.sent_key
            && self
                .sent_at
                .is_some_and(|at| at.elapsed().as_secs_f64() < every)
        {
            return;
        }
        bridge.send(&self.status());
        self.sent_at = Some(std::time::Instant::now());
        self.sent_key = key;
    }

    /// What the notch app shows: the source's track, where it is, the spectrum and the skin.
    pub fn status(&self) -> serde_json::Value {
        let k = skin(self.skin);
        let now = self.now();
        let round = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
        serde_json::json!({
            "v": 1,
            "state": match now.play { Play::Playing => "playing", Play::Paused => "paused", Play::Stopped => "stopped" },
            "title": now.title,
            "artist": now.artist,
            "pos": (self.position() * 10.0).round() / 10.0,
            "dur": self.duration().unwrap_or(0.0),
            "bands": self.analyzer.levels.bands.iter().map(|&b| round(b)).collect::<Vec<_>>(),
            "accent": k.accent,
            "accent2": k.accent2,
            "bars": k.bars,
        })
    }

    /// Pushes levels, progress and the clock into the live values.
    pub fn publish(&self) {
        if self.now_play() == Play::Playing {
            let [_, y] = self.tick.get();
            self.tick.set([0.0, if y == 0.0 { 0.001 } else { 0.0 }]);
        }
        let l = &self.analyzer.levels;
        for i in 0..BANDS {
            let h = (l.bands[i] * BAR_H).round().clamp(0.0, BAR_H);
            self.bars[i].set([0.0, BAR_H - h, BAR_W, h]);
            let p = (l.peaks[i] * BAR_H).round().clamp(2.0, BAR_H);
            self.peaks[i].set([0.0, BAR_H - p, BAR_W, 2.0]);
        }
        let pos = self.position();
        let frac = self
            .duration()
            .filter(|d| *d > 0.0)
            .map_or(0.0, |d| (pos / d).clamp(0.0, 1.0)) as f32;
        let w = frac * SEEK_W;
        self.progress.set([0.0, 0.0, w, 4.0]);
        self.knob.set([w - 7.0, 0.0]);
        let shown = match (self.remaining, self.duration()) {
            (true, Some(d)) => format!("-{}", library::clock(d - pos)),
            _ => library::clock(pos),
        };
        self.clock.set(shown);
    }
}
