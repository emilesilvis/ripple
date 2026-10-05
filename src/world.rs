//! The world: the field of matter, the sky above it, and the voices through
//! which their motion becomes sound. Presets assemble a world by laying out
//! terrain and switching on whatever belongs there; the world then runs itself.
//!
//! Two clocks turn here. The physics advances in short control blocks (water
//! flows, weather drifts, drops land, embers pop). Between those, every sample
//! is a reading of the world as it stands — the resonators still ringing, the
//! turbulence still hissing — gathered at two ears and returned as stereo.

use crate::bubbles::BubbleCloud;
use crate::contacts::ChimeRig;
use crate::critters::{Chorus, Species};
use crate::dsp::{pan, Noise, Space};
use crate::events::{EventLog, Kind as Observation};
use crate::field::{Event, Field, Surface};
use crate::matter::{bar_length_for_pitch, Body, Matter};
use crate::sky::{Climate, Sky};
use crate::voices::{Eddies, Turbulence};
use std::f32::consts::TAU;

mod assembly;
pub use assembly::{PhysicsReport, ResonantBody};

/// Minnaert's law: an air bubble of radius r in water rings at ~3.26/r Hz·m.
/// Every plink and burble in the world gets its pitch from this one line.
fn minnaert_hz(radius_m: f32) -> f32 {
    3.26 / radius_m.max(1e-4)
}

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
        Self {
            active: false,
            phase: 0.0,
            freq: 0.0,
            freq_rise: 0.0,
            amp: 0.0,
            decay: 0.0,
            pan: 0.5,
            sr,
        }
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

/// One steel bar that hangs in the air and is struck by the wind (a chime).
/// Its pitch is nothing but its length; the timing comes from the gusts.
struct Chime {
    body: Body,
    pan: f32,
    length_m: f32,
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

    // Struck bodies — lattices of bonded matter.
    roof: Option<Body>, // a corrugated steel sheet for rain to drum on
    fire_wood: Option<Body>,
    chimes: Vec<Chime>,
    chime_rig: Option<ChimeRig>,
    chime_strikes: u64,

    // Wind whistle band (resonant turbulence through an aperture).
    whistle: Option<Turbulence>,

    // Fire state (a small heat engine).
    fire_activity: f32,
    fire_activity_target: f32,
    fire_activity_timer: f32,
    fire_gain: f32,

    // Transient pools.
    bubbles: Vec<Bubble>,
    clouds: Vec<BubbleCloud>,
    cloud_events: u64,

    control_seconds: f64,

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
    events: EventLog,
}

