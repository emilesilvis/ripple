//! Descriptors of rendered sound, independent of recipe parameters.
use super::worlds::Soundscape;
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};

pub const ANALYSIS_RATE: u32 = 16_000;
pub const ANALYSIS_SECONDS: usize = 12;
const FFT: usize = 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Character {
    pub brightness: f64,
    pub texture: f64,
    pub motion: f64,
    pub width: f64,
    pub roughness: f64,
    pub high_fraction: f64,
    pub onsets_per_second: f64,
    pub rms: f64,
    pub peak: f64,
}

impl Character {
    /// 4 brightness x 3 texture x 3 motion x 3 width cells. Thresholds describe
    /// sound after rendering; they never select instruments or synthesis types.
    pub fn cell(&self) -> [u8; 4] {
        let bin = |value: f64, cuts: &[f64]| cuts.iter().filter(|c| value >= **c).count() as u8;
        [
            bin(self.brightness, &[220.0, 650.0, 1600.0]),
            bin(self.texture, &[0.5, 0.75]),
            bin(self.motion, &[0.18, 0.45]),
            bin(self.width, &[0.05, 0.2]),
        ]
    }

    pub fn label(&self) -> String {
        let c = self.cell();
        format!(
            "{} / {} / {} / {}",
            ["deep", "warm", "bright", "high"][c[0] as usize],
            ["tonal", "textured", "diffuse"][c[1] as usize],
            ["steady", "drifting", "pulsing"][c[2] as usize],
            ["focused", "open", "wide"][c[3] as usize]
        )
    }

    /// A calmness proxy, not a preference model. Comparisons only occur inside
    /// a cell, so a low-scoring sound cannot evict other kinds of sounds.
    pub fn score(&self) -> f64 {
        self.roughness
            + 0.25 * self.high_fraction
            + 0.015 * (self.peak / self.rms.max(1e-9) - 6.0).max(0.0)
            + 0.025 * (self.onsets_per_second - 2.0).max(0.0)
    }

    pub fn validate(&self) -> Result<(), String> {
        let bounded = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !(bounded(self.brightness, 30.0, 6500.0)
            && bounded(self.texture, 0.0, 1.0)
            && bounded(self.motion, 0.0, 20.0)
            && bounded(self.width, 0.0, 1.0)
            && bounded(self.roughness, 0.0, 1.0)
            && bounded(self.high_fraction, 0.0, 1.0)
            && bounded(self.onsets_per_second, 0.0, 10.0)
            && bounded(self.rms, 0.0003, 0.5)
            && bounded(self.peak, self.rms, 0.500001))
        {
            return Err("audio is silent, non-finite, or outside the listening bounds".into());
        }
        Ok(())
    }
}

