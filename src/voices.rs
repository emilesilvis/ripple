//! The voice of moving fluid, driven by law rather than by ear.
//!
//! A turbulent source keeps a single physical datum: the size of the
//! eddies that do its radiating. Everything audible follows from two laws:
//!
//! - **Strouhal.** Eddies of scale `l` shed by flow at speed `v` fluctuate
//!   around `f = St * v / l`, St ~ 0.2. Faster flow, higher voice — for every
//!   fluid, with the same constant.
//! - **Turbulent radiation.** Radiated energy climbs steeply with flow speed;
//!   the world hands each source its flow energy and the band simply follows.
//!
//! Why the scales are small (fractions of a millimetre to millimetres):
//! radiation efficiency rises steeply with frequency, so of the whole eddy
//! cascade it is the smallest, fastest eddies — at leaf edges, at the water's
//! broken surface — that the ear actually receives. The band is the honest
//! rendering of an unresolved cascade: broadband noise centred where the law
//! puts it.

use crate::dsp::{Biquad, Noise, OnePole};

/// The Strouhal number — one constant for every fluid in the world.
pub const STROUHAL: f32 = 0.2;
/// The width of a free-shear eddy band (no geometry to sharpen it).
const Q_FREE: f32 = 0.7;
/// The air between source and ear absorbs the highest frequencies —
/// one universal lowpass, the same for every source.
const AIR_ABSORB_HZ: f32 = 6500.0;

/// A turbulent source's physical description.
#[derive(Clone, Copy)]
pub struct Eddies {
    /// Size of the radiating eddies, metres.
    pub scale_m: f32,
    /// Flow speed in m/s when the driver reports speed = 1.
    pub v_ref: f32,
    /// Band resonance. Free shear flow is broad; a resonant aperture
    /// (a whistling crack) sharpens the same law into a tone.
    pub q: f32,
}

impl Eddies {
    /// Turbulence with nothing to resonate against: wind, water, foam, flame.
    pub fn free_shear(scale_m: f32, v_ref: f32) -> Self {
        Self { scale_m, v_ref, q: Q_FREE }
    }
    /// Flow through an opening that answers back — an edge tone. The same
    /// Strouhal law picks the pitch; the geometry supplies the resonance.
    pub fn aperture(scale_m: f32, v_ref: f32, q: f32) -> Self {
        Self { scale_m, v_ref, q }
    }
}

/// A continuously radiating patch of turbulent fluid. Feed it a flow `energy`
/// (loudness) and `speed` (0..1 of `v_ref`) each control block; the Strouhal
/// law places its voice.
pub struct Turbulence {
    eddies: Eddies,
    sr: f32,
    bp_l: Biquad,
    bp_r: Biquad,
    lp_l: OnePole,
    lp_r: OnePole,
    noise_l: Noise,
    noise_r: Noise,
    level: OnePole,  // smoothed loudness
    center: OnePole, // smoothed band centre (Hz)
    gain: f32,
}

impl Turbulence {
    pub fn new(eddies: Eddies, sr: f32, seed: u32, gain: f32) -> Self {
        let f0 = (STROUHAL * eddies.v_ref * 0.5 / eddies.scale_m).clamp(25.0, sr * 0.42);
        let mut center = OnePole::new(3.0, sr);
        center.prime(f0);
        Self {
            bp_l: Biquad::bandpass(f0, eddies.q, sr),
            bp_r: Biquad::bandpass(f0 * 1.05, eddies.q, sr),
            lp_l: OnePole::new(AIR_ABSORB_HZ, sr),
            lp_r: OnePole::new(AIR_ABSORB_HZ, sr),
            noise_l: Noise::new(seed ^ 0x1234_abcd),
            noise_r: Noise::new(seed ^ 0x8765_4321),
            level: OnePole::new(4.0, sr),
            center,
            gain,
            eddies,
            sr,
        }
    }

    /// Set the flow driving this patch (called at control rate). `energy` is a
    /// 0..1-ish loudness; `speed` a 0..1 fraction of the reference flow.
    #[inline]
    pub fn drive(&mut self, energy: f32, speed: f32) {
        self.level.process(energy);
        // Strouhal: the eddies' frequency follows the flow.
        let v = speed.max(0.0) * self.eddies.v_ref;
        let target = (STROUHAL * v / self.eddies.scale_m).clamp(25.0, self.sr * 0.42);
        let c = self.center.process(target);
        self.bp_l.set_bandpass(c, self.eddies.q);
        self.bp_r.set_bandpass(c * 1.05, self.eddies.q);
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
        // The band's *placement* must shape the tone, not the loudness: a
        // fixed-Q band lower down is narrower in absolute Hz, so compensate,
        // keeping radiated power true to the flow energy alone.
        const MAKEUP: f32 = 3.0;
        let c = self.center.value().max(50.0);
        let bw = (1200.0 / c).sqrt().clamp(0.5, 3.0);
        let g = lvl * self.gain * MAKEUP * bw;
        let l = self.lp_l.process(self.bp_l.process(nl)) * g;
        let r = self.lp_r.process(self.bp_r.process(nr)) * g;
        (l, r)
    }
}
