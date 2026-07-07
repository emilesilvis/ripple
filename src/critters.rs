//! The living voice — small agents that drive an oscillator to sing.
//!
//! A critter is a nonlinear oscillator (a voice) plus a behavioural clock. On
//! its own it just sings now and then. The interesting part is the *chorus*:
//! when many critters gently kick each other's clocks, order emerges — crickets
//! fall into rhythm, frogs take turns. That synchrony is not scripted; it is
//! the same pulse-coupling that syncs fireflies, left to run.

use crate::dsp::{pan, Noise};
use std::f32::consts::{PI, TAU};

/// A species: the fixed parameters that make a cricket a cricket. Timing is in
/// seconds, pitch in Hz, ratios relative to the carrier.
#[derive(Clone)]
pub struct Species {
    pub carrier_lo: f32,
    pub carrier_hi: f32,
    pub phrase_gap_lo: f32,
    pub phrase_gap_hi: f32,
    pub syllables: (usize, usize),
    pub syl_dur_lo: f32,
    pub syl_dur_hi: f32,
    pub syl_gap_lo: f32,
    pub syl_gap_hi: f32,
    pub sweep_lo: f32,
    pub sweep_hi: f32,
    pub pulse_rate: f32, // 0 = pure tone; >0 = amplitude-pulsed (tick-tick)
    pub harmonics: Vec<(f32, f32)>,
    pub vib_rate: f32,
    pub vib_depth: f32,
    pub amp_lo: f32,
    pub amp_hi: f32,
    pub reverb_send: f32,
}

impl Species {
    pub fn cricket() -> Self {
        Self {
            carrier_lo: 3900.0,
            carrier_hi: 4800.0,
            // Fairly uniform natural tempo, so the coupling can win and the
            // chorus falls into rhythm.
            phrase_gap_lo: 0.7,
            phrase_gap_hi: 1.0,
            syllables: (1, 1),
            syl_dur_lo: 0.22,
            syl_dur_hi: 0.34,
            syl_gap_lo: 0.15,
            syl_gap_hi: 0.4,
            sweep_lo: 1.0,
            sweep_hi: 1.0,
            pulse_rate: 26.0,
            harmonics: vec![(2.0, 0.15)],
            vib_rate: 0.0,
            vib_depth: 0.0,
            amp_lo: 0.05,
            amp_hi: 0.10,
            reverb_send: 0.05,
        }
    }
    pub fn frog() -> Self {
        Self {
            carrier_lo: 220.0,
            carrier_hi: 430.0,
            phrase_gap_lo: 2.5,
            phrase_gap_hi: 8.0,
            syllables: (1, 2),
            syl_dur_lo: 0.18,
            syl_dur_hi: 0.45,
            syl_gap_lo: 0.12,
            syl_gap_hi: 0.3,
            sweep_lo: 0.86,
            sweep_hi: 0.92,
            pulse_rate: 30.0,
            harmonics: vec![(2.0, 0.3)],
            vib_rate: 0.0,
            vib_depth: 0.0,
            amp_lo: 0.16,
            amp_hi: 0.28,
            reverb_send: 0.12,
        }
    }
    pub fn songbird() -> Self {
        Self {
            carrier_lo: 1800.0,
            carrier_hi: 4200.0,
            phrase_gap_lo: 1.5,
            phrase_gap_hi: 7.0,
            syllables: (2, 6),
            syl_dur_lo: 0.04,
            syl_dur_hi: 0.14,
            syl_gap_lo: 0.05,
            syl_gap_hi: 0.16,
            sweep_lo: 0.75,
            sweep_hi: 1.4,
            pulse_rate: 0.0,
            harmonics: vec![],
            vib_rate: 30.0,
            vib_depth: 0.06,
            amp_lo: 0.10,
            amp_hi: 0.20,
            reverb_send: 0.18,
        }
    }
    pub fn gull() -> Self {
        Self {
            carrier_lo: 1100.0,
            carrier_hi: 2000.0,
            phrase_gap_lo: 4.0,
            phrase_gap_hi: 14.0,
            syllables: (1, 3),
            syl_dur_lo: 0.35,
            syl_dur_hi: 0.7,
            syl_gap_lo: 0.3,
            syl_gap_hi: 0.9,
            sweep_lo: 0.62,
            sweep_hi: 0.78,
            pulse_rate: 0.0,
            harmonics: vec![(2.0, 0.35), (3.0, 0.15)],
            vib_rate: 6.0,
            vib_depth: 0.03,
            amp_lo: 0.12,
            amp_hi: 0.22,
            reverb_send: 0.22,
        }
    }
    pub fn owl() -> Self {
        Self {
            carrier_lo: 320.0,
            carrier_hi: 430.0,
            phrase_gap_lo: 14.0,
            phrase_gap_hi: 40.0,
            syllables: (2, 4),
            syl_dur_lo: 0.25,
            syl_dur_hi: 0.5,
            syl_gap_lo: 0.35,
            syl_gap_hi: 0.8,
            sweep_lo: 0.95,
            sweep_hi: 0.95,
            pulse_rate: 0.0,
            harmonics: vec![(2.0, 0.18)],
            vib_rate: 0.0,
            vib_depth: 0.0,
            amp_lo: 0.20,
            amp_hi: 0.28,
            reverb_send: 0.3,
        }
    }
}

