//! Matter: bodies as lattices of bonded point masses.
//!
//! A material here knows no frequencies. A substance is two constants — how
//! fast a bending wave runs through it, and how much of every flex is lost
//! inside it as heat — and a body adds only geometry: a length, a thickness.
//! The body is discretised as a small lattice of point masses joined by stiff
//! bonds, and struck matter is integrated directly with Newton's law, sample
//! by sample.
//!
//! Everything the ear knows about a material then *emerges*:
//!
//! - A long steel bar rings at the free-bar overtone ratios 1 : 2.76 : 5.40 —
//!   the chime's signature — because that is what a stiff lattice with free
//!   ends does. No overtone is written anywhere.
//! - The *same* steel, rolled into a high-pitched panel, rings for only
//!   milliseconds, because internal loss grows as frequency squared. One
//!   constant makes a chime sustain for seconds and a rain-struck roof ping
//!   and die.
//! - The click of contact is the strike itself: a sharp kick excites the
//!   lattice's highest modes, which perish almost instantly.
//! - Stereo is geometric: the body is listened to at two points on its
//!   surface, and a strike near one edge genuinely arrives louder there.

use crate::dsp::Noise;
use std::f32::consts::TAU;

/// A substance, with no idea what it will be shaped into.
#[derive(Clone, Copy)]
pub struct Matter {
    /// Longitudinal wave speed sqrt(E/rho), m/s. Steel ~5100; wood ~4000 along
    /// the grain. (Wood and steel are surprisingly alike here — what separates
    /// them to the ear is loss, not stiffness.)
    pub wave_speed: f32,
    /// Internal friction (s): the loss rate of a mode is `internal_loss * w^2`,
    /// so high modes die fastest — the physical reason wood thuds while steel
    /// rings, and why the same steel pings briefly at 3 kHz yet sings for
    /// seconds at 300 Hz.
    pub internal_loss: f32,
    /// Loss to the surrounding air, 1/s — a flat floor under every mode.
    pub air_loss: f32,
}

impl Matter {
    pub fn steel() -> Self {
        Self { wave_speed: 5100.0, internal_loss: 2.0e-7, air_loss: 0.3 }
    }
    pub fn wood() -> Self {
        Self { wave_speed: 4000.0, internal_loss: 1.1e-5, air_loss: 8.0 }
    }
}

/// The bending stiffness constant of a beam or plate cross-section:
/// `c_L * radius_of_gyration`, in m^2/s. A mode of wavenumber `beta` (rad/m)
/// rings at `w = stiff * beta^2` — the Euler–Bernoulli law.
fn section_stiffness(matter: &Matter, thickness_m: f32) -> f32 {
    matter.wave_speed * thickness_m / 12f32.sqrt()
}

/// The free-free bar's bending operator: curvature on the interior,
/// redistributed by its transpose (D^T D). Symmetric, so it conserves energy;
/// its null space is exactly the rigid motions (translation and tilt).
fn bar_bend(src: &[f32], dst: &mut [f32]) {
    let n = src.len();
    let mut cur = vec![0.0f32; n];
    for j in 1..n - 1 {
        cur[j] = src[j - 1] - 2.0 * src[j] + src[j + 1];
    }
    for i in 0..n {
        let cm = if i >= 1 { cur[i - 1] } else { 0.0 };
        let cp = if i + 1 < n { cur[i + 1] } else { 0.0 };
        dst[i] = cm - 2.0 * cur[i] + cp;
    }
}

/// The smallest *flexural* eigenvalue of the n-node free bar operator — the
/// lattice's own fundamental, found by power iteration with the two rigid
/// modes (translation, tilt) projected out. No closed form is assumed.
fn bar_fundamental_eigenvalue(nodes: usize) -> f32 {
    let n = nodes;
    // Orthonormal rigid modes to deflate.
    let e0: Vec<f32> = vec![1.0 / (n as f32).sqrt(); n];
    let mut e1: Vec<f32> = (0..n).map(|i| i as f32 - (n as f32 - 1.0) / 2.0).collect();
    let norm = e1.iter().map(|v| v * v).sum::<f32>().sqrt();
    e1.iter_mut().for_each(|v| *v /= norm);

    // Power-iterate on (cI - B): its top surviving mode is B's smallest
    // flexural one. c must dominate B's spectrum (bounded by 16).
    let c = 17.0f32;
    let mut x: Vec<f32> = (0..n).map(|i| ((i * 37 + 11) % 23) as f32 - 11.0).collect();
    let mut bx = vec![0.0f32; n];
    for _ in 0..400 {
        let d0: f32 = x.iter().zip(&e0).map(|(a, b)| a * b).sum();
        let d1: f32 = x.iter().zip(&e1).map(|(a, b)| a * b).sum();
        for i in 0..n {
            x[i] -= d0 * e0[i] + d1 * e1[i];
        }
        bar_bend(&x, &mut bx);
        for i in 0..n {
            x[i] = c * x[i] - bx[i];
        }
        let norm = x.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-20);
        x.iter_mut().for_each(|v| *v /= norm);
    }
    bar_bend(&x, &mut bx);
    let num: f32 = x.iter().zip(&bx).map(|(a, b)| a * b).sum();
    let den: f32 = x.iter().map(|v| v * v).sum();
    num / den
}