// Iterative radix-2 FFT. Analysis is outside the callback; runtime synthesis
// does not know anything about descriptors or search cells.
fn spectrum(real: &mut [f64; FFT]) {
    let mut imaginary = [0.0; FFT];
    let mut j = 0;
    for i in 1..FFT {
        let mut bit = FFT >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            real.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= FFT {
        let (si, co) = (-TAU / len as f64).sin_cos();
        for start in (0..FFT).step_by(len) {
            let (mut wr, mut wi) = (1.0, 0.0);
            for offset in 0..len / 2 {
                let a = start + offset;
                let b = a + len / 2;
                let (tr, ti) = (
                    wr * real[b] - wi * imaginary[b],
                    wr * imaginary[b] + wi * real[b],
                );
                (real[b], imaginary[b]) = (real[a] - tr, imaginary[a] - ti);
                (real[a], imaginary[a]) = (real[a] + tr, imaginary[a] + ti);
                (wr, wi) = (wr * co - wi * si, wr * si + wi * co);
            }
        }
        len *= 2;
    }
    for i in 0..FFT {
        real[i] = real[i].powi(2) + imaginary[i].powi(2);
    }
}

struct Meter {
    count: usize,
    squares: f64,
    side: f64,
    peak: f64,
    window: [f64; FFT],
    powers: [f64; FFT / 2],
    block_power: f64,
    levels: Vec<f64>,
}

impl Meter {
    fn new() -> Self {
        Self {
            count: 0,
            squares: 0.0,
            side: 0.0,
            peak: 0.0,
            window: [0.0; FFT],
            powers: [0.0; FFT / 2],
            block_power: 0.0,
            levels: Vec::new(),
        }
    }
    fn push(&mut self, (l, r): (f32, f32)) -> Result<(), String> {
        if !l.is_finite() || !r.is_finite() {
            return Err("non-finite audio".into());
        }
        let (l, r) = (f64::from(l), f64::from(r));
        let power = (l * l + r * r) * 0.5;
        self.squares += power;
        self.block_power += power;
        self.side += (l - r).powi(2) * 0.25;
        self.peak = self.peak.max(l.abs()).max(r.abs());
        let i = self.count % FFT;
        self.window[i] = l * (0.5 - 0.5 * (TAU * i as f64 / FFT as f64).cos());
        if i == FFT - 1 {
            spectrum(&mut self.window);
            for (sum, value) in self.powers.iter_mut().zip(self.window) {
                *sum += value;
            }
        }
        self.count += 1;
        if self.count % (ANALYSIS_RATE as usize / 10) == 0 {
            self.levels
                .push((self.block_power / (ANALYSIS_RATE as f64 / 10.0)).sqrt());
            self.block_power = 0.0;
        }
        Ok(())
    }
    fn finish(self) -> Character {
        let lo = 2;
        let hi = 416; // 31..6500 Hz at the canonical analysis sample rate.
        let total = self.powers[lo..hi].iter().sum::<f64>().max(1e-30);
        let mut brightness = 0.0;
        let mut entropy = 0.0;
        let mut high_fraction = 0.0;
        for i in lo..hi {
            let p = self.powers[i] / total;
            let f = i as f64 * ANALYSIS_RATE as f64 / FFT as f64;
            brightness += p * f;
            entropy -= p * p.max(1e-30).ln();
            if f > 3200.0 {
                high_fraction += p;
            }
        }
        let mut peaks: Vec<_> = (lo..hi)
            .filter(|&i| {
                self.powers[i] > self.powers[i - 1] && self.powers[i] >= self.powers[i + 1]
            })
            .map(|i| {
                (
                    i as f64 * ANALYSIS_RATE as f64 / FFT as f64,
                    self.powers[i].sqrt(),
                )
            })
            .collect();
        peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
        peaks.truncate(24);
        let weight = peaks.iter().map(|p| p.1).sum::<f64>().max(1e-30);
        let mut roughness = 0.0;
        for (i, a) in peaks.iter().enumerate() {
            for b in peaks.iter().skip(i + 1) {
                let d = (a.0 - b.0).abs() * 0.24 / (0.021 * a.0.min(b.0) + 19.0);
                roughness +=
                    2.0 * a.1 * b.1 / weight.powi(2) * ((-3.5 * d).exp() - (-5.75 * d).exp());
            }
        }
        let mean = self.levels.iter().sum::<f64>() / self.levels.len().max(1) as f64;
        let variance = self.levels.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
            / self.levels.len().max(1) as f64;
        let onsets = self
            .levels
            .windows(2)
            .filter(|p| p[1] > 1.6 * p[0] && p[1] > mean * 0.6)
            .count();
        Character {
            brightness,
            texture: (entropy / ((hi - lo) as f64).ln()).clamp(0.0, 1.0),
            motion: variance.sqrt() / mean.max(1e-12),
            width: (self.side / self.squares.max(1e-30)).clamp(0.0, 1.0),
            roughness,
            high_fraction: high_fraction.clamp(0.0, 1.0),
            onsets_per_second: onsets as f64 / (self.count as f64 / ANALYSIS_RATE as f64),
            rms: (self.squares / self.count.max(1) as f64).sqrt(),
            peak: self.peak,
        }
    }
}

pub fn measure(scene: &mut Soundscape, cancel: &AtomicBool) -> Result<Character, String> {
    let mut meter = Meter::new();
    let warmup = ANALYSIS_RATE as usize * 3;
    for i in 0..warmup + ANALYSIS_RATE as usize * ANALYSIS_SECONDS {
        if i % 1024 == 0 && cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let sample = scene.next_sample();
        if i >= warmup {
            meter.push(sample)?;
        } else if !sample.0.is_finite() || !sample.1.is_finite() {
            return Err("non-finite warmup audio".into());
        }
    }
    let result = meter.finish();
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendered_descriptors_distinguish_pitch_texture_motion_and_space() {
        let mut low = Meter::new();
        let mut high = Meter::new();
        let mut noise = Meter::new();
        let mut rng = super::super::Random(42);
        for i in 0..ANALYSIS_RATE * 3 {
            let t = i as f64 / ANALYSIS_RATE as f64;
            let a = (TAU * 125.0 * t).sin() as f32 * 0.05;
            let b = (TAU * 2000.0 * t).sin() as f32 * 0.05;
            low.push((a, a)).unwrap();
            high.push((b, b)).unwrap();
            let level = (TAU * 0.7 * t).sin().powi(2) as f32 * 0.05;
            noise
                .push((
                    (rng.unit() as f32 - 0.5) * level,
                    (rng.unit() as f32 - 0.5) * level,
                ))
                .unwrap();
        }
        let (low, high, noise) = (low.finish(), high.finish(), noise.finish());
        assert!((low.brightness - 125.0).abs() < 1.0);
        assert!((high.brightness - 2000.0).abs() < 1.0);
        assert!(noise.texture > low.texture + 0.4);
        assert!(noise.motion > low.motion + 0.4);
        assert!(noise.width > 0.4 && low.width == 0.0);
        assert_ne!(low.cell(), high.cell());
        assert_ne!(high.cell(), noise.cell());
    }
}
