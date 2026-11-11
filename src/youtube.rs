//! The YouTube Music source: the Swift backend (`backend/`, linked into this binary by
//! build.rs) plays music.youtube.com in a WKWebView window inside Nightjar and taps its audio.
//! Rust talks to it through four C calls on the main thread:
//!
//! - `nj_yt_start`: create the player (window hidden) with the page script; it wakes Rune after
//!   each new status.
//! - `nj_yt_show`: bring its window forward.
//! - `nj_yt_command`: `toggle`, `next`, `prev`, `seek <s>`, `volume <0..1>`, `queue <n>`.
//! - `nj_yt_status`: the newest status JSON (state, title, artist, album, art, pos, dur, volume,
//!   64 spectrum bins and a 64-point waveform while playing, signedIn, queue, tap).

use serde::Deserialize;
use std::ffi::{CString, c_char, c_void};
use std::time::{Duration, Instant};

/// The page script injected into music.youtube.com.
const PAGE_SCRIPT: &str = include_str!("../backend/inject.js");
/// No status for this long means the page stopped answering.
const TIMEOUT: Duration = Duration::from_secs(3);

unsafe extern "C" {
    fn nj_yt_start(
        wake: extern "C" fn(*mut c_void),
        ctx: *mut c_void,
        script: *const c_char,
        url: *const c_char,
        silent: bool,
        tap: bool,
    );
    fn nj_yt_show();
    fn nj_yt_command(name: *const c_char, arg: f64, has_arg: bool);
    fn nj_yt_status(after: u64, seq: *mut u64, buf: *mut u8, cap: usize) -> usize;
}

extern "C" fn wake(ctx: *mut c_void) {
    // SAFETY: ctx is the boxed waker YouTube::start keeps alive for the process's life.
    let waker = unsafe { &*(ctx as *const rune::Waker) };
    waker();
}

/// One song after the current one in YouTube Music's queue.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct QueueItem {
    pub title: String,
    pub artist: String,
    pub dur: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Status {
    pub state: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Artwork URL from the page's media session (largest size).
    pub art: String,
    pub pos: f64,
    pub dur: f64,
    pub volume: f32,
    pub bins: Vec<f32>,
    pub wave: Vec<f32>,
    pub signed_in: bool,
    /// What plays after the current song (up to 30), read from the page's queue.
    pub queue: Vec<QueueItem>,
    pub tap: String,
}

pub struct YouTube {
    /// None: an offline link for tests and headless scenes (no player).
    native: Option<Native>,
    pub last: Option<Status>,
    last_at: Option<Instant>,
    /// Spectrum not yet handed to the analyser.
    spectrum: Option<(Vec<f32>, Vec<f32>)>,
}

struct Native {
    seq: u64,
    buf: Vec<u8>,
    /// Kept alive: the Swift side calls back into it after every status.
    _waker: Box<rune::Waker>,
}

/// How the player starts: tests ask for a silent one (see YouTubePlayer.init) and a page.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub silent: bool,
    pub tap: bool,
    pub url: Option<String>,
}

impl YouTube {
    /// Creates the player (its window stays hidden until `show`). Once per process.
    pub fn start(waker: rune::Waker, options: &Options) -> YouTube {
        let waker = Box::new(waker);
        let script = CString::new(PAGE_SCRIPT).expect("page script has no NUL");
        let url = options.url.as_deref().and_then(|u| CString::new(u).ok());
        // SAFETY: all pointers outlive the call; the Swift side copies the strings, and the
        // boxed waker lives in the returned value for as long as the player runs.
        unsafe {
            nj_yt_start(
                wake,
                &*waker as *const rune::Waker as *mut c_void,
                script.as_ptr(),
                url.as_ref().map_or(std::ptr::null(), |u| u.as_ptr()),
                options.silent,
                options.tap,
            );
        }
        YouTube {
            native: Some(Native {
                seq: 0,
                buf: vec![0; 256 * 1024],
                _waker: waker,
            }),
            last: None,
            last_at: None,
            spectrum: None,
        }
    }

    /// A link with no player that reports `status` as just received: headless scenes, tests.
    pub fn offline(status: Status) -> YouTube {
        YouTube {
            native: None,
            last: Some(status),
            last_at: Some(Instant::now()),
            spectrum: None,
        }
    }

    /// Takes the newest status. Returns true when what the view shows changed (state, track,
    /// sign-in or tap), not for position or spectrum alone.
    pub fn poll(&mut self) -> bool {
        let was_connected = self.connected();
        let mut changed = false;
        if let Some(status) = self.read() {
            let prev = self.last.as_ref();
            changed |= prev.is_none_or(|p| {
                p.state != status.state
                    || p.title != status.title
                    || p.artist != status.artist
                    || p.signed_in != status.signed_in
                    || p.tap != status.tap
                    || p.queue != status.queue
            });
            if !status.bins.is_empty() {
                self.spectrum = Some((status.bins.clone(), status.wave.clone()));
            }
            self.last = Some(status);
            self.last_at = Some(Instant::now());
        }
        changed || was_connected != self.connected()
    }

    /// The newest status from the player, if there's one it hasn't read yet.
    fn read(&mut self) -> Option<Status> {
        let native = self.native.as_mut()?;
        let mut seq = native.seq;
        // SAFETY: buf is valid for its length; the call writes at most that many bytes.
        let n = unsafe {
            nj_yt_status(
                native.seq,
                &mut seq,
                native.buf.as_mut_ptr(),
                native.buf.len(),
            )
        };
        if n == 0 {
            return None;
        }
        native.seq = seq;
        serde_json::from_slice(&native.buf[..n]).ok()
    }

    pub fn connected(&self) -> bool {
        self.last_at.is_some_and(|t| t.elapsed() < TIMEOUT)
    }

    /// The latest status while the player answers.
    pub fn status(&self) -> Option<&Status> {
        self.last.as_ref().filter(|_| self.connected())
    }

    pub fn take_spectrum(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        self.spectrum.take()
    }

    /// `name` or `name value`: toggle, next, prev, seek 42.5, volume 0.6, queue 3.
    pub fn send(&self, command: &str) {
        if self.native.is_none() {
            return;
        }
        let mut words = command.split_whitespace();
        let Some(name) = words.next().and_then(|w| CString::new(w).ok()) else {
            return;
        };
        let arg = words.next().and_then(|w| w.parse::<f64>().ok());
        // SAFETY: name outlives the call; the Swift side copies it.
        unsafe { nj_yt_command(name.as_ptr(), arg.unwrap_or(0.0), arg.is_some()) };
    }

    /// Brings the YouTube Music window forward.
    pub fn show(&self) {
        if self.native.is_some() {
            // SAFETY: no arguments; main thread, as every handler runs.
            unsafe { nj_yt_show() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_status_parses_with_missing_fields() {
        let s: Status = serde_json::from_str(r#"{"state":"playing","title":"Song","pos":12.5,"dur":200,"bins":[0.5,0.25],"signedIn":true,"extra":1}"#).unwrap();
        assert_eq!(
            (s.state.as_str(), s.title.as_str(), s.pos, s.signed_in),
            ("playing", "Song", 12.5, true)
        );
        assert_eq!(s.bins, vec![0.5, 0.25]);
        assert!(s.artist.is_empty() && s.wave.is_empty());
    }
}