/// The chime-maker's craft: invert the bending law to find how long to cut a
/// bar of this matter and thickness so its *fundamental* lands at `f1`. The
/// pitch itself is never handed to the lattice — only the length is; the bar
/// rings where physics puts it. (Solved against the discrete lattice's own
/// spectrum, so the cut is true for the bar we actually build.)
pub fn bar_length_for_pitch(matter: &Matter, thickness_m: f32, nodes: usize, f1: f32) -> f32 {
    let stiff = section_stiffness(matter, thickness_m);
    let lambda1 = bar_fundamental_eigenvalue(nodes);
    // w1 = (stiff / a^2) * sqrt(lambda1)  =>  a = sqrt(stiff * sqrt(lambda1) / w1)
    let a = (stiff * lambda1.sqrt() / (TAU * f1)).sqrt();
    a * (nodes as f32 - 1.0)
}

/// A body: matter poured into a shape, discretised as a lattice.
///
/// `u` is each node's transverse displacement, `v` its velocity. The elastic
/// force is the discrete free-edge bending operator (curvature measured, then
/// re-distributed — symmetric, so the lattice conserves energy up to its
/// honest losses). Integration is semi-implicit Euler with as many substeps
/// as the stiffest mode demands.
pub struct Body {
    w: usize,
    h: usize, // 1 for a bar
    u: Vec<f32>,
    v: Vec<f32>,
    scratch: Vec<f32>,
    scratch2: Vec<f32>,
    /// Bond stiffness (stiff/a^2)^2 folded per unit operator eigenvalue, 1/s^2.
    k: f32,
    /// Internal loss time constant (applied as stiffness-proportional damping).
    gamma: f32,
    air: f32,
    /// A weak spring to the rest position — the mounting (a cord, some nails).
    /// It pins the rigid free-body modes far below hearing.
    anchor: f32,
    substeps: usize,
    dt_sub: f32,
    pick_l: usize,
    pick_r: usize,
    prev_l: f32,
    prev_r: f32,
    out_gain: f32,
    rng: Noise,
}

impl Body {
    /// A bar: a 1-D lattice with free ends (a chime tube, a log).
    pub fn bar(
        matter: &Matter,
        length_m: f32,
        thickness_m: f32,
        nodes: usize,
        sr: f32,
        seed: u32,
        out_gain: f32,
    ) -> Self {
        let a = length_m / (nodes as f32 - 1.0);
        Self::build(matter, thickness_m, a, nodes, 1, sr, seed, out_gain)
    }

    /// A sheet: a 2-D lattice with free edges (a roof panel). `thickness_m` is
    /// the *effective* stiffness thickness — corrugation makes a 0.5 mm skin
    /// bend like centimetres of solid metal, which is why tin roofs ring in
    /// the kilohertz instead of flapping subsonically.
    pub fn sheet(
        matter: &Matter,
        w_m: f32,
        h_m: f32,
        thickness_m: f32,
        nodes_across: usize,
        sr: f32,
        seed: u32,
        out_gain: f32,
    ) -> Self {
        let a = w_m / (nodes_across as f32 - 1.0);
        let hn = ((h_m / a).round() as usize + 1).max(3);
        Self::build(matter, thickness_m, a, nodes_across, hn, sr, seed, out_gain)
    }

    fn build(
        matter: &Matter,
        thickness_m: f32,
        spacing_m: f32,
        w: usize,
        h: usize,
        sr: f32,
        seed: u32,
        out_gain: f32,
    ) -> Self {
        let n = w * h;
        let stiff = section_stiffness(matter, thickness_m);
        let k = (stiff / (spacing_m * spacing_m)).powi(2);
        let gamma = matter.internal_loss;
        let anchor = (TAU * 18.0).powi(2); // mounting resonance ~18 Hz

        // The stiffest mode the lattice can hold sets the substep count.
        let eig_max = if h == 1 { 16.0 } else { 64.0 };
        let w_max = (k * eig_max + anchor).sqrt();
        let damp_max = gamma * k * eig_max + matter.air_loss;
        let dt = 1.0 / sr;
        let need = (dt * w_max.max(damp_max) / 1.4).ceil() as usize;
        let substeps = need.max(1);

        let pick_l = if h == 1 { w / 3 } else { (h / 2) * w + w / 3 };
        let pick_r = if h == 1 { 2 * w / 3 } else { (h / 2) * w + 2 * w / 3 };

        Self {
            w,
            h,
            u: vec![0.0; n],
            v: vec![0.0; n],
            scratch: vec![0.0; n],
            scratch2: vec![0.0; n],
            k,
            gamma,
            air: matter.air_loss,
            anchor,
            substeps,
            dt_sub: dt / substeps as f32,
            pick_l,
            pick_r,
            prev_l: 0.0,
            prev_r: 0.0,
            out_gain,
            rng: Noise::new(seed ^ 0xb0d1_e5),
        }
    }

