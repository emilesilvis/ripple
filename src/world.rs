//! The world: the field of matter, the sky above it, and the voices through
//! which their motion becomes sound. Presets assemble a world by laying out
//! terrain and switching on whatever belongs there; the world then runs itself.
//!
//! Two clocks turn here. The physics advances in short control blocks (water
//! flows, weather drifts, drops land, embers pop). Between those, every sample
//! is a reading of the world as it stands — the resonators still ringing, the
//! turbulence still hissing — gathered at two ears and returned as stereo.

use crate::critters::{Chorus, Species};
use crate::dsp::{pan, Noise, Space};
use crate::field::{Event, Field, Surface};
use crate::sky::{Climate, Sky};
use crate::voices::{FlowVoicing, Material, Modal, Turbulence};
use std::f32::consts::TAU;

const BLOCK: usize = 32; // samples between physics updates (~1.5 kHz control rate)

/// A little rising bubble, air escaping churning water.
#[derive(Clone, Copy)]
struct Bubble {
    active: bool,
    phase: f32,
    freq: f32,
    freq_rise: f32,
    amp: f32,
    decay: f32,
    pan: f32,
    sr: f32,
}

impl Bubble {
    fn inactive(sr: f32) -> Self {
        Self { active: false, phase: 0.0, freq: 0.0, freq_rise: 0.0, amp: 0.0, decay: 0.0, pan: 0.5, sr }
    }
    fn spawn(&mut self, freq: f32, energy: f32, pan: f32, rng: &mut Noise) {
        let dur = rng.range(0.015, 0.06);
        self.active = true;
        self.phase = 0.0;
        self.freq = freq;
        self.freq_rise = freq * rng.range(1.5, 3.5) / (dur * self.sr);
        self.amp = energy;
        self.decay = (-1.0 / (dur * self.sr)).exp();
        self.pan = pan;
    }
    fn process(&mut self) -> (f32, f32) {
        if !self.active {
            return (0.0, 0.0);
        }
        let s = self.phase.sin() * self.amp;
        self.phase += TAU * self.freq / self.sr;
        self.freq += self.freq_rise;
        self.amp *= self.decay;
        if self.amp < 1e-5 {
            self.active = false;
        }
        pan(s, self.pan)
    }
}

/// One resonant object that hangs in the air and is struck by the wind (a
/// chime). Its pitch is the material; the timing comes from the gusts.
struct Chime {
    modal: Modal,
    pan: f32,
}

/// A critter chorus with its gain and when it is awake.
struct Voices {
    chorus: Chorus,
    gain: f32,
    /// 0 = sings by day, 1 = sings by night, values between blend.
    nocturnal: f32,
}

/// A send into the shared reverb: (dry gain already applied, send amount).
pub struct World {
    sr: f32,
    seed: u32,
    rng: Noise,
    block_pos: usize,

    field: Field,
    sky: Sky,

    // Continuous fluid voices.
    stream: Option<(Turbulence, f32)>, // (voice, reverb send)
    wind: Option<Turbulence>,
    leaves: Option<Turbulence>,
    surf: Option<Turbulence>,
    flame: Option<Turbulence>,

    // Struck-solid voices.
    roof: Option<Modal>,   // rain drumming on a roof
    ground: Option<Modal>, // rain landing on wet ground / pond
    fire_wood: Option<Modal>,
    chimes: Vec<Chime>,
    chime_gain: f32,

    // Wind whistle band (resonant turbulence through an aperture).
    whistle: Option<Turbulence>,

    // Fire state (a small heat engine).
    fire_activity: f32,
    fire_activity_target: f32,
    fire_activity_timer: f32,
    fire_gain: f32,

    // Transient pools.
    bubbles: Vec<Bubble>,

    // Living voices.
    choruses: Vec<Voices>,

    // Rain routing.
    rain_gain: f32,

    // The space around the listener.
    space_l: Space,
    space_r: Space,
    reverb_mix: f32,

    // Gentle startup fade.
    fade: f32,
    fade_step: f32,
}

