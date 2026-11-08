//! Biquad filter math (RBJ Audio EQ Cookbook), magnitude response for drawing the
//! EQ curve, and the no-clip loudness guard that derives the preamp from the curve.
//!
//! `Biquad` / `Chain` are the realtime engine path; only tests exercise them
//! until the owner wires the WASAPI fallback renderer.
#![cfg_attr(not(test), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Reference sample rate used for coefficient design and curve rendering.
pub const SAMPLE_RATE: f64 = 48_000.0;
/// Lowest / highest frequency drawn and accepted for a band.
pub const MIN_HZ: f64 = 20.0;
pub const MAX_HZ: f64 = 20_000.0;
/// Hard gain limits per band, matching the UI fader travel.
pub const MAX_GAIN_DB: f64 = 15.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterKind {
    Peaking,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
}

impl FilterKind {
    /// Filter type token used by Equalizer APO `Filter:` lines.
    pub fn apo_code(self) -> &'static str {
        match self {
            FilterKind::Peaking => "PK",
            FilterKind::LowShelf => "LSC",
            FilterKind::HighShelf => "HSC",
            FilterKind::HighPass => "HPQ",
            FilterKind::LowPass => "LPQ",
        }
    }

    pub fn has_gain(self) -> bool {
        !matches!(self, FilterKind::HighPass | FilterKind::LowPass)
    }
}

/// One parametric EQ band as edited in the UI and stored in a profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    pub kind: FilterKind,
    pub freq: f64,
    pub gain_db: f64,
    pub q: f64,
    pub enabled: bool,
}

impl Band {
    pub fn new(kind: FilterKind, freq: f64, gain_db: f64, q: f64) -> Self {
        Band { kind, freq, gain_db, q, enabled: true }.clamped()
    }

    pub fn peak(freq: f64, gain_db: f64, q: f64) -> Self {
        Band::new(FilterKind::Peaking, freq, gain_db, q)
    }

    /// Keep values inside the ranges the engine and Equalizer APO accept.
    pub fn clamped(mut self) -> Self {
        self.freq = self.freq.clamp(MIN_HZ, MAX_HZ);
        self.gain_db = self.gain_db.clamp(-MAX_GAIN_DB, MAX_GAIN_DB);
        self.q = self.q.clamp(0.1, 18.0);
        self
    }

    pub fn coefficients(&self, fs: f64) -> Coeffs {
        Coeffs::design(self, fs)
    }

    /// Magnitude response of this band alone at `hz`, in dB.
    pub fn response_db(&self, hz: f64, fs: f64) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        self.coefficients(fs).magnitude_db(hz, fs)
    }
}

/// Normalised biquad coefficients (a0 == 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    /// RBJ cookbook formulas. Shelves use the Q form (S derived from Q).
    pub fn design(band: &Band, fs: f64) -> Coeffs {
        let f0 = band.freq.min(fs * 0.49);
        let w0 = 2.0 * PI * f0 / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * band.q);
        let a = 10f64.powf(band.gain_db / 40.0);
        let sqa = 2.0 * a.sqrt() * alpha;

        let (b0, b1, b2, a0, a1, a2) = match band.kind {
            FilterKind::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            FilterKind::LowShelf => (
                a * ((a + 1.0) - (a - 1.0) * cos + sqa),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - sqa),
                (a + 1.0) + (a - 1.0) * cos + sqa,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - sqa,
            ),
            FilterKind::HighShelf => (
                a * ((a + 1.0) + (a - 1.0) * cos + sqa),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - sqa),
                (a + 1.0) - (a - 1.0) * cos + sqa,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - sqa,
            ),
            FilterKind::HighPass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            FilterKind::LowPass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
        };
        Coeffs { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// |H(e^jw)| in dB, evaluated directly from the transfer function.
    pub fn magnitude_db(&self, hz: f64, fs: f64) -> f64 {
        let w = 2.0 * PI * hz / fs;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        let mag2 = (nr * nr + ni * ni) / (dr * dr + di * di);
        10.0 * mag2.max(1e-30).log10()
    }
}