#[derive(Clone, Copy)]
struct Syllable {
    delay: u32,
    freq: f32,
    dur: f32,
    sweep: f32,
}

const NO_SYLLABLE: Syllable = Syllable {
    delay: 0,
    freq: 0.0,
    dur: 0.0,
    sweep: 1.0,
};

/// One agent. Holds its fixed voice, a behavioural clock (`phrase_phase`),
/// and whatever syllable it is currently singing.
pub struct Critter {
    sr: f32,
    rng: Noise,
    sp: Species,
    // Behavioural clock: rises 0->1, then the critter sings and it resets.
    phrase_phase: f32,
    phrase_inc: f32,
    pan: f32,
    // Currently sounding syllable.
    active: bool,
    t: f32,
    dur: f32,
    phase: f32,
    freq: f32,
    sweep: f32,
    amp: f32,
    pulse_phase: f32,
    pulse_inc: f32,
    // Syllables scheduled later in this phrase.
    pending: [Syllable; 8],
    pending_len: usize,
}

impl Critter {
    pub fn new(sp: Species, sr: f32, seed: u32) -> Self {
        let mut rng = Noise::new(seed);
        let period = rng.range(sp.phrase_gap_lo, sp.phrase_gap_hi);
        let pan = rng.range(0.08, 0.92);
        Self {
            sr,
            phrase_phase: rng.unit(), // desync the start
            phrase_inc: 1.0 / (period * sr),
            pan,
            active: false,
            t: 0.0,
            dur: 0.0,
            phase: 0.0,
            freq: 0.0,
            sweep: 1.0,
            amp: 0.0,
            pulse_phase: 0.0,
            pulse_inc: 0.0,
            pending: [NO_SYLLABLE; 8],
            pending_len: 0,
            rng,
            sp,
        }
    }

    /// The behavioural clock's position — read by the chorus for coupling.
    #[inline]
    pub fn phase(&self) -> f32 {
        self.phrase_phase
    }

    /// A neighbour nudges this critter's clock (pulse coupling). Positive pulls
    /// it toward firing (crickets synchronise); negative pushes it back (frogs
    /// take turns).
    #[inline]
    pub fn kick(&mut self, amount: f32) {
        if !self.active {
            self.phrase_phase = (self.phrase_phase + amount).clamp(0.0, 0.999);
        }
    }

    /// Begin a phrase: choose a voice for it and lay out its syllables.
    fn start_phrase(&mut self) {
        let base = self.rng.range(self.sp.carrier_lo, self.sp.carrier_hi);
        self.amp = self.rng.range(self.sp.amp_lo, self.sp.amp_hi);
        self.pan = self.rng.range(0.08, 0.92);
        let n = if self.sp.syllables.1 > self.sp.syllables.0 {
            self.sp.syllables.0
                + (self.rng.unit() * (self.sp.syllables.1 - self.sp.syllables.0 + 1) as f32)
                    as usize
        } else {
            self.sp.syllables.0
        };
        let mut at = 0.0f32;
        self.pending_len = 0;
        for k in 0..n.max(1) {
            let syl = Syllable {
                delay: (at * self.sr) as u32,
                freq: base * self.rng.range(0.94, 1.06),
                dur: self.rng.range(self.sp.syl_dur_lo, self.sp.syl_dur_hi),
                sweep: self.rng.range(self.sp.sweep_lo, self.sp.sweep_hi),
            };
            if k == 0 {
                self.begin_syllable(syl);
            } else if self.pending_len < self.pending.len() {
                self.pending[self.pending_len] = syl;
                self.pending_len += 1;
            }
            at += syl.dur + self.rng.range(self.sp.syl_gap_lo, self.sp.syl_gap_hi);
        }
        // Choose how long until the next phrase.
        let period = self.rng.range(self.sp.phrase_gap_lo, self.sp.phrase_gap_hi);
        self.phrase_inc = 1.0 / (period * self.sr);
    }

