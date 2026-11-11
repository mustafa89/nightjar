//! Turns the scope's latest samples into what the screen draws: 20 deck bands with falling
//! peaks, 64 finer bins and a short waveform for the visualizer, bass/mid/treble levels and a
//! beat pulse. Runs on the main thread from `poll`, about 60 times a second.

use crate::audio::Scope;
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

pub const WINDOW: usize = 2048;
pub const BANDS: usize = 20;
pub const BINS: usize = 64;
pub const WAVE: usize = 64;

/// Lowest and highest frequency the bands cover.
const LO_HZ: f32 = 40.0;
const HI_HZ: f32 = 16_000.0;
/// Magnitudes map from this many decibels below full scale up to the top of the range.
const FLOOR_DB: f32 = -72.0;
const CEIL_DB: f32 = -12.0;
/// Bars fall at this many full heights per second; peaks hold, then fall slower.
const FALL: f32 = 1.6;
const PEAK_HOLD: f32 = 0.45;
const PEAK_FALL: f32 = 0.7;

#[derive(Clone, Debug)]
pub struct Levels {
    pub bands: [f32; BANDS],
    pub peaks: [f32; BANDS],
    pub bins: [f32; BINS],
    pub wave: [f32; WAVE],
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// 1 on a detected beat, decaying to 0.
    pub beat: f32,
}

impl Default for Levels {
    fn default() -> Levels {
        Levels {
            bands: [0.0; BANDS],
            peaks: [0.0; BANDS],
            bins: [0.0; BINS],
            wave: [0.0; WAVE],
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            beat: 0.0,
        }
    }
}

pub struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    hann: Vec<f32>,
    samples: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    peak_age: [f32; BANDS],
    last_written: usize,
    bass_avg: f32,
    pub levels: Levels,
}

