//! Playback through rodio, with a tap that copies every decoded frame (mixed to mono) into a
//! lock-free ring the analyser reads. The device opens on the first play, so headless runs and
//! tests never touch audio hardware.

use rodio::source::SeekError;
use rodio::{Decoder, MixerDeviceSink, Player, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

/// Ring length in mono samples: a power of two, well over the analyser's 2048-sample window.
const RING: usize = 1 << 14;

/// The most recent mono samples the speakers were handed. One writer (the audio thread), any
/// number of readers; a reader may see a sample torn across a wrap, which only nudges a bar.
pub struct Scope {
    buf: Box<[AtomicU32]>,
    head: AtomicUsize,
    rate: AtomicU32,
}

impl Scope {
    pub fn new() -> Arc<Scope> {
        Arc::new(Scope {
            buf: (0..RING).map(|_| AtomicU32::new(0)).collect(),
            head: AtomicUsize::new(0),
            rate: AtomicU32::new(44_100),
        })
    }

    fn push(&self, v: f32) {
        let i = self.head.load(Ordering::Relaxed);
        self.buf[i & (RING - 1)].store(v.to_bits(), Ordering::Relaxed);
        self.head.store(i.wrapping_add(1), Ordering::Release);
    }

    /// Total samples written so far: unchanged between two reads means nothing is playing.
    pub fn written(&self) -> usize {
        self.head.load(Ordering::Acquire)
    }

    pub fn sample_rate(&self) -> f32 {
        self.rate.load(Ordering::Relaxed) as f32
    }

    /// Copies the latest `out.len()` samples, oldest first.
    pub fn latest(&self, out: &mut [f32]) {
        let head = self.written();
        let n = out.len().min(RING);
        for (k, o) in out.iter_mut().enumerate().take(n) {
            let i = head.wrapping_sub(n).wrapping_add(k);
            *o = f32::from_bits(self.buf[i & (RING - 1)].load(Ordering::Relaxed));
        }
    }
}

/// A source that passes its samples through unchanged and mixes each frame into the scope.
struct Tap<S> {
    inner: S,
    scope: Arc<Scope>,
    channels: u16,
    at: u16,
    sum: f32,
}

impl<S: Source> Iterator for Tap<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.inner.next()?;
        self.sum += s;
        self.at += 1;
        if self.at >= self.channels {
            self.scope.push(self.sum / self.channels as f32);
            self.at = 0;
            self.sum = 0.0;
        }
        Some(s)
    }
}

impl<S: Source> Source for Tap<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    // The default says "not supported": delegate, or seeking silently stops working.
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.at = 0;
        self.sum = 0.0;
        self.inner.try_seek(pos)
    }
}

/// What the decoder reports about the stream it opened.
#[derive(Clone, Debug, Default)]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: Option<f64>,
}

pub struct Engine {
    // Dropping the sink stops all sound; it must outlive the player.
    _sink: MixerDeviceSink,
    player: Player,
}

impl Engine {
    pub fn open() -> Result<Engine, String> {
        let mut sink = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| format!("no audio output: {e}"))?;
        sink.log_on_drop(false);
        let player = Player::connect_new(sink.mixer());
        Ok(Engine {
            _sink: sink,
            player,
        })
    }

    /// Replaces whatever is loaded with `path` and starts it.
    pub fn load(&self, path: &Path, scope: &Arc<Scope>, volume: f32) -> Result<StreamInfo, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut builder = Decoder::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(len)
            .with_seekable(true);
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            builder = builder.with_hint(ext);
        }
        let decoder = builder
            .build()
            .map_err(|e| format!("can't decode {}: {e}", file_name(path)))?;
        let info = StreamInfo {
            sample_rate: decoder.sample_rate().get(),
            channels: decoder.channels().get(),
            duration: decoder.total_duration().map(|d| d.as_secs_f64()),
        };
        scope.rate.store(info.sample_rate, Ordering::Relaxed);
        let tap = Tap {
            channels: info.channels.max(1),
            inner: decoder,
            scope: scope.clone(),
            at: 0,
            sum: 0.0,
        };
        self.player.clear();
        self.player.set_volume(volume);
        self.player.append(tap);
        self.player.play();
        Ok(info)
    }

    pub fn play(&self) {
        self.player.play();
    }
    pub fn pause(&self) {
        self.player.pause();
    }
    pub fn stop(&self) {
        self.player.clear();
    }
    pub fn set_volume(&self, v: f32) {
        self.player.set_volume(v);
    }
    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        self.player
            .try_seek(Duration::from_secs_f64(seconds.max(0.0)))
            .map_err(|e| format!("seek: {e}"))
    }
    pub fn position(&self) -> f64 {
        self.player.get_pos().as_secs_f64()
    }
    /// True once the loaded track has played to its end (or nothing is loaded).
    pub fn finished(&self) -> bool {
        self.player.empty()
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
pub fn test_push(scope: &Scope, v: f32) {
    scope.push(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit mono WAV of a 440 Hz tone.
    fn tone(path: &Path, seconds: f32) {
        let rate = 44_100u32;
        let n = (rate as f32 * seconds) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 2).to_le_bytes());
        for i in 0..n {
            let v = (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin() * 0.5;
            b.extend_from_slice(&((v * i16::MAX as f32) as i16).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    /// Needs an audio output device: `cargo test -- --ignored`. Plays at volume 0.
    #[test]
    #[ignore]
    fn a_file_plays_through_the_tap_and_seeks() {
        let path = std::env::temp_dir().join("nightjar-tone.wav");
        tone(&path, 3.0);
        let engine = Engine::open().unwrap();
        let scope = Scope::new();
        let info = engine.load(&path, &scope, 0.0).unwrap();
        assert_eq!((info.sample_rate, info.channels), (44_100, 1));
        assert!((info.duration.unwrap() - 3.0).abs() < 0.05);
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            scope.written() > 4_000,
            "the tap saw {} samples",
            scope.written()
        );
        assert!(engine.position() > 0.2);
        engine.seek(2.0).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            engine.position() >= 2.0,
            "position {} after seeking to 2 s",
            engine.position()
        );
        std::thread::sleep(Duration::from_millis(1200));
        assert!(engine.finished(), "done after the end");
    }
}
