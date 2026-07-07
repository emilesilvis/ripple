//! Primitive DSP math shared by every physical voice.
//!
//! Nothing in here knows what rain or wind is. These are just the honest
//! building blocks of vibration: a resonant mode, a filter, a noise source,
//! and a lumped model of a room's reflections. The *world* decides how to
//! excite them.

use std::f32::consts::TAU;

/// A fast, allocation-free noise source (xorshift). Deterministic per seed so
/// a rendered world is reproducible.
#[derive(Clone)]
pub struct Noise {
    state: u32,
}

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed | 1, // never zero
        }
    }
    #[inline]
    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
    /// Uniform white noise in [-1, 1).
    #[inline]
    pub fn white(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    /// Uniform value in [0, 1).
    #[inline]
    pub fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }
    /// Uniform value in [lo, hi).
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
    /// A coin flip that comes up true with probability `p`.
    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

/// One-pole lowpass smoother — the simplest lag: a quantity easing toward its
/// input. Used everywhere a value should change smoothly rather than jump.
#[derive(Clone, Copy)]
pub struct OnePole {
    a: f32,
    z: f32,
}

impl OnePole {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let a = 1.0 - (-TAU * cutoff_hz / sample_rate).exp();
        Self { a, z: 0.0 }
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.z += self.a * (x - self.z);
        self.z
    }
    #[inline]
    pub fn value(&self) -> f32 {
        self.z
    }
    /// Start the smoother already settled at `v` instead of easing up from 0.
    pub fn prime(&mut self, v: f32) {
        self.z = v;
    }
}

/// A general biquad. Constructed for a role (bandpass, lowpass, highpass,
/// peaking) via RBJ's cookbook formulas. `set_*` retunes it in place, cheaply
/// enough to sweep at control rate.
#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    sr: f32,
}

impl Biquad {
    pub fn bandpass(freq: f32, q: f32, sr: f32) -> Self {
        let mut f = Self::empty(sr);
        f.set_bandpass(freq, q);
        f
    }
    // Kept as part of the primitive palette even where a given world doesn't
    // reach for them.
    #[allow(dead_code)]
    pub fn lowpass(freq: f32, q: f32, sr: f32) -> Self {
        let mut f = Self::empty(sr);
        f.set_lowpass(freq, q);
        f
    }
    #[allow(dead_code)]
    pub fn highpass(freq: f32, q: f32, sr: f32) -> Self {
        let mut f = Self::empty(sr);
        f.set_highpass(freq, q);
        f
    }

    fn empty(sr: f32) -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            sr,
        }
    }

    pub fn set_bandpass(&mut self, freq: f32, q: f32) {
        let w = TAU * freq.clamp(1.0, self.sr * 0.49) / self.sr;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / (2.0 * q.max(0.01));
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b1 = 0.0;
        self.b2 = -alpha / a0;
        self.a1 = -2.0 * cs / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[allow(dead_code)]
    pub fn set_lowpass(&mut self, freq: f32, q: f32) {
        let w = TAU * freq.clamp(1.0, self.sr * 0.49) / self.sr;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / (2.0 * q.max(0.01));
        let a0 = 1.0 + alpha;
        self.b1 = (1.0 - cs) / a0;
        self.b0 = self.b1 * 0.5;
        self.b2 = self.b0;
        self.a1 = -2.0 * cs / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[allow(dead_code)]
    pub fn set_highpass(&mut self, freq: f32, q: f32) {
        let w = TAU * freq.clamp(1.0, self.sr * 0.49) / self.sr;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / (2.0 * q.max(0.01));
        let a0 = 1.0 + alpha;
        self.b1 = -(1.0 + cs) / a0;
        self.b0 = -self.b1 * 0.5;
        self.b2 = self.b0;
        self.a1 = -2.0 * cs / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// A comb filter with damping — one reflection path that recirculates and
/// loses its highs each pass, like sound bouncing in a room.
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
    damp: OnePole,
}

impl Comb {
    fn new(delay: usize, feedback: f32, damp_hz: f32, sr: f32) -> Self {
        Self {
            buf: vec![0.0; delay.max(1)],
            idx: 0,
            feedback,
            damp: OnePole::new(damp_hz, sr),
        }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let out = self.buf[self.idx];
        let fed = self.damp.process(out) * self.feedback;
        self.buf[self.idx] = x + fed;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }
}

/// An allpass diffuser — smears reflections in time without colouring them.
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    gain: f32,
}

impl Allpass {
    fn new(delay: usize, gain: f32) -> Self {
        Self {
            buf: vec![0.0; delay.max(1)],
            idx: 0,
            gain,
        }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let buffered = self.buf[self.idx];
        let y = -x + buffered;
        self.buf[self.idx] = x + buffered * self.gain;
        self.idx = (self.idx + 1) % self.buf.len();
        y
    }
}

/// A lumped model of the space around the listener: the countless small
/// reflections of the surrounding terrain, collapsed into a Schroeder reverb.
/// It is the propagation law made cheap — sound doesn't just reach the ear, it
/// arrives again and again, a little softer and darker each time.
pub struct Space {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
}

impl Space {
    pub fn new(sr: f32, size: f32) -> Self {
        // `size` scales the delays: a bigger clearing has a longer tail.
        let ms = |m: f32| ((m * size) * 0.001 * sr) as usize;
        let combs = vec![
            Comb::new(ms(29.7), 0.80, 3200.0, sr),
            Comb::new(ms(37.1), 0.79, 3000.0, sr),
            Comb::new(ms(41.1), 0.78, 2800.0, sr),
            Comb::new(ms(43.7), 0.77, 2600.0, sr),
        ];
        let allpasses = vec![Allpass::new(ms(5.0), 0.5), Allpass::new(ms(1.7), 0.5)];
        Self { combs, allpasses }
    }
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut acc = 0.0;
        for c in &mut self.combs {
            acc += c.process(x);
        }
        acc *= 0.25;
        for a in &mut self.allpasses {
            acc = a.process(acc);
        }
        acc
    }
}

/// Equal-power pan: 0.0 hard left, 1.0 hard right.
#[inline]
pub fn pan(s: f32, pos: f32) -> (f32, f32) {
    let p = pos.clamp(0.0, 1.0);
    (s * (1.0 - p).sqrt(), s * p.sqrt())
}