impl World {
    pub fn new(sr: f32, seed: u32, climate: Climate, field_w: usize, field_h: usize, cell: f32) -> Self {
        Self {
            sr,
            seed,
            rng: Noise::new(seed ^ 0xa11c_e5),
            block_pos: 0,
            field: Field::new(field_w, field_h, cell, seed),
            sky: Sky::new(climate, sr, seed),
            stream: None,
            wind: None,
            leaves: None,
            surf: None,
            flame: None,
            roof: None,
            ground: None,
            fire_wood: None,
            chimes: Vec::new(),
            chime_gain: 0.0,
            whistle: None,
            fire_activity: 0.6,
            fire_activity_target: 0.6,
            fire_activity_timer: 0.0,
            fire_gain: 0.0,
            bubbles: vec![Bubble::inactive(sr); 24],
            choruses: Vec::new(),
            rain_gain: 0.0,
            space_l: Space::new(sr, 1.0),
            space_r: Space::new(sr, 1.15),
            reverb_mix: 0.5,
            fade: 0.0,
            fade_step: 1.0 / (3.0 * sr),
        }
    }

    // --- assembly (called by presets) --------------------------------------

    pub fn field_mut(&mut self) -> &mut Field {
        &mut self.field
    }

    pub fn set_reverb(&mut self, size: f32, mix: f32) {
        self.space_l = Space::new(self.sr, size);
        self.space_r = Space::new(self.sr, size * 1.15);
        self.reverb_mix = mix;
    }

    pub fn enable_stream(&mut self, gain: f32) {
        self.stream = Some((Turbulence::new(FlowVoicing::water(), self.sr, self.seed ^ 0x57, gain), 0.06));
    }
    pub fn enable_surf(&mut self, gain: f32) {
        self.surf = Some(Turbulence::new(FlowVoicing::surf(), self.sr, self.seed ^ 0x5f, gain));
    }
    pub fn enable_wind(&mut self, gain: f32) {
        self.wind = Some(Turbulence::new(FlowVoicing::wind(), self.sr, self.seed ^ 0x1d, gain));
        self.whistle = Some(Turbulence::new(
            FlowVoicing { base_hz: 1700.0, speed_hz: 700.0, q: 9.0, darken_hz: 4000.0 },
            self.sr,
            self.seed ^ 0x1e,
            gain * 0.5,
        ));
    }
    pub fn enable_leaves(&mut self, gain: f32) {
        self.leaves = Some(Turbulence::new(FlowVoicing::leaves(), self.sr, self.seed ^ 0x1f, gain));
    }
    pub fn enable_fire(&mut self, gain: f32) {
        self.flame = Some(Turbulence::new(FlowVoicing::flame(), self.sr, self.seed ^ 0xf1, gain));
        self.fire_wood = Some(Modal::new(&Material::wood(), self.sr, self.seed ^ 0xf2, gain * 1.4));
        self.fire_gain = gain;
    }
    pub fn enable_rain(&mut self, gain: f32, on_roof: bool) {
        self.rain_gain = gain;
        if on_roof {
            self.roof = Some(Modal::new(&Material::tin(), self.sr, self.seed ^ 0x2a, gain * 0.9));
        }
        self.ground = Some(Modal::new(&Material::water_surface(), self.sr, self.seed ^ 0x2b, gain * 0.8));
    }
    pub fn add_chimes(&mut self, freqs: &[f32], gain: f32) {
        self.chime_gain = gain;
        for (i, f) in freqs.iter().enumerate() {
            let modal = Modal::new(&Material::chime(*f), self.sr, self.seed ^ (0x3a + i as u32), gain);
            self.chimes.push(Chime { modal, pan: 0.15 + 0.7 * (i as f32 / freqs.len().max(1) as f32) });
        }
    }
    pub fn add_chorus(&mut self, sp: Species, count: usize, coupling: f32, gain: f32, nocturnal: f32) {
        let chorus = Chorus::new(sp, count, coupling, self.sr, self.seed.wrapping_add(0x600d + count as u32));
        self.choruses.push(Voices { chorus, gain, nocturnal });
    }

    // --- the running world -------------------------------------------------