    /// Deposit a sharp kick of momentum at a position across the body
    /// (0 = left edge, 1 = right edge). Where exactly it lands decides which
    /// modes wake — every strike voices the body a little differently.
    #[inline]
    pub fn strike(&mut self, energy: f32, pos: f32) {
        let col = ((pos.clamp(0.0, 1.0) * (self.w as f32 - 1.0)).round() as usize).min(self.w - 1);
        let row = if self.h == 1 {
            0
        } else {
            (self.rng.unit() * self.h as f32) as usize % self.h
        };
        self.v[row * self.w + col] += energy;
    }

    /// One elastic pass: force = -k * B(u + gamma*v) - anchor*u - air*v, where
    /// B is the free-edge bending operator. Damping rides the same operator
    /// (Rayleigh stiffness damping), so loss grows as frequency squared.
    fn step_sub(&mut self) {
        let n = self.w * self.h;
        // s = u + gamma * v (elastic + internal-loss argument, one pass)
        for i in 0..n {
            self.scratch2[i] = self.u[i] + self.gamma * self.v[i];
        }

        if self.h == 1 {
            // Bar: curvature on the interior, redistributed by its transpose.
            // This is the free-free beam operator; its null space is exactly
            // the rigid motions (translation and tilt), held by the anchor.
            let s = &self.scratch2;
            let c = &mut self.scratch;
            c[0] = 0.0;
            c[n - 1] = 0.0;
            for j in 1..n - 1 {
                c[j] = s[j - 1] - 2.0 * s[j] + s[j + 1];
            }
            for i in 0..n {
                let cm = if i >= 1 { self.scratch[i - 1] } else { 0.0 };
                let cp = if i + 1 < n { self.scratch[i + 1] } else { 0.0 };
                let bend = cm - 2.0 * self.scratch[i] + cp;
                let f = -self.k * bend - self.anchor * self.u[i] - self.air * self.v[i];
                self.v[i] += self.dt_sub * f;
            }
        } else {
            // Sheet: the free-edge Laplacian applied twice (the plate's
            // biharmonic), symmetric by construction.
            let (w, h) = (self.w, self.h);
            let lap = |src: &[f32], dst: &mut [f32]| {
                for y in 0..h {
                    for x in 0..w {
                        let i = y * w + x;
                        let mut acc = 0.0;
                        if x > 0 {
                            acc += src[i - 1] - src[i];
                        }
                        if x + 1 < w {
                            acc += src[i + 1] - src[i];
                        }
                        if y > 0 {
                            acc += src[i - w] - src[i];
                        }
                        if y + 1 < h {
                            acc += src[i + w] - src[i];
                        }
                        dst[i] = acc;
                    }
                }
            };
            lap(&self.scratch2, &mut self.scratch);
            let tmp = std::mem::take(&mut self.scratch2);
            let mut out = tmp;
            lap(&self.scratch, &mut out);
            for i in 0..n {
                let f = -self.k * out[i] - self.anchor * self.u[i] - self.air * self.v[i];
                self.v[i] += self.dt_sub * f;
            }
            self.scratch2 = out;
        }

        for i in 0..n {
            self.u[i] += self.dt_sub * self.v[i];
        }
    }