/// Transposed direct form II biquad used by the realtime engine.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    c: Coeffs,
    z1: f64,
    z2: f64,
}

impl Biquad {
    pub fn new(c: Coeffs) -> Self {
        Biquad { c, z1: 0.0, z2: 0.0 }
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// A cascade of biquads plus a linear preamp; one per output channel.
// TODO(owner): feed this from a WASAPI shared-mode render stream for the
// "no Equalizer APO" fallback path. Today system-wide audio goes through APO.
pub struct Chain {
    stages: Vec<Biquad>,
    preamp: f64,
}

impl Chain {
    pub fn new(bands: &[Band], preamp_db: f64, fs: f64) -> Self {
        let stages = bands
            .iter()
            .filter(|b| b.enabled)
            .map(|b| Biquad::new(b.coefficients(fs)))
            .collect();
        Chain { stages, preamp: db_to_lin(preamp_db) }
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            let mut x = *s as f64 * self.preamp;
            for st in self.stages.iter_mut() {
                x = st.process(x);
            }
            *s = x as f32;
        }
    }

    pub fn reset(&mut self) {
        self.stages.iter_mut().for_each(Biquad::reset);
    }
}

pub fn db_to_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// `n` log-spaced frequencies between `lo` and `hi` inclusive.
pub fn log_space(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    if n < 2 {
        return vec![lo];
    }
    let (l, h) = (lo.ln(), hi.ln());
    (0..n).map(|i| (l + (h - l) * i as f64 / (n - 1) as f64).exp()).collect()
}

/// Combined response of all bands at one frequency.
pub fn response_at(bands: &[Band], hz: f64, fs: f64) -> f64 {
    bands.iter().map(|b| b.response_db(hz, fs)).sum()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Curve {
    pub hz: Vec<f64>,
    pub db: Vec<f64>,
}

/// Curve used by the UI to draw the parametric EQ line.
pub fn curve(bands: &[Band], points: usize) -> Curve {
    let coeffs: Vec<Coeffs> = bands
        .iter()
        .filter(|b| b.enabled)
        .map(|b| b.coefficients(SAMPLE_RATE))
        .collect();
    let hz = log_space(MIN_HZ, MAX_HZ, points.clamp(16, 2048));
    let db = hz
        .iter()
        .map(|&f| coeffs.iter().map(|c| c.magnitude_db(f, SAMPLE_RATE)).sum())
        .collect();
    Curve { hz, db }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GuardResult {
    /// Largest boost of the summed curve (can exceed the max single band gain).
    pub peak_db: f64,
    pub peak_hz: f64,
    /// Largest single band gain, the quick estimate shown next to the faders.
    pub max_band_db: f64,
    /// Preamp to apply so the boosted region cannot clip.
    pub preamp_db: f64,
}

/// No-clip loudness guard: derive a negative preamp from the peak of the curve
/// plus a safety headroom, rounded to 0.1 dB as Equalizer APO displays it.
pub fn no_clip_preamp(bands: &[Band], headroom_db: f64) -> GuardResult {
    let c = curve(bands, 512);
    let (mut peak_db, mut peak_hz) = (f64::MIN, MIN_HZ);
    for (&f, &d) in c.hz.iter().zip(c.db.iter()) {
        if d > peak_db {
            peak_db = d;
            peak_hz = f;
        }
    }
    let max_band_db = bands
        .iter()
        .filter(|b| b.enabled && b.kind.has_gain())
        .map(|b| b.gain_db)
        .fold(0.0, f64::max);
    let boost = peak_db.max(max_band_db).max(0.0);
    let preamp = if boost > 0.0 { -(boost + headroom_db.max(0.0)) } else { 0.0 };
    GuardResult {
        peak_db: round1(peak_db),
        peak_hz: peak_hz.round(),
        max_band_db: round1(max_band_db),
        preamp_db: round1(preamp),
    }
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = SAMPLE_RATE;

    #[test]
    fn peaking_hits_gain_at_center() {
        let b = Band::peak(2000.0, 6.0, 1.4);
        assert!((b.response_db(2000.0, FS) - 6.0).abs() < 0.01);
        assert!(b.response_db(40.0, FS).abs() < 0.1);
        assert!(b.response_db(19_000.0, FS).abs() < 0.3);
    }

    #[test]
    fn cut_is_symmetric_to_boost() {
        let up = Band::peak(160.0, 5.0, 1.0);
        let down = Band::peak(160.0, -5.0, 1.0);
        for f in [80.0, 160.0, 320.0] {
            assert!((up.response_db(f, FS) + down.response_db(f, FS)).abs() < 1e-6);
        }
    }

    #[test]
    fn shelves_reach_their_plateaus() {
        let ls = Band::new(FilterKind::LowShelf, 120.0, -4.0, 0.71);
        assert!((ls.response_db(20.0, FS) + 4.0).abs() < 0.15);
        assert!(ls.response_db(5000.0, FS).abs() < 0.05);
        let hs = Band::new(FilterKind::HighShelf, 6000.0, 3.0, 0.71);
        assert!((hs.response_db(20_000.0, FS) - 3.0).abs() < 0.2);
        assert!(hs.response_db(200.0, FS).abs() < 0.05);
    }

    #[test]
    fn highpass_is_minus_3db_at_cutoff_for_butterworth_q() {
        let hp = Band::new(FilterKind::HighPass, 100.0, 0.0, std::f64::consts::FRAC_1_SQRT_2);
        assert!((hp.response_db(100.0, FS) + 3.01).abs() < 0.05);
        assert!(hp.response_db(10_000.0, FS).abs() < 0.01);
    }

    #[test]
    fn disabled_band_is_transparent() {
        let mut b = Band::peak(1000.0, 9.0, 2.0);
        b.enabled = false;
        assert_eq!(b.response_db(1000.0, FS), 0.0);
    }

    #[test]
    fn biquad_dc_gain_matches_magnitude() {
        let b = Band::new(FilterKind::LowShelf, 200.0, 6.0, 0.71);
        let mut f = Biquad::new(b.coefficients(FS));
        let mut y = 0.0;
        for _ in 0..48_000 {
            y = f.process(1.0);
        }
        let expect = db_to_lin(b.coefficients(FS).magnitude_db(1.0, FS));
        assert!((y - expect).abs() < 1e-3, "{y} vs {expect}");
    }

    #[test]
    fn chain_applies_preamp() {
        let mut c = Chain::new(&[], -6.0, FS);
        let mut buf = [1.0f32; 4];
        c.process_block(&mut buf);
        c.reset();
        assert!((buf[0] as f64 - db_to_lin(-6.0)).abs() < 1e-6);
    }

    #[test]
    fn guard_compensates_peak_with_headroom() {
        let bands = [Band::peak(2500.0, 4.0, 1.4), Band::peak(3500.0, 4.0, 1.4)];
        let g = no_clip_preamp(&bands, 0.5);
        assert!(g.peak_db > 4.0, "overlapping bands sum above single gain");
        assert_eq!(g.max_band_db, 4.0);
        assert!((g.preamp_db + g.peak_db + 0.5).abs() < 0.11);
    }

    #[test]
    fn guard_is_zero_for_cuts_only() {
        let g = no_clip_preamp(&[Band::peak(150.0, -6.0, 1.0)], 1.0);
        assert_eq!(g.preamp_db, 0.0);
    }

    #[test]
    fn log_space_endpoints() {
        let v = log_space(20.0, 20_000.0, 4);
        assert!((v[0] - 20.0).abs() < 1e-9 && (v[3] - 20_000.0).abs() < 1e-6);
        assert!((v[1] - 200.0).abs() < 1e-6);
    }

    #[test]
    fn clamping_limits_values() {
        let b = Band::peak(50_000.0, 40.0, 0.0);
        assert_eq!((b.freq, b.gain_db, b.q), (MAX_HZ, MAX_GAIN_DB, 0.1));
    }
}