    fn step_controls(&mut self) {
        let dt = BLOCK as f32 / self.sr;
        self.sky.step(dt);
        self.field.step(dt);

        // Rain: rainfall becomes falling drops that land on whatever is below.
        if self.rain_gain > 0.0 {
            let rate = self.sky.rain() * 95.0; // drops per second at full rain
            let expected = rate * dt;
            let mut n = expected.floor() as i32;
            if self.rng.unit() < expected.fract() {
                n += 1;
            }
            for _ in 0..n {
                let nx = self.rng.unit();
                let ny = self.rng.unit();
                let energy = self.rng.range(0.05, 0.32);
                match self.field.surface_at(nx, ny) {
                    Surface::Roof => {
                        if let Some(roof) = &mut self.roof {
                            roof.strike(energy, nx);
                        }
                    }
                    Surface::Open | Surface::Rock => {
                        // The drop soaks in — this is how rain swells the brook.
                        self.field.add_water(nx, ny, energy * 0.006);
                        if let Some(ground) = &mut self.ground {
                            ground.strike(energy * 0.7, nx);
                        }
                    }
                }
            }
        }

        // Water's own voices: churn drives the stream band; breaking waves
        // drive the surf band.
        let (flow_e, flow_s) = self.field.flow();
        if let Some((stream, _)) = &mut self.stream {
            stream.drive(flow_e, flow_s);
        }
        if let Some(surf) = &mut self.surf {
            // The surf bed follows the shifting mass of water; breaks add to it.
            surf.drive(flow_e * 0.6 + 0.12, flow_s);
        }

        // Field events (bubbles in the brook, foam at the shore).
        let events: Vec<Event> = self.field.drain_events().collect();
        for ev in events {
            match ev {
                Event::Bubble { pan, freq, energy } => {
                    if let Some(b) = self.bubbles.iter_mut().find(|b| !b.active) {
                        b.spawn(freq, energy, pan, &mut self.rng);
                    }
                }
                Event::Break { pan, energy } => {
                    if let Some(surf) = &mut self.surf {
                        // A break is a sudden surge in the foam.
                        surf.drive(energy + 0.2, 0.9);
                    }
                    let _ = pan;
                }
            }
        }

        // Wind and leaves ride the sky's air speed.
        let air = self.sky.air_speed();
        let gust = self.sky.gust();
        if let Some(wind) = &mut self.wind {
            wind.drive(0.15 + 0.85 * air, air);
        }
        if let Some(whistle) = &mut self.whistle {
            // The whistle only speaks on the strongest gusts.
            let w = (gust * gust * gust) * air;
            whistle.drive(w, air);
        }
        if let Some(leaves) = &mut self.leaves {
            leaves.drive(0.1 + 0.9 * air * gust, air);
        }

        // Wind strikes the chimes: a strong gust nudges them into voice.
        if !self.chimes.is_empty() && gust > 0.6 {
            let p = (gust - 0.6) * 0.12;
            if self.rng.chance(p) {
                let i = (self.rng.unit() * self.chimes.len() as f32) as usize % self.chimes.len();
                let energy = self.rng.range(0.15, 0.4) * gust;
                let pan_pos = self.chimes[i].pan;
                self.chimes[i].modal.strike(energy, pan_pos);
            }
        }

        // Fire: heat flares and settles; embers pop at a rate that follows it.
        if self.fire_gain > 0.0 {
            self.fire_activity_timer -= dt;
            if self.fire_activity_timer <= 0.0 {
                self.fire_activity_target = self.rng.range(0.3, 1.0);
                self.fire_activity_timer = self.rng.range(4.0, 12.0);
            }
            self.fire_activity += (self.fire_activity_target - self.fire_activity) * (dt / 3.0).min(1.0);
            let act = self.fire_activity;
            if let Some(flame) = &mut self.flame {
                flame.drive(0.35 + 0.65 * act, act);
            }
            if let Some(wood) = &mut self.fire_wood {
                let rate = 6.0 + 22.0 * act;
                let expected = rate * dt;
                let mut n = expected.floor() as i32;
                if self.rng.unit() < expected.fract() {
                    n += 1;
                }
                for _ in 0..n {
                    // Most pops are small and bright; a few are big log-shifts.
                    let big = self.rng.chance(0.07);
                    let energy = if big { self.rng.range(0.5, 0.9) } else { self.rng.range(0.08, 0.4) };
                    wood.strike(energy, self.rng.range(0.3, 0.7));
                }
            }
        }
    }

    /// Run the physics (no audio) and print how the world is moving — for
    /// calibrating the water and weather without listening.
    pub fn probe(&mut self, seconds: f32) {
        let per_sec = (self.sr / BLOCK as f32) as usize;
        let steps = (seconds * self.sr / BLOCK as f32) as usize;
        let mut bubbles = 0usize;
        for i in 0..steps {
            self.step_controls();
            bubbles += self.bubbles.iter().filter(|b| b.active).count();
            if i % per_sec == 0 {
                let (e, s) = self.field.flow();
                println!(
                    "t={:>4.0}s  flow_e={:.3} flow_s={:.3}  water={:.3}  rain={:.2} air={:.2} gust={:.2}  bubbles~{}",
                    i as f32 * BLOCK as f32 / self.sr,
                    e,
                    s,
                    self.field.total_water(),
                    self.sky.rain(),
                    self.sky.air_speed(),
                    self.sky.gust(),
                    bubbles / per_sec.max(1),
                );
                bubbles = 0;
            }
        }
    }

