//! Spectrum analyzer frames and the footstep transient detector.
//!
//! TODO(owner): replace `Synth` with WASAPI loopback capture of the default
//! render endpoint (IAudioClient in AUDCLNT_STREAMFLAGS_LOOPBACK mode) + a
//! 4096-point Hann-windowed FFT folded into `BIN_COUNT` log-spaced bins.
//! Everything downstream (band energy, transient detection, UI) stays as is.

use crate::dsp::{log_space, MAX_HZ, MIN_HZ};
use serde::Serialize;

pub const BIN_COUNT: usize = 96;
/// The band the UI highlights and the detector watches.
pub const FOOTSTEP_LO_HZ: f64 = 2000.0;
pub const FOOTSTEP_HI_HZ: f64 = 4000.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpectrumFrame {
    pub hz: Vec<f64>,
    pub db: Vec<f64>,
    pub footstep_db: f64,
    pub transient: bool,
}

/// Power-average level of the bins between `lo` and `hi`, in dB.
pub fn band_energy_db(hz: &[f64], db: &[f64], lo: f64, hi: f64) -> f64 {
    let (mut sum, mut n) = (0.0, 0usize);
    for (&f, &d) in hz.iter().zip(db) {
        if f >= lo && f <= hi {
            sum += 10f64.powf(d / 10.0);
            n += 1;
        }
    }
    if n == 0 {
        -120.0
    } else {
        10.0 * (sum / n as f64).log10()
    }
}

/// Fires when the footstep band jumps well above its slow running average.
/// Uses a fast/slow envelope pair with a refractory hold so one step is one event.
#[derive(Debug, Clone)]
pub struct TransientDetector {
    slow: f64,
    fast: f64,
    pub threshold_db: f64,
    hold_frames: u32,
    hold: u32,
    primed: bool,
}

impl TransientDetector {
    pub fn new(threshold_db: f64) -> Self {
        TransientDetector { slow: -60.0, fast: -60.0, threshold_db, hold_frames: 6, hold: 0, primed: false }
    }

    pub fn push(&mut self, level_db: f64) -> bool {
        if !self.primed {
            self.slow = level_db;
            self.fast = level_db;
            self.primed = true;
            return false;
        }
        self.fast += 0.6 * (level_db - self.fast);
        self.slow += 0.04 * (level_db - self.slow);
        if self.hold > 0 {
            self.hold -= 1;
            return false;
        }
        if self.fast - self.slow > self.threshold_db {
            self.hold = self.hold_frames;
            return true;
        }
        false
    }
}

/// Deterministic sample-signal generator: pink-ish game ambience, periodic
/// footstep transients in 2-4 kHz and occasional low-frequency explosions.
pub struct Synth {
    rng: u64,
    t_ms: f64,
    hz: Vec<f64>,
    smooth: Vec<f64>,
    detector: TransientDetector,
}

impl Synth {
    pub fn new(seed: u64) -> Self {
        let hz = log_space(MIN_HZ, MAX_HZ, BIN_COUNT);
        Synth { rng: seed | 1, t_ms: 0.0, smooth: vec![-70.0; hz.len()], hz, detector: TransientDetector::new(6.0) }
    }

    fn noise(&mut self) -> f64 {
        // xorshift64*, mapped to [-1, 1)
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let v = self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        v as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }

    pub fn next(&mut self, dt_ms: f64) -> SpectrumFrame {
        self.t_ms += dt_ms.clamp(1.0, 200.0);
        let t = self.t_ms / 1000.0;
        // Footsteps at ~2.1 steps/s, decaying envelope per step.
        let step_phase = (t * 2.1).fract();
        let step = (-step_phase * 14.0).exp();
        // An explosion every ~7 s.
        let boom_phase = (t / 7.0).fract();
        let boom = if boom_phase < 0.12 { (-boom_phase * 30.0).exp() } else { 0.0 };
        let mut db = Vec::with_capacity(self.hz.len());
        for i in 0..self.hz.len() {
            let f = self.hz[i];
            let oct = (f / 1000.0).log2();
            let mut level = -38.0 - 3.0 * oct - 0.6 * oct * oct + 2.5 * self.noise();
            let step_w = (-((f / 2900.0).log2() / 0.55).powi(2)).exp();
            level += 22.0 * step * step_w;
            let boom_w = (-((f / 90.0).log2() / 1.2).powi(2)).exp();
            level += 26.0 * boom * boom_w;
            let a = if level > self.smooth[i] { 0.7 } else { 0.22 };
            self.smooth[i] += a * (level - self.smooth[i]);
            db.push((self.smooth[i] * 10.0).round() / 10.0);
        }
        let footstep_db = band_energy_db(&self.hz, &db, FOOTSTEP_LO_HZ, FOOTSTEP_HI_HZ);
        let transient = self.detector.push(footstep_db);
        SpectrumFrame { hz: self.hz.clone(), db, footstep_db, transient }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_energy_of_constant_is_constant() {
        let hz = [1000.0, 2000.0, 3000.0, 5000.0];
        let db = [-20.0, -30.0, -30.0, -10.0];
        assert!((band_energy_db(&hz, &db, 1500.0, 4000.0) + 30.0).abs() < 1e-9);
        assert_eq!(band_energy_db(&hz, &db, 10.0, 20.0), -120.0);
    }

    #[test]
    fn detector_ignores_steady_and_fires_on_jump_once() {
        let mut d = TransientDetector::new(6.0);
        for _ in 0..50 {
            assert!(!d.push(-40.0));
        }
        let fired: Vec<bool> = [-25.0, -25.0, -26.0, -30.0].iter().map(|&l| d.push(l)).collect();
        assert_eq!(fired.iter().filter(|&&f| f).count(), 1);
    }

    #[test]
    fn synth_produces_footstep_transients() {
        let mut s = Synth::new(7);
        let hits = (0..300).filter(|_| s.next(1000.0 / 60.0).transient).count();
        assert!(hits >= 5, "only {hits} transients in 5 s");
        let f = s.next(16.0);
        assert_eq!(f.db.len(), BIN_COUNT);
    }
}