impl Analyzer {
    pub fn new() -> Analyzer {
        let fft = FftPlanner::new().plan_fft_forward(WINDOW);
        let scratch = vec![Complex32::default(); fft.get_inplace_scratch_len()];
        let hann = (0..WINDOW)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (WINDOW - 1) as f32).cos())
            .collect();
        Analyzer {
            fft,
            hann,
            samples: vec![0.0; WINDOW],
            spectrum: vec![Complex32::default(); WINDOW],
            scratch,
            peak_age: [0.0; BANDS],
            last_written: 0,
            bass_avg: 0.0,
            levels: Levels::default(),
        }
    }

    /// Advances by `dt` seconds. With no new samples (paused, stopped) everything falls to rest.
    /// Returns false once everything is at rest, so the caller can stop asking for frames.
    pub fn update(&mut self, scope: &Scope, dt: f32) -> bool {
        let dt = dt.clamp(0.0, 0.1);
        let written = scope.written();
        let fresh = written != self.last_written;
        self.last_written = written;
        let mut target_bins = [0.0f32; BINS];
        let mut target_bands = [0.0f32; BANDS];
        if fresh {
            scope.latest(&mut self.samples);
            self.transform(scope.sample_rate(), &mut target_bands, &mut target_bins);
            let step = WINDOW / WAVE;
            for (k, w) in self.levels.wave.iter_mut().enumerate() {
                *w = self.samples[WINDOW - WAVE * step + k * step];
            }
        } else {
            for w in &mut self.levels.wave {
                *w *= (1.0 - dt * 8.0).max(0.0);
            }
        }
        let l = &mut self.levels;
        for (i, target) in target_bands.into_iter().enumerate() {
            l.bands[i] = target.max(l.bands[i] - FALL * dt);
            if l.bands[i] >= l.peaks[i] {
                l.peaks[i] = l.bands[i];
                self.peak_age[i] = 0.0;
            } else {
                self.peak_age[i] += dt;
                if self.peak_age[i] > PEAK_HOLD {
                    l.peaks[i] = (l.peaks[i] - PEAK_FALL * dt).max(l.bands[i]);
                }
            }
        }
        for (bin, target) in l.bins.iter_mut().zip(target_bins) {
            *bin = target.max(*bin - FALL * 1.4 * dt);
        }
        let mean =
            |r: std::ops::Range<usize>| r.clone().map(|i| l.bins[i]).sum::<f32>() / r.len() as f32;
        l.bass = mean(0..12);
        l.mid = mean(12..40);
        l.treble = mean(40..BINS);
        // A beat: bass well above its recent average.
        self.bass_avg += (l.bass - self.bass_avg) * (dt * 2.5).min(1.0);
        if fresh && l.bass > 0.25 && l.bass > self.bass_avg * 1.35 && l.beat < 0.4 {
            l.beat = 1.0;
        } else {
            l.beat = (l.beat - dt * 3.0).max(0.0);
        }
        fresh || l.peaks.iter().any(|&p| p > 0.001) || l.beat > 0.0
    }

    fn transform(&mut self, rate: f32, bands: &mut [f32; BANDS], bins: &mut [f32; BINS]) {
        for (k, c) in self.spectrum.iter_mut().enumerate() {
            *c = Complex32::new(self.samples[k] * self.hann[k], 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.spectrum, &mut self.scratch);
        let hz_per_bin = rate / WINDOW as f32;
        // Amplitude of a full-scale sine through a Hann window is N/4.
        let norm = 4.0 / WINDOW as f32;
        let level = |lo: f32, hi: f32, spectrum: &[Complex32]| {
            let a = ((lo / hz_per_bin) as usize).max(1);
            let b = ((hi / hz_per_bin).ceil() as usize).clamp(a + 1, WINDOW / 2);
            let peak = spectrum[a..b]
                .iter()
                .map(|c| c.norm())
                .fold(0.0f32, f32::max)
                * norm;
            let db = 20.0 * peak.max(1e-9).log10();
            ((db - FLOOR_DB) / (CEIL_DB - FLOOR_DB)).clamp(0.0, 1.0)
        };
        let edge = |i: usize, n: usize| LO_HZ * (HI_HZ / LO_HZ).powf(i as f32 / n as f32);
        for (i, b) in bands.iter_mut().enumerate() {
            *b = level(edge(i, BANDS), edge(i + 1, BANDS), &self.spectrum);
        }
        for (i, b) in bins.iter_mut().enumerate() {
            *b = level(edge(i, BINS), edge(i + 1, BINS), &self.spectrum);
        }
    }

    /// Fake, musical-looking levels for headless snapshots and tests.
    pub fn demo(t: f32) -> Levels {
        let mut l = Levels::default();
        let shape = |x: f32| (1.0 - x * 0.55) * (0.65 + 0.35 * (x * 9.0 + t * 3.0).sin().abs());
        for i in 0..BANDS {
            let x = i as f32 / BANDS as f32;
            l.bands[i] = shape(x).clamp(0.05, 1.0);
            l.peaks[i] = (l.bands[i] + 0.08).min(1.0);
        }
        for i in 0..BINS {
            l.bins[i] = shape(i as f32 / BINS as f32).clamp(0.0, 1.0);
        }
        for (k, w) in l.wave.iter_mut().enumerate() {
            let x = k as f32 / WAVE as f32;
            *w = 0.6 * (x * 12.0 + t).sin() * (x * 3.1).sin();
        }
        (l.bass, l.mid, l.treble, l.beat) = (0.85, 0.6, 0.4, 0.7);
        l
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_440_hz_tone_lights_the_band_that_holds_440_hz() {
        let scope = Scope::new();
        let rate = scope.sample_rate();
        let mut a = Analyzer::new();
        // Enough samples to fill the window, then one update.
        let mut s = vec![0.0f32; WINDOW * 2];
        for (n, v) in s.iter_mut().enumerate() {
            *v = 0.5 * (std::f32::consts::TAU * 440.0 * n as f32 / rate).sin();
        }
        for v in s {
            scope_push(&scope, v);
        }
        a.update(&scope, 1.0 / 60.0);
        let loudest = (0..BANDS)
            .max_by(|&x, &y| a.levels.bands[x].total_cmp(&a.levels.bands[y]))
            .unwrap();
        let lo = LO_HZ * (HI_HZ / LO_HZ).powf(loudest as f32 / BANDS as f32);
        let hi = LO_HZ * (HI_HZ / LO_HZ).powf((loudest + 1) as f32 / BANDS as f32);
        assert!(
            lo <= 440.0 && 440.0 < hi,
            "loudest band {loudest} covers {lo:.0}–{hi:.0} Hz"
        );
        assert!(a.levels.bands[loudest] > 0.8);
    }

    #[test]
    fn silence_after_sound_falls_to_rest() {
        let scope = Scope::new();
        let mut a = Analyzer::new();
        a.levels = Analyzer::demo(0.0);
        let mut moving = true;
        for _ in 0..240 {
            moving = a.update(&scope, 1.0 / 60.0);
        }
        assert!(!moving, "at rest after four silent seconds");
        assert!(a.levels.bands.iter().all(|&b| b == 0.0));
    }

    fn scope_push(scope: &Scope, v: f32) {
        crate::audio::test_push(scope, v);
    }
}
