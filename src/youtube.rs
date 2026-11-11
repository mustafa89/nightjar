//! The YouTube Music source: talks to the Nightjar YouTube helper app (`ytmusic/`), which plays
//! music.youtube.com in a web view and taps its audio. UDP on 127.0.0.1 only:
//!
//! - helper → Nightjar, port 47813: JSON status (state, title, artist, album, pos, dur, volume,
//!   64 spectrum bins and a 64-point waveform while playing, signedIn, tap).
//! - Nightjar → helper, port 47814: `toggle`, `next`, `prev`, `show`, `seek <s>`, `volume <0..1>`.

use serde::Deserialize;
use std::net::UdpSocket;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

pub const STATUS_PORT: u16 = 47813;
pub const COMMAND_PORT: u16 = 47814;
pub const BUNDLE_ID: &str = "dev.mustafa.nightjar.youtube";
/// No status for this long means the helper quit.
const TIMEOUT: Duration = Duration::from_secs(3);

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
    pub tap: String,
}

pub struct YouTube {
    out: Option<UdpSocket>,
    rx: Receiver<Status>,
    pub last: Option<Status>,
    last_at: Option<Instant>,
    /// Spectrum not yet handed to the analyser.
    spectrum: Option<(Vec<f32>, Vec<f32>)>,
}

impl YouTube {
    /// Listens for the helper's status, unless another Nightjar already does.
    pub fn start(waker: Option<rune::Waker>) -> YouTube {
        let out = UdpSocket::bind(("127.0.0.1", 0))
            .ok()
            .filter(|s| s.connect(("127.0.0.1", COMMAND_PORT)).is_ok());
        let (tx, rx) = channel();
        if let Ok(sock) = UdpSocket::bind(("127.0.0.1", STATUS_PORT)) {
            let _ = std::thread::Builder::new()
                .name("nightjar-youtube".into())
                .spawn(move || {
                    let mut buf = vec![0u8; 16 * 1024];
                    while let Ok((n, _)) = sock.recv_from(&mut buf) {
                        let Ok(status) = serde_json::from_slice::<Status>(&buf[..n]) else {
                            continue;
                        };
                        if tx.send(status).is_err() {
                            break;
                        }
                        if let Some(w) = &waker {
                            w();
                        }
                    }
                });
        }
        YouTube {
            out,
            rx,
            last: None,
            last_at: None,
            spectrum: None,
        }
    }

    /// Takes the newest status. Returns true when what the view shows changed (state, track,
    /// sign-in or tap), not for position or spectrum alone.
    pub fn poll(&mut self) -> bool {
        let was_connected = self.connected();
        let mut changed = false;
        for status in self.rx.try_iter() {
            let prev = self.last.as_ref();
            changed |= prev.is_none_or(|p| {
                p.state != status.state
                    || p.title != status.title
                    || p.artist != status.artist
                    || p.signed_in != status.signed_in
                    || p.tap != status.tap
            });
            if !status.bins.is_empty() {
                self.spectrum = Some((status.bins.clone(), status.wave.clone()));
            }
            self.last = Some(status);
            self.last_at = Some(Instant::now());
        }
        changed || was_connected != self.connected()
    }

    pub fn connected(&self) -> bool {
        self.last_at.is_some_and(|t| t.elapsed() < TIMEOUT)
    }

    /// The latest status while the helper is running.
    pub fn status(&self) -> Option<&Status> {
        self.last.as_ref().filter(|_| self.connected())
    }

    pub fn take_spectrum(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        self.spectrum.take()
    }

    pub fn send(&self, command: &str) {
        if let Some(sock) = &self.out {
            let _ = sock.send(command.as_bytes());
        }
    }

    /// Opens the helper (or brings it forward). Err when it isn't installed.
    pub fn launch(&self) -> Result<(), String> {
        if self.connected() {
            self.send("show");
            return Ok(());
        }
        let ok = std::process::Command::new("/usr/bin/open")
            .args(["-b", BUNDLE_ID])
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            Ok(())
        } else {
            Err("Nightjar YouTube isn't installed: run ytmusic/build.sh".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_helper_status_parses_with_missing_fields() {
        let s: Status = serde_json::from_str(r#"{"state":"playing","title":"Song","pos":12.5,"dur":200,"bins":[0.5,0.25],"signedIn":true,"extra":1}"#).unwrap();
        assert_eq!(
            (s.state.as_str(), s.title.as_str(), s.pos, s.signed_in),
            ("playing", "Song", 12.5, true)
        );
        assert_eq!(s.bins, vec![0.5, 0.25]);
        assert!(s.artist.is_empty() && s.wave.is_empty());
    }
}
