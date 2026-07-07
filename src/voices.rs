//! The physical voices — the small vocabulary of ways matter makes sound.
//!
//! There are only a few. A struck solid rings (`Modal`). A moving fluid
//! hisses and roars (`Turbulence`). A living thing drives a limit cycle
//! (`Critter`, in its own module). Every soundscape in this world is these
//! voices, excited by the physics.

use crate::dsp::{Biquad, Mode, Noise, OnePole};

// ---------------------------------------------------------------------------
// Modal — the voice of a struck solid
// ---------------------------------------------------------------------------

/// A material: the set of modes an object rings with when struck, plus how
/// bright and gritty the moment of contact is. This is *all* that separates a
/// tin roof from a wooden log from a glass chime.
#[derive(Clone)]
pub struct Material {
    /// (frequency Hz, decay seconds, relative gain) for each mode.
    pub modes: Vec<(f32, f32, f32)>,
    /// Centre of the broadband "contact" click at the moment of impact.
    pub contact_hz: f32,
    /// How long the contact click lasts (seconds).
    pub contact_decay_secs: f32,
    /// Relative loudness of the contact click vs. the ring.
    pub contact_amp: f32,
}

impl Material {
    /// A thin metal panel: a few inharmonic high modes, short bright ring — a
    /// tin roof, a bucket, a sheet of steel.
    pub fn tin() -> Self {
        Self {
            modes: vec![
                (2100.0, 0.10, 1.0),
                (3170.0, 0.08, 0.7),
                (4600.0, 0.06, 0.5),
                (5900.0, 0.045, 0.3),
            ],
            contact_hz: 3800.0,
            contact_decay_secs: 0.004,
            contact_amp: 0.6,
        }
    }
    /// Wood: low, quick, woody thuds — a shifting log, a snapping twig.
    pub fn wood() -> Self {
        Self {
            modes: vec![
                (280.0, 0.06, 1.0),
                (540.0, 0.045, 0.6),
                (950.0, 0.03, 0.35),
            ],
            contact_hz: 1500.0,
            contact_decay_secs: 0.006,
            contact_amp: 0.8,
        }
    }
    /// A tuned chime tube: a near-harmonic, long-ringing voice. `freq` is its
    /// fundamental; a chime set is several of these at pentatonic pitches.
    pub fn chime(freq: f32) -> Self {
        Self {
            modes: vec![
                (freq, 3.5, 1.0),
                (freq * 2.76, 2.2, 0.4), // struck-bar overtones lean inharmonic
                (freq * 5.40, 1.4, 0.15),
            ],
            contact_hz: freq * 6.0,
            contact_decay_secs: 0.002,
            contact_amp: 0.15,
        }
    }
    /// Water's surface: a soft, low, fast-damped plip with almost no ring —
    /// what a drop finds when it lands in the pond.
    pub fn water_surface() -> Self {
        Self {
            modes: vec![(620.0, 0.02, 1.0), (1250.0, 0.012, 0.4)],
            contact_hz: 900.0,
            contact_decay_secs: 0.005,
            contact_amp: 1.0,
        }
    }
    /// Soft ground / leaf litter: nearly no ring, just a damp pat.
    #[allow(dead_code)]
    pub fn earth() -> Self {
        Self {
            modes: vec![(180.0, 0.015, 1.0)],
            contact_hz: 2600.0,
            contact_decay_secs: 0.003,
            contact_amp: 1.0,
        }
    }
}

/// A struck solid. Excite it with `strike` and it rings; many overlapping
/// strikes simply superpose, because a real object's response is linear.
/// Renders in stereo by running two faintly detuned copies of the material.
pub struct Modal {
    modes_l: Vec<Mode>,
    modes_r: Vec<Mode>,
    gains: Vec<f32>,
    // Pending impulse energy for this sample, per channel.
    excite_l: f32,
    excite_r: f32,
    // Broadband contact click, per channel (a decaying noise burst).
    contact_l: f32,
    contact_r: f32,
    contact_decay: f32,
    contact_amp: f32,
    contact_bp: Biquad,
    noise: Noise,
    out_gain: f32,
}

impl Modal {
    pub fn new(material: &Material, sr: f32, seed: u32, out_gain: f32) -> Self {
        let modes_l: Vec<Mode> = material
            .modes
            .iter()
            .map(|(f, d, _)| Mode::new(*f, *d, sr))
            .collect();
        // Right channel detuned a touch for natural width.
        let modes_r: Vec<Mode> = material
            .modes
            .iter()
            .map(|(f, d, _)| Mode::new(*f * 1.006, *d, sr))
            .collect();
        let gains = material.modes.iter().map(|(_, _, g)| *g).collect();
        Self {
            modes_l,
            modes_r,
            gains,
            excite_l: 0.0,
            excite_r: 0.0,
            contact_l: 0.0,
            contact_r: 0.0,
            contact_decay: (-1.0 / (material.contact_decay_secs * sr)).exp(),
            contact_amp: material.contact_amp,
            contact_bp: Biquad::bandpass(material.contact_hz, 1.2, sr),
            noise: Noise::new(seed ^ 0x51ed_270b),
            out_gain,
        }
    }