    /// Advance one sample and listen at the two pickup points. What radiates
    /// is the surface's *acceleration* (taken as the per-sample velocity
    /// change), which is also what makes the strike click all by itself.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        for _ in 0..self.substeps {
            self.step_sub();
        }
        let vl = self.v[self.pick_l];
        let vr = self.v[self.pick_r];
        let l = (vl - self.prev_l) * self.out_gain;
        let r = (vr - self.prev_r) * self.out_gain;
        self.prev_l = vl;
        self.prev_r = vr;
        (l, r)
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Power at frequency f over a signal (Goertzel).
    fn power_at(x: &[f32], sr: f32, f: f32) -> f64 {
        let k = 2.0 * (TAU as f64 * f as f64 / sr as f64).cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s0 = v as f64 + k * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - k * s1 * s2) / (x.len() as f64 * x.len() as f64)
    }

    /// The frequency of the strongest peak in [lo, hi], scanned at 2 Hz.
    fn peak_in(x: &[f32], sr: f32, lo: f32, hi: f32) -> f32 {
        let mut best = (lo, 0.0f64);
        let mut f = lo;
        while f <= hi {
            let p = power_at(x, sr, f);
            if p > best.1 {
                best = (f, p);
            }
            f += 2.0;
        }
        best.0
    }

    // Listen on one pickup only: summing the two symmetric pickups would
    // null every antisymmetric mode (including the bar's second overtone) —
    // real interference, but not what these tests want to measure.
    fn ring(body: &mut Body, sr: f32, secs: f32) -> Vec<f32> {
        (0..(sr * secs) as usize).map(|_| body.process().0).collect()
    }

    /// A steel bar cut for 400 Hz must ring at 400 Hz with the free-bar
    /// overtone ratios 1 : 2.76 : 5.40 — none of which appear in the code.
    #[test]
    fn tuned_bar_rings_at_free_bar_ratios() {
        let sr = 48_000.0;
        let steel = Matter::steel();
        let len = bar_length_for_pitch(&steel, 0.022, 14, 400.0);
        let mut bar = Body::bar(&steel, len, 0.022, 14, sr, 7, 1.0);
        bar.strike(0.3, 0.23);
        let x = ring(&mut bar, sr, 1.0);

        let f1 = peak_in(&x, sr, 300.0, 500.0);
        assert!((f1 - 400.0).abs() / 400.0 < 0.06, "fundamental {f1} Hz, wanted ~400");

        let f2 = peak_in(&x, sr, f1 * 2.756 * 0.85, f1 * 2.756 * 1.15);
        let r2 = f2 / f1;
        assert!((r2 - 2.756).abs() / 2.756 < 0.08, "second-mode ratio {r2}, wanted ~2.76");

        let f3 = peak_in(&x, sr, f1 * 5.404 * 0.85, f1 * 5.404 * 1.15);
        let r3 = f3 / f1;
        assert!((r3 - 5.404).abs() / 5.404 < 0.10, "third-mode ratio {r3}, wanted ~5.40");
    }

    /// One loss constant, two behaviours: the same steel sustains for seconds
    /// as a low bar but is gone in a blink as a high panel; wood is dead fast.
    #[test]
    fn loss_scales_with_frequency_squared() {
        let sr = 48_000.0;
        let steel = Matter::steel();
        let rms = |x: &[f32], from: f32, to: f32| {
            let a = (from * sr) as usize;
            let b = (to * sr) as usize;
            (x[a..b].iter().map(|v| (v * v) as f64).sum::<f64>() / (b - a) as f64).sqrt()
        };

        let len = bar_length_for_pitch(&steel, 0.022, 14, 262.0);
        let mut chime = Body::bar(&steel, len, 0.022, 14, sr, 7, 1.0);
        chime.strike(0.3, 0.31);
        let x = ring(&mut chime, sr, 1.6);
        // Compare power at the fundamental itself (broadband strike-click
        // energy dominates the early samples, so raw RMS misleads here).
        let f1 = peak_in(&x[..(0.4 * sr) as usize], sr, 200.0, 330.0);
        let early = power_at(&x[(0.05 * sr) as usize..(0.35 * sr) as usize], sr, f1);
        let late = power_at(&x[(1.2 * sr) as usize..(1.5 * sr) as usize], sr, f1);
        let sustain = late / early.max(1e-30);
        assert!(sustain > 0.1, "low steel bar should still ring at {f1} Hz after 1.2 s (got {sustain})");

        let mut wood = Body::bar(&Matter::wood(), 1.2, 0.10, 12, sr, 7, 1.0);
        wood.strike(0.3, 0.31);
        let y = ring(&mut wood, sr, 0.8);
        let thud = rms(&y, 0.4, 0.7) / rms(&y, 0.0, 0.1).max(1e-12);
        assert!(thud < 0.02, "wood should be silent well before 0.4 s (got {thud})");
    }

    /// A rain-drummed sheet must stay finite and bounded under heavy strikes.
    #[test]
    fn sheet_is_stable_under_a_downpour() {
        let sr = 48_000.0;
        let mut roof = Body::sheet(&Matter::steel(), 0.25, 0.18, 0.02, 7, sr, 7, 1.0);
        let mut rng = Noise::new(99);
        let mut peak = 0.0f32;
        for i in 0..(sr as usize * 2) {
            if i % 300 == 0 {
                roof.strike(rng.range(0.05, 0.4), rng.unit());
            }
            let (l, r) = roof.process();
            assert!(l.is_finite() && r.is_finite(), "sample {i} not finite");
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak < 5.0, "runaway amplitude {peak}");
    }
}