impl World {
    pub fn new(
        sr: f32,
        seed: u32,
        climate: Climate,
        field_w: usize,
        field_h: usize,
        cell: f32,
    ) -> Self {
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
            fire_wood: None,
            chimes: Vec::new(),
            chime_rig: None,
            chime_strikes: 0,
            whistle: None,
            fire_activity: 0.6,
            fire_activity_target: 0.6,
            fire_activity_timer: 0.0,
            fire_gain: 0.0,
            bubbles: vec![Bubble::inactive(sr); 24],
            clouds: vec![BubbleCloud::new(sr); 24],
            cloud_events: 0,
            control_seconds: 0.0,
            choruses: Vec::new(),
            rain_gain: 0.0,
            space_l: Space::new(sr, 1.0),
            space_r: Space::new(sr, 1.15),
            reverb_mix: 0.5,
            fade: 0.0,
            fade_step: 1.0 / (3.0 * sr),
            events: EventLog::new(sr),
        }
    }

    pub fn observe(&mut self) {
        if !self.events.enabled() {
            self.events.enable();
            self.events.emit(Observation::Started { seed: self.seed });
        }
    }

    pub fn drain_events(&mut self, batch: &mut crate::events::Batch) {
        self.events.drain(batch);
    }

    pub(crate) fn visualize(&self, frame: &mut crate::visual::Frame) {
        frame.clear();
        frame.ready = true;
        frame.seed = self.seed;
        self.field.visualize(frame);
        frame.air = self.sky.air_speed();
        frame.rain = self.sky.rain();
        frame.daylight = self.sky.daylight();
        frame.fire = (self.fire_gain > 0.0).then_some(self.fire_activity);
        frame.bubbles = self.bubbles.iter().filter(|b| b.active).count();
        frame.clouds = self.clouds.iter().filter(|c| c.is_active()).count();
        if let Some(rig) = &self.chime_rig {
            rig.visualize(frame);
        }
        for (i, voice) in self.choruses.iter().enumerate() {
            let wake =
                voice.nocturnal * self.sky.night() + (1.0 - voice.nocturnal) * self.sky.daylight();
            voice
                .chorus
                .visualize(frame, i, voice.gain * wake, wake >= 1e-3);
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

    // Every fluid voice below is the same Turbulence under the same Strouhal
    // law; all that distinguishes wind from brook from flame is the physical
    // eddy scale and the reference flow speed handed over here.

    /// Water running over its bed: capillary-scale surface eddies, ~1 m/s.
    pub fn enable_stream(&mut self, gain: f32) {
        self.stream = Some((
            Turbulence::new(
                Eddies::free_shear(1.0e-4, 1.2),
                self.sr,
                self.seed ^ 0x57,
                gain,
            ),
            0.06,
        ));
    }
    /// Foam churning in the surf zone: finer spray eddies, faster water.
    pub fn enable_surf(&mut self, gain: f32) {
        self.surf = Some(Turbulence::new(
            Eddies::free_shear(1.5e-4, 3.0),
            self.sr,
            self.seed ^ 0x5f,
            gain,
        ));
    }
    /// Air shearing past the listener (millimetre eddies, gale at speed 1),
    /// plus a whistling crack — the same law through a resonant aperture.
    pub fn enable_wind(&mut self, gain: f32) {
        self.wind = Some(Turbulence::new(
            Eddies::free_shear(3.0e-3, 14.0),
            self.sr,
            self.seed ^ 0x1d,
            gain,
        ));
        self.whistle = Some(Turbulence::new(
            Eddies::aperture(1.0e-3, 14.0, 9.0),
            self.sr,
            self.seed ^ 0x1e,
            gain * 0.5,
        ));
    }
    /// Air torn at leaf edges: the smallest eddies in the world, hence the
    /// highest voice.
    pub fn enable_leaves(&mut self, gain: f32) {
        self.leaves = Some(Turbulence::new(
            Eddies::free_shear(3.0e-4, 14.0),
            self.sr,
            self.seed ^ 0x1f,
            gain,
        ));
    }
    /// A fire: a buoyant plume of slowish air (the rumble) over a wooden log
    /// — a real bar of wood — that pops as pockets burst against it.
    pub fn enable_fire(&mut self, gain: f32) {
        self.flame = Some(Turbulence::new(
            Eddies::free_shear(1.0e-3, 2.5),
            self.sr,
            self.seed ^ 0xf1,
            gain,
        ));
        self.fire_wood = Some(Body::bar(
            &Matter::wood(),
            1.2,
            0.10,
            12,
            self.sr,
            self.seed ^ 0xf2,
            gain * 28.0,
        ));
        self.fire_gain = gain;
    }
    /// Rain. If there is a roof, it is an actual corrugated steel sheet —
    /// the same steel as the chimes, rolled thin; corrugation is why it
    /// rings in the kilohertz. Drops on open water need no instrument at
    /// all: each entrains a little air bubble whose Minnaert ring *is* the
    /// plink.
    pub fn enable_rain(&mut self, gain: f32, on_roof: bool) {
        self.rain_gain = gain;
        if on_roof {
            self.roof = Some(Body::sheet(
                &Matter::steel(),
                0.25,
                0.18,
                0.02,
                7,
                self.sr,
                self.seed ^ 0x2a,
                gain * 36.0,
            ));
        }
    }
    /// Hang a set of steel bars. The preset hands over *pitches* only in the
    /// sense a chime-maker does: each is converted to a length by inverting
    /// the bending law, and from then on the bar rings entirely on its own —
    /// overtones, click and decay are the lattice's business.
    pub fn add_chimes(&mut self, freqs: &[f32], gain: f32) {
        let steel = Matter::steel();
        const THICKNESS: f32 = 0.022;
        const NODES: usize = 14;
        for (i, f) in freqs.iter().enumerate() {
            let length = bar_length_for_pitch(&steel, THICKNESS, NODES, *f);
            let body = Body::bar(
                &steel,
                length,
                THICKNESS,
                NODES,
                self.sr,
                self.seed ^ (0x3a + i as u32),
                gain * 60.0,
            );
            self.chimes.push(Chime {
                body,
                pan: 0.15 + 0.7 * (i as f32 / freqs.len().max(1) as f32),
                length_m: length,
            });
        }
        let lengths: Vec<f32> = self.chimes.iter().map(|chime| chime.length_m).collect();
        self.chime_rig = Some(ChimeRig::new(&lengths, self.seed));
    }
    pub fn add_chorus(
        &mut self,
        sp: Species,
        count: usize,
        coupling: f32,
        gain: f32,
        nocturnal: f32,
    ) {
        let chorus = Chorus::new(
            sp,
            count,
            coupling,
            self.sr,
            self.seed.wrapping_add(0x600d + count as u32),
        );
        self.choruses.push(Voices {
            chorus,
            gain,
            nocturnal,
        });
    }

    // --- the running world -------------------------------------------------

    fn step_controls(&mut self) {
        let dt = BLOCK as f32 / self.sr;
        self.sky.step(dt);
        self.field.step(dt);
        self.control_seconds += dt as f64;

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
                let surface = self.field.surface_at(nx, ny);
                self.events.emit(Observation::RainImpact {
                    x: nx,
                    y: ny,
                    strength: energy,
                    surface: match surface {
                        Surface::Roof => "roof",
                        Surface::Open => "open ground",
                        Surface::Rock => "rock",
                    },
                });
                match surface {
                    Surface::Roof => {
                        if let Some(roof) = &mut self.roof {
                            roof.strike(energy, nx);
                        }
                    }
                    Surface::Open | Surface::Rock => {
                        // The drop soaks in — this is how rain swells the brook.
                        self.field.rain_on(nx, ny, energy * 0.006);
                        // A drop punching into water entrains a tiny bubble;
                        // its Minnaert ring is the plink of rain on a pond.
                        let radius_m = self.rng.range(0.8e-3, 2.5e-3);
                        let amp = energy * 0.4 * self.rain_gain;
                        let mut voiced = false;
                        if let Some(b) = self.bubbles.iter_mut().find(|b| !b.active) {
                            b.spawn(minnaert_hz(radius_m), amp, nx, &mut self.rng);
                            voiced = true;
                        }
                        self.events.emit(Observation::Bubble {
                            cause: "rain",
                            radius_m,
                            strength: amp,
                            pan: nx,
                            voiced,
                        });
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

        // Each entrainment event divides its gas volume among eight coupled
        // members; the separate raindrop pool keeps isolated plinks distinct.
        for ev in self.field.drain_events() {
            match ev {
                Event::Bubble {
                    pan,
                    radius_m,
                    energy,
                } => {
                    let mut voiced = false;
                    if let Some(cloud) = self.clouds.iter_mut().find(|cloud| !cloud.is_active()) {
                        cloud.spawn(radius_m, energy, pan);
                        self.cloud_events += 1;
                        voiced = true;
                    }
                    self.events.emit(Observation::Bubble {
                        cause: "churn",
                        radius_m,
                        strength: energy,
                        pan,
                        voiced,
                    });
                }
                Event::Break { pan, energy } => {
                    self.events.emit(Observation::WaveBreak {
                        pan,
                        strength: energy,
                    });
                    if let Some(surf) = &mut self.surf {
                        surf.drive(energy + 0.2, 0.9);
                    }
                    // Breaking foam entrains a small gas packet as well as
                    // driving the continuous surf. Its equivalent volume is
                    // set by break strength; this is a coarse entrainment law.
                    if let Some(cloud) = self.clouds.iter_mut().find(|cloud| !cloud.is_active()) {
                        let radius_m = 0.003 + 0.006 * (energy / 0.6).clamp(0.0, 1.0);
                        cloud.spawn(radius_m, energy * 0.15, pan);
                        self.cloud_events += 1;
                        self.events.emit(Observation::Bubble {
                            cause: "breaking wave",
                            radius_m,
                            strength: energy * 0.15,
                            pan,
                            voiced: true,
                        });
                    }
                }
            }
        }

        // Wind and leaves ride the sky's air speed.
        let air = self.sky.air_speed();
        let gust = self.sky.gust();
        // A shared diffuse noise estimate at the animals' ears. This is a
        // coarse masking floor, not a resolved local acoustic field.
        for voices in &mut self.choruses {
            voices
                .chorus
                .set_masking_level(0.001 + flow_e * 0.006 + air * 0.0005);
        }
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

        if let Some(rig) = &mut self.chime_rig {
            // Chimes hang under a canopy/roof: their site receives half the
            // exposed listener's 14 m/s reference wind. This sheltered site
            // keeps the suspension model in its small-angle operating range.
            // Direction drifts continuously; it never schedules a strike.
            let direction = self.seed as f64 / u32::MAX as f64 * std::f64::consts::TAU
                + 0.35 * (self.control_seconds * 0.19).sin()
                + 0.3 * (gust as f64 - 0.5);
            let speed = air * 7.0;
            let chimes = &mut self.chimes;
            let strikes = &mut self.chime_strikes;
            let events = &mut self.events;
            rig.step(
                dt,
                [
                    speed * direction.cos() as f32,
                    speed * direction.sin() as f32,
                ],
                |impact| {
                    chimes[impact.bar]
                        .body
                        .strike(impact.velocity_kick, impact.position);
                    *strikes += 1;
                    events.emit(Observation::Contact {
                        body: impact.bar,
                        position: impact.position,
                        velocity_kick: impact.velocity_kick,
                    });
                },
            );
        }

        // Fire: heat flares and settles; embers pop at a rate that follows it.
        if self.fire_gain > 0.0 {
            self.fire_activity_timer -= dt;
            if self.fire_activity_timer <= 0.0 {
                self.fire_activity_target = self.rng.range(0.3, 1.0);
                self.fire_activity_timer = self.rng.range(4.0, 12.0);
            }
            self.fire_activity +=
                (self.fire_activity_target - self.fire_activity) * (dt / 3.0).min(1.0);
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
                    let energy = if big {
                        self.rng.range(0.5, 0.9)
                    } else {
                        self.rng.range(0.08, 0.4)
                    };
                    let position = self.rng.range(0.3, 0.7);
                    wood.strike(energy, position);
                    self.events.emit(Observation::FirePop {
                        position,
                        strength: energy,
                    });
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
                let history = self.field.history_stats();
                let soil = history.retained_water_m3;
                let stored = history.surface_water_m3 + soil;
                let contact = self
                    .chime_rig
                    .as_ref()
                    .map(ChimeRig::diagnostics)
                    .unwrap_or_default();
                println!(
                    "t={:>4.0}s  flow_e={:.3} flow_s={:.3}  water={:.3} soil={:.5} total={:.3}m3  rain={:.2} air={:.2} gust={:.2}  drops~{} contacts={} angle={:.3}",
                    i as f32 * BLOCK as f32 / self.sr,
                    e,
                    s,
                    self.field.total_water(),
                    soil,
                    stored,
                    self.sky.rain(),
                    self.sky.air_speed(),
                    self.sky.gust(),
                    bubbles / per_sec.max(1),
                    self.chime_strikes,
                    contact.max_suspension_angle_rad,
                );
                bubbles = 0;
            }
        }
    }

    /// Advance the actual audio timeline and inspect the first population's
    /// behavioral-clock phase order plus emitted and received calls. A high
    /// phase order is not a guarantee that audible syllables coincide.
    pub fn probe_sync(&mut self, seconds: f32) {
        if self.choruses.is_empty() {
            println!("(this world has no chorus)");
            return;
        }
        let sr = self.sr as usize;
        let total = (seconds * self.sr) as usize;
        for completed in 1..=total {
            self.next_sample();
            if completed % sr == 0 || completed == total {
                let chorus = &self.choruses[0].chorus;
                let order = chorus.order();
                let hearing = chorus.hearing_stats();
                let bar = "#".repeat((order * 40.0) as usize);
                println!(
                    "t={:>7.3}s  order={:.3} emitted={} heard={} masked={}  {}",
                    completed as f64 / self.sr as f64,
                    order,
                    hearing.emitted_calls,
                    hearing.heard_calls,
                    hearing.masked_calls,
                    bar
                );
            }
        }
    }

    pub fn next_sample(&mut self) -> (f32, f32) {
        if self.block_pos == 0 {
            self.step_controls();
            self.block_pos = BLOCK;
            if self.events.summary_due() {
                self.events.emit(Observation::Weather {
                    air: self.sky.air_speed(),
                    gust: self.sky.gust(),
                    rain: self.sky.rain(),
                    daylight: self.sky.daylight(),
                });
                let history = self.field.history_stats();
                let (flow, speed) = self.field.flow();
                self.events.emit(Observation::Water {
                    flow,
                    speed,
                    surface_m3: history.surface_water_m3,
                    retained_m3: history.retained_water_m3,
                    suspended_m3: history.suspended_solid_m3,
                    exported_m3: history.exported_solid_m3,
                });
            }
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

        // Struck bodies, each a lattice still ringing from whatever hit it.
        if let Some(roof) = &mut self.roof {
            let (x, y) = roof.process();
            l += x;
            r += y;
            verb_l += x * 0.12;
            verb_r += y * 0.12;
        }
        if let Some(wood) = &mut self.fire_wood {
            let (x, y) = wood.process();
            l += x;
            r += y;
            verb_l += x * 0.04;
            verb_r += y * 0.04;
        }
        for chime in &mut self.chimes {
            let (x, y) = chime.body.process();
            let (xl, _) = pan(x, chime.pan);
            let (_, yr) = pan(y, chime.pan);
            l += xl;
            r += yr;
            verb_l += xl * 0.6;
            verb_r += yr * 0.6;
        }

        // Bubbles.
        for b in &mut self.bubbles {
            let (x, y) = b.process();
            l += x;
            r += y;
        }

        for cloud in &mut self.clouds {
            let (x, y) = cloud.process();
            l += x;
            r += y;
        }

        // Living voices, weighted by time of day.
        let night = self.sky.night();
        let day = 1.0 - night;
        for (population, v) in self.choruses.iter_mut().enumerate() {
            let wake = v.nocturnal * night + (1.0 - v.nocturnal) * day;
            if wake < 1e-3 {
                continue;
            }
            let (x, y) = if self.events.enabled() {
                v.chorus
                    .process_observed(population, |event| self.events.emit(event))
            } else {
                v.chorus.process()
            };
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
        self.events.advance();
        ((l * 0.8).tanh() * 0.9, (r * 0.8).tanh() * 0.9)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    #[ignore = "30-second physical integration audit across flowing-water presets"]
    fn flowing_presets_drive_suspensions_clouds_and_active_history() {
        for name in ["glade", "brook", "cozy-rain", "shore", "storm"] {
            let mut world = crate::presets::build(name, 48_000.0, 12345).unwrap();
            let mut cloud_seen = false;
            for sample in 0..48_000 * 30 {
                let sound = world.next_sample();
                assert!(sound.0.is_finite() && sound.1.is_finite());
                if sample % BLOCK == 0 {
                    cloud_seen |= world.clouds.iter().any(BubbleCloud::is_active);
                }
            }
            let history = world.field.history_stats();
            let angle = world
                .chime_rig
                .as_ref()
                .map_or(0.0, |rig| rig.diagnostics().max_suspension_angle_rad);
            println!("{name}: contacts={}, cloud_seen={cloud_seen}, max_angle={angle:.6}, retained_m3={:.9}, mobile_solid_m3={:.9}",
                world.chime_strikes, history.retained_water_m3, history.suspended_solid_m3 + history.exported_solid_m3);
            // The stream presets churn continuously; a gentle sea can stay
            // below the field's breaking/entrainment threshold for this seed.
            if name != "shore" {
                assert!(cloud_seen, "{name}: churn must activate the cloud pool");
            }
            assert!(
                history.suspended_solid_m3 + history.exported_solid_m3 > 0.0,
                "{name}: moving water must change its bed"
            );
            if world.chime_rig.is_some() {
                assert!(
                    world.chime_rig.as_ref().unwrap().diagnostics().wind_work_j > 0.0,
                    "{name}: air must do work on the suspension"
                );
                assert!(
                    angle < 0.3,
                    "{name}: small-angle approximation exceeded: {angle}"
                );
            }
        }
    }
}