    fn begin_syllable(&mut self, syl: Syllable) {
        self.active = true;
        self.t = 0.0;
        self.dur = syl.dur;
        self.phase = 0.0;
        self.freq = syl.freq;
        self.sweep = syl.sweep;
        self.pulse_phase = 0.0;
        self.pulse_inc = TAU * self.sp.pulse_rate / self.sr;
    }

    pub fn process(&mut self) -> (f32, f32) {
        // Release any scheduled syllables whose delay has elapsed.
        let mut i = 0;
        while i < self.pending_len {
            if self.pending[i].delay == 0 {
                let syl = self.pending[i];
                self.pending[i] = self.pending[self.pending_len - 1];
                self.pending_len -= 1;
                self.begin_syllable(syl);
            } else {
                self.pending[i].delay -= 1;
                i += 1;
            }
        }

        // Advance the behavioural clock; fire a phrase when it wraps.
        self.phrase_phase += self.phrase_inc;
        if self.phrase_phase >= 1.0 {
            self.phrase_phase -= 1.0;
            if !self.active && self.pending_len == 0 {
                self.start_phrase();
            }
        }

        if !self.active {
            return (0.0, 0.0);
        }

        let x = self.t / self.dur;
        let env = (PI * x).sin(); // gentle rise and fall, no edges
        let vib = if self.sp.vib_depth > 0.0 {
            1.0 + self.sp.vib_depth * (TAU * self.sp.vib_rate * self.t).sin()
        } else {
            1.0
        };
        let freq = self.freq * (1.0 + (self.sweep - 1.0) * x) * vib;

        // Carrier plus any harmonics.
        let mut s = self.phase.sin();
        for (ratio, gain) in &self.sp.harmonics {
            s += (self.phase * ratio).sin() * gain;
        }

        // Optional amplitude pulse train (the cricket/frog "tick-tick").
        let gate = if self.sp.pulse_rate > 0.0 {
            let g = self.pulse_phase.sin().max(0.0);
            self.pulse_phase += self.pulse_inc;
            g * g * g
        } else {
            1.0
        };

        let out = s * env * gate * self.amp;
        self.phase += TAU * freq / self.sr;
        self.t += 1.0 / self.sr;
        if self.t >= self.dur {
            self.active = false;
        }
        pan(out, self.pan)
    }
}

/// A population of one species that couples to itself. Stepping the chorus
/// advances every critter and lets each firing gently kick the others.
pub struct Chorus {
    critters: Vec<Critter>,
    coupling: f32,
    reverb_send: f32,
    // Snapshot of who is about to fire, so a kick within a block is coherent.
    was_high: Vec<bool>,
}

impl Chorus {
    pub fn new(sp: Species, count: usize, coupling: f32, sr: f32, seed: u32) -> Self {
        let reverb_send = sp.reverb_send;
        let critters = (0..count)
            .map(|i| Critter::new(sp.clone(), sr, seed.wrapping_add(i as u32 * 2_654_435_761)))
            .collect();
        Self {
            critters,
            coupling,
            reverb_send,
            was_high: vec![false; count],
        }
    }

    pub fn reverb_send(&self) -> f32 {
        self.reverb_send
    }

    /// The Kuramoto order parameter of the flock's behavioural clocks: 0 when
    /// their phases are scattered, 1 when they fire as one. Watching this rise
    /// is watching the chorus find its rhythm.
    pub fn order(&self) -> f32 {
        let n = self.critters.len();
        if n == 0 {
            return 0.0;
        }
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        for c in &self.critters {
            let a = std::f32::consts::TAU * c.phase();
            sx += a.cos();
            sy += a.sin();
        }
        ((sx * sx + sy * sy).sqrt()) / n as f32
    }

    pub fn process(&mut self) -> (f32, f32) {
        // Detect fresh firings (clock just wrapped past the near-fire mark) and
        // kick everyone else. Positive coupling pulls the flock together;
        // negative makes them alternate.
        let n = self.critters.len();
        let mut fired: Option<usize> = None;
        for i in 0..n {
            let p = self.critters[i].phase();
            let high = p > 0.92;
            if high && !self.was_high[i] {
                fired = Some(i);
            }
            self.was_high[i] = high;
        }
        if let Some(f) = fired {
            if self.coupling.abs() > 1e-6 {
                for j in 0..n {
                    if j != f {
                        self.critters[j].kick(self.coupling);
                    }
                }
            }
        }

        let mut l = 0.0;
        let mut r = 0.0;
        for c in &mut self.critters {
            let (x, y) = c.process();
            l += x;
            r += y;
        }
        (l, r)
    }
}