    /// Step the physics and report the synchrony (Kuramoto order) of the first
    /// critter chorus each second — for watching a rhythm emerge.
    pub fn probe_sync(&mut self, seconds: f32) {
        if self.choruses.is_empty() {
            println!("(this world has no chorus)");
            return;
        }
        let sr = self.sr as usize;
        let total = (seconds * self.sr) as usize;
        for i in 0..total {
            self.next_sample(); // advances the choruses (and everything else)
            if i % sr == 0 {
                let r = self.choruses[0].chorus.order();
                let bar = "#".repeat((r * 40.0) as usize);
                println!("t={:>4.0}s  order={:.3}  {}", i as f32 / self.sr, r, bar);
            }
        }
    }

    pub fn next_sample(&mut self) -> (f32, f32) {
        if self.block_pos == 0 {
            self.step_controls();
            self.block_pos = BLOCK;
        }
        self.block_pos -= 1;

        let mut l = 0.0;
        let mut r = 0.0;
        let mut verb_l = 0.0;
        let mut verb_r = 0.0;

        // Continuous fluid voices.
        if let Some((stream, send)) = &mut self.stream {
            let (x, y) = stream.process();
            l += x;
            r += y;
            verb_l += x * *send;
            verb_r += y * *send;
        }
        if let Some(surf) = &mut self.surf {
            let (x, y) = surf.process();
            l += x;
            r += y;
            verb_l += x * 0.05;
            verb_r += y * 0.05;
        }
        if let Some(wind) = &mut self.wind {
            let (x, y) = wind.process();
            l += x;
            r += y;
        }
        if let Some(whistle) = &mut self.whistle {
            let (x, y) = whistle.process();
            l += x;
            r += y;
        }
        if let Some(leaves) = &mut self.leaves {
            let (x, y) = leaves.process();
            l += x;
            r += y;
            verb_l += x * 0.05;
            verb_r += y * 0.05;
        }
        if let Some(flame) = &mut self.flame {
            let (x, y) = flame.process();
            l += x;
            r += y;
        }

        // Struck solids.
        if let Some(roof) = &mut self.roof {
            let (x, y) = roof.process();
            l += x;
            r += y;
            verb_l += x * 0.12;
            verb_r += y * 0.12;
        }
        if let Some(ground) = &mut self.ground {
            let (x, y) = ground.process();
            l += x;
            r += y;
            verb_l += x * 0.08;
            verb_r += y * 0.08;
        }
        if let Some(wood) = &mut self.fire_wood {
            let (x, y) = wood.process();
            l += x;
            r += y;
            verb_l += x * 0.04;
            verb_r += y * 0.04;
        }
        for chime in &mut self.chimes {
            let (x, y) = chime.modal.process();
            l += x;
            r += y;
            verb_l += x * 0.6;
            verb_r += y * 0.6;
        }

        // Bubbles.
        for b in &mut self.bubbles {
            let (x, y) = b.process();
            l += x;
            r += y;
        }

        // Living voices, weighted by time of day.
        let night = self.sky.night();
        let day = 1.0 - night;
        for v in &mut self.choruses {
            let wake = v.nocturnal * night + (1.0 - v.nocturnal) * day;
            if wake < 1e-3 {
                continue;
            }
            let (x, y) = v.chorus.process();
            let g = v.gain * wake;
            l += x * g;
            r += y * g;
            let send = v.chorus.reverb_send();
            verb_l += x * g * send;
            verb_r += y * g * send;
        }

        // The surrounding space answers back.
        l += self.space_l.process(verb_l) * self.reverb_mix;
        r += self.space_r.process(verb_r) * self.reverb_mix;

        // Gentle startup fade.
        if self.fade < 1.0 {
            self.fade += self.fade_step;
            let g = self.fade * self.fade;
            l *= g;
            r *= g;
        }

        // Cushioned master soft-clip.
        ((l * 0.8).tanh() * 0.9, (r * 0.8).tanh() * 0.9)
    }
}