    /// Deposit energy into the object at a stereo position (0 = left, 1 = right).
    #[inline]
    pub fn strike(&mut self, energy: f32, pan: f32) {
        let p = pan.clamp(0.0, 1.0);
        let gl = (1.0 - p).sqrt();
        let gr = p.sqrt();
        self.excite_l += energy * gl;
        self.excite_r += energy * gr;
        self.contact_l += energy * gl;
        self.contact_r += energy * gr;
    }

    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let mut l = 0.0;
        let mut r = 0.0;
        for i in 0..self.gains.len() {
            let g = self.gains[i];
            l += self.modes_l[i].process(self.excite_l) * g;
            r += self.modes_r[i].process(self.excite_r) * g;
        }
        self.excite_l = 0.0;
        self.excite_r = 0.0;

        // Contact click: brief filtered-noise burst at the moment of impact.
        if self.contact_l + self.contact_r > 1e-6 {
            let n = self.noise.white();
            let c = self.contact_bp.process(n) * self.contact_amp;
            l += c * self.contact_l;
            r += c * self.contact_r;
            self.contact_l *= self.contact_decay;
            self.contact_r *= self.contact_decay;
        }

        (l * self.out_gain, r * self.out_gain)
    }
}

// ---------------------------------------------------------------------------
// Turbulence — the voice of a moving fluid
// ---------------------------------------------------------------------------

/// How a body of moving fluid sounds: a band of noise whose centre rises with
/// flow speed (faster eddies are smaller and higher) and whose loudness rises
/// with flow energy. Air through leaves, water over rock, foam up a beach,
/// the airy roar of a flame — all the same voice, different bands.
#[derive(Clone)]
pub struct FlowVoicing {
    pub base_hz: f32,   // centre at rest / slow flow
    pub speed_hz: f32,  // extra centre frequency per unit speed
    pub q: f32,         // resonance of the eddy band
    pub darken_hz: f32, // a lowpass over the top, for body
}

impl FlowVoicing {
    pub fn wind() -> Self {
        Self { base_hz: 320.0, speed_hz: 380.0, q: 0.6, darken_hz: 2200.0 }
    }
    pub fn leaves() -> Self {
        Self { base_hz: 4200.0, speed_hz: 2600.0, q: 0.4, darken_hz: 9000.0 }
    }
    pub fn water() -> Self {
        Self { base_hz: 700.0, speed_hz: 1500.0, q: 1.1, darken_hz: 3200.0 }
    }
    pub fn surf() -> Self {
        Self { base_hz: 900.0, speed_hz: 2200.0, q: 0.4, darken_hz: 5000.0 }
    }
    pub fn flame() -> Self {
        Self { base_hz: 180.0, speed_hz: 120.0, q: 0.7, darken_hz: 900.0 }
    }
}

/// A continuously radiating patch of turbulent fluid. Feed it a flow `energy`
/// (loudness) and `speed` (brightness) each control block; it hisses onward.
pub struct Turbulence {
    voicing: FlowVoicing,
    bp_l: Biquad,
    bp_r: Biquad,
    lp_l: OnePole,
    lp_r: OnePole,
    noise_l: Noise,
    noise_r: Noise,
    level: OnePole,   // smoothed loudness
    center: OnePole,  // smoothed brightness (Hz)
    gain: f32,
}

impl Turbulence {
    pub fn new(voicing: FlowVoicing, sr: f32, seed: u32, gain: f32) -> Self {
        let base = voicing.base_hz;
        let darken = voicing.darken_hz;
        let q = voicing.q;
        Self {
            bp_l: Biquad::bandpass(base, q, sr),
            bp_r: Biquad::bandpass(base * 1.05, q, sr),
            lp_l: OnePole::new(darken, sr),
            lp_r: OnePole::new(darken, sr),
            noise_l: Noise::new(seed ^ 0x1234_abcd),
            noise_r: Noise::new(seed ^ 0x8765_4321),
            level: OnePole::new(4.0, sr),
            center: OnePole::new(3.0, sr),
            gain,
            voicing,
        }
    }

    /// Set the flow driving this patch (called at control rate). `energy` is a
    /// 0..1-ish loudness; `speed` a 0..1-ish flow rate.
    #[inline]
    pub fn drive(&mut self, energy: f32, speed: f32) {
        self.level.process(energy);
        let target = self.voicing.base_hz + self.voicing.speed_hz * speed;
        let c = self.center.process(target);
        self.bp_l.set_bandpass(c, self.voicing.q);
        self.bp_r.set_bandpass(c * 1.05, self.voicing.q);
    }

    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let lvl = self.level.value();
        if lvl < 1e-5 {
            return (0.0, 0.0);
        }
        let nl = self.noise_l.white();
        let nr = self.noise_r.white();
        // A bandpass over white noise passes only a sliver of its energy, so a
        // makeup factor brings a driven flow up to a natural, useful level.
        const MAKEUP: f32 = 3.0;
        let l = self.lp_l.process(self.bp_l.process(nl)) * lvl * self.gain * MAKEUP;
        let r = self.lp_r.process(self.bp_r.process(nr)) * lvl * self.gain * MAKEUP;
        (l, r)
    }
}
