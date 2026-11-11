//! App state and every player action. The view reads it; handlers and `poll` change it.

use crate::analysis::{Analyzer, BANDS, Levels};
use crate::audio::{Engine, Scope, StreamInfo};
use crate::library::{self, MetaReader, Track};
use crate::settings::{self, Settings};
use crate::skin::SKINS;
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
    last_t: f64,
    seed: u64,

    // Values the screen draws without re-running the view.
    pub bars: Vec<Live<[f32; 4]>>,
    pub peaks: Vec<Live<[f32; 4]>>,
    pub progress: Live<[f32; 4]>,
    pub knob: Live<[f32; 2]>,
    pub clock: LiveText,
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
            message: None,
            drop_hover: false,
            scrub: None,
            remaining: false,
            demo: false,
            scope: Scope::new(),
            analyzer: Analyzer::new(),
            engine: None,
            reader: None,
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
        st
    }

    /// Window start: the tag reader, and tags for the restored playlist. Paths on the command
    /// line join the playlist and the first of them plays.
    pub fn start(&mut self, waker: rune::Waker) {
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

    /// Length of the loaded track: the decoder's figure, else the tag reader's.
    pub fn duration(&self) -> Option<f64> {
        self.info
            .as_ref()
            .and_then(|i| i.duration)
            .or_else(|| self.current_track().and_then(|t| t.meta.duration))
    }

    pub fn position(&self) -> f64 {
        if let (Some(f), Some(d)) = (self.scrub, self.duration()) {
            return f as f64 * d;
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
                self.stop();
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
        self.save();
    }

    pub fn clear(&mut self) {
        self.stop();
        self.tracks.clear();
        self.current = None;
        self.selected = None;
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

    pub fn stop(&mut self) {
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

    /// The track after `current`, or None at the end of a non-repeating list.
    fn following(&mut self, step: isize) -> Option<usize> {
        let n = self.tracks.len();
        if n == 0 {
            return None;
        }
        if self.shuffle && n > 1 {
            let cur = self.current.unwrap_or(0);
            let mut r = self.random(n - 1);
            if r >= cur {
                r += 1;
            }
            return Some(r);
        }
        let Some(cur) = self.current else {
            return Some(0);
        };
        let next = cur as isize + step;
        if (0..n as isize).contains(&next) {
            Some(next as usize)
        } else if self.repeat == Repeat::All {
            Some(next.rem_euclid(n as isize) as usize)
        } else {
            None
        }
    }

    pub fn next(&mut self) {
        match self.following(1) {
            Some(i) => self.play_index(i),
            None => self.stop(),
        }
    }

    pub fn prev(&mut self) {
        if self.position() > 3.0 {
            self.seek_to(0.0);
            return;
        }
        if let Some(i) = self.following(-1) {
            self.play_index(i);
        }
    }

    pub fn seek_to(&mut self, seconds: f64) {
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
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
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

        if self.play == Play::Playing && self.engine.as_ref().is_some_and(|e| e.finished()) {
            match (self.repeat, self.current) {
                (Repeat::One, Some(i)) => self.play_index(i),
                _ => self.next(),
            }
            changed = true;
        }

        let moving = if self.demo {
            self.analyzer.levels = Analyzer::demo(t as f32);
            false
        } else {
            self.analyzer.update(&self.scope, dt)
        };
        self.publish();
        if self.play == Play::Playing || moving {
            tx.wake_after(Duration::from_millis(16));
        }
        changed
    }

    /// Pushes levels, progress and the clock into the live values.
    pub fn publish(&self) {
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
