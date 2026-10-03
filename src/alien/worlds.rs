//! Discovered initial conditions for the same World used by Earth presets.
//! No named species, tuning scales, recordings, or preset-world builders.
use super::{Genome, Random, Structure, MAX_NODES, MIN_NODES};
use crate::acoustics::{Barrier, HearingScene, Point};
use crate::critters::Species;
use crate::matter::Matter;
use crate::sky::Climate;
use crate::voices::Eddies;
use crate::world::{PhysicsReport, ResonantBody, World};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Solid {
    wave_speed: f64,
    loss: f64,
    length: f64,
    thickness: f64,
    pan: f64,
    gain: f64,
}

impl Solid {
    fn random(rng: &mut Random) -> Self {
        Self {
            wave_speed: rng.range(1200.0, 5000.0),
            loss: rng.log_range(2e-7, 8e-6),
            length: rng.range(0.55, 1.6),
            thickness: rng.range(0.006, 0.020),
            pan: rng.range(0.05, 0.95),
            gain: rng.log_range(0.8, 6.0),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Flow {
    scale: f64,
    speed: f64,
    resonance: f64,
    gain: f64,
}

impl Flow {
    fn random(rng: &mut Random) -> Self {
        Self {
            scale: rng.log_range(0.0002, 0.015),
            speed: rng.range(1.0, 10.0),
            resonance: rng.log_range(0.55, 4.0),
            gain: rng.log_range(0.03, 0.8),
        }
    }
    fn eddies(&self) -> Eddies {
        Eddies::aperture(self.scale as f32, self.speed as f32, self.resonance as f32)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Population {
    organ: Genome,
    scale: f64,
    pace: f64,
    duration: f64,
    pulse: f64,
    coupling: f64,
    spread: f64,
    transmission: f64,
    count: usize,
    gain: f64,
    nocturnal: f64,
}

impl Population {
    fn random(rng: &mut Random) -> Self {
        Self {
            organ: Genome::random(rng),
            scale: rng.log_range(0.25, 2.8),
            pace: rng.range(1.5, 9.0),
            duration: rng.range(0.3, 1.8),
            pulse: rng.range(0.0, 5.0),
            coupling: rng.range(-0.25, 0.35),
            spread: rng.range(1.0, 12.0),
            transmission: rng.range(0.1, 1.0),
            count: 2 + rng.index(4),
            gain: rng.range(0.3, 1.0),
            nocturnal: rng.range(0.1, 0.9),
        }
    }

    fn species(&self, sr: f32) -> Species {
        let solved = Structure::solve(&self.organ);
        let mut modes: Vec<_> = (0..self.organ.nodes.len())
            .map(|i| {
                let weight = solved.pickup[i][0].hypot(solved.pickup[i][1]) / solved.beta[i].sqrt();
                (solved.omega[i] / TAU * self.scale, weight)
            })
            .filter(|(f, _)| *f > 20.0 && *f < f64::from(sr) * 0.35)
            .collect();
        modes.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Valid parameter bounds ensure at least one audible mode at 16 kHz.
        let (carrier, first) = modes[0];
        let harmonics: Vec<_> = modes
            .iter()
            .skip(1)
            .map(|(f, w)| ((*f / carrier) as f32, (w / first.max(1e-8)).min(2.0) as f32))
            .collect();
        let norm = 1.0 + harmonics.iter().map(|(_, g)| g).sum::<f32>();
        Species {
            carrier_lo: carrier as f32,
            carrier_hi: carrier as f32 * 1.015,
            phrase_gap_lo: self.pace as f32,
            phrase_gap_hi: self.pace as f32 * 1.5,
            syllables: (1, 2),
            syl_dur_lo: self.duration as f32,
            syl_dur_hi: self.duration as f32 * 1.2,
            syl_gap_lo: 0.2,
            syl_gap_hi: 0.6,
            sweep_lo: 0.98,
            sweep_hi: 1.02,
            pulse_rate: self.pulse as f32,
            harmonics,
            vib_rate: 0.35,
            vib_depth: 0.008,
            amp_lo: 0.045 / norm,
            amp_hi: 0.09 / norm,
            reverb_send: 0.3,
        }
    }
}

/// Versioned physical recipe. Save parameters, not just a changing search seed.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldSpec {
    pub seed: u32,
    slope: f64,
    cross_slope: f64,
    relief: f64,
    spring: f64,
    swell: f64,
    wind: f64,
    rain: f64,
    day_seconds: f64,
    room: f64,
    reflection: f64,
    aperture: f64,
    flows: [Flow; 2],
    solids: Vec<Solid>,
    populations: Vec<Population>,
}

fn bounded(value: f64, lo: f64, hi: f64) -> bool {
    value.is_finite() && (lo..=hi).contains(&value)
}

impl WorldSpec {
    pub fn random(seed: u32) -> Self {
        let mut rng = Random(u64::from(seed) ^ 0xa71a5);
        Self {
            seed,
            slope: rng.range(0.015, 0.07),
            cross_slope: rng.range(-0.025, 0.025),
            relief: rng.range(0.025, 0.22),
            spring: rng.range(0.25, 1.4),
            swell: rng.range(0.0, 0.16),
            wind: rng.range(0.18, 0.7),
            rain: rng.range(0.05, 0.5),
            day_seconds: rng.range(80.0, 400.0),
            room: rng.range(0.5, 2.8),
            reflection: rng.range(0.1, 0.65),
            aperture: rng.range(0.08, 1.0),
            flows: [Flow::random(&mut rng), Flow::random(&mut rng)],
            solids: (0..2 + rng.index(4))
                .map(|_| Solid::random(&mut rng))
                .collect(),
            populations: (0..1 + rng.index(3))
                .map(|_| Population::random(&mut rng))
                .collect(),
        }
    }

    pub fn mutate(&self, seed: u64) -> Self {
        let mut rng = Random(seed);
        let mut child = self.clone();
        // Retain geography's random phases when exploring neighbours.
        for _ in 0..3 {
            match rng.index(10) {
                0 => {
                    child.slope = (child.slope * rng.range(-0.4, 0.4).exp()).clamp(0.015, 0.07);
                    child.relief = (child.relief * rng.range(-0.4, 0.4).exp()).clamp(0.025, 0.22);
                }
                1 => {
                    child.spring = (child.spring * rng.range(-0.4, 0.4).exp()).clamp(0.25, 1.4);
                    child.rain = (child.rain * rng.range(-0.5, 0.5).exp()).clamp(0.05, 0.5);
                }
                2 => {
                    child.wind = (child.wind * rng.range(-0.3, 0.3).exp()).clamp(0.18, 0.7);
                    child.day_seconds =
                        (child.day_seconds * rng.range(-0.4, 0.4).exp()).clamp(80.0, 400.0);
                }
                3 => {
                    let i = rng.index(2);
                    child.flows[i] = Flow::random(&mut rng);
                }
                4 => {
                    let i = rng.index(child.solids.len());
                    child.solids[i] = Solid::random(&mut rng);
                }
                5 if child.solids.len() < 5 => child.solids.push(Solid::random(&mut rng)),
                6 => {
                    let i = rng.index(child.populations.len());
                    child.populations[i].organ = child.populations[i].organ.mutate(&mut rng);
                    child.populations[i].scale =
                        (child.populations[i].scale * rng.range(-0.4, 0.4).exp()).clamp(0.25, 2.8);
                }
                7 => {
                    let i = rng.index(child.populations.len());
                    child.populations[i] = Population::random(&mut rng);
                }
                8 if child.populations.len() < 3 => {
                    child.populations.push(Population::random(&mut rng))
                }
                _ => {
                    child.room = rng.range(0.5, 2.8);
                    child.reflection = rng.range(0.1, 0.65);
                    child.aperture = rng.range(0.08, 1.0);
                    if child.solids.len() > 2 {
                        child.solids.remove(rng.index(child.solids.len()));
                    }
                }
            }
        }
        child
    }

    pub fn validate(&self) -> Result<(), String> {
        let valid = bounded(self.slope, 0.015, 0.07)
            && bounded(self.cross_slope, -0.025, 0.025)
            && bounded(self.relief, 0.025, 0.22)
            && bounded(self.spring, 0.25, 1.4)
            && bounded(self.swell, 0.0, 0.16)
            && bounded(self.wind, 0.18, 0.7)
            && bounded(self.rain, 0.05, 0.5)
            && bounded(self.day_seconds, 80.0, 400.0)
            && bounded(self.room, 0.5, 2.8)
            && bounded(self.reflection, 0.1, 0.65)
            && bounded(self.aperture, 0.08, 1.0)
            && (2..=5).contains(&self.solids.len())
            && (1..=3).contains(&self.populations.len());
        if !valid {
            return Err("world recipe exceeds the model's physical bounds".into());
        }
        for f in &self.flows {
            if !(bounded(f.scale, 0.0002, 0.015)
                && bounded(f.speed, 1.0, 10.0)
                && bounded(f.resonance, 0.55, 4.0)
                && bounded(f.gain, 0.03, 0.8))
            {
                return Err("invalid fluid geometry".into());
            }
        }
        for s in &self.solids {
            if !(bounded(s.wave_speed, 1200.0, 5000.0)
                && bounded(s.loss, 2e-7, 8e-6)
                && bounded(s.length, 0.55, 1.6)
                && bounded(s.thickness, 0.006, 0.020)
                && bounded(s.pan, 0.05, 0.95)
                && bounded(s.gain, 0.8, 6.0))
            {
                return Err("invalid solid material or geometry".into());
            }
        }
        for p in &self.populations {
            let g = &p.organ;
            if !(bounded(p.scale, 0.25, 2.8)
                && bounded(p.pace, 1.5, 9.0)
                && bounded(p.duration, 0.3, 1.8)
                && bounded(p.pulse, 0.0, 5.0)
                && bounded(p.coupling, -0.25, 0.35)
                && bounded(p.spread, 1.0, 12.0)
                && bounded(p.transmission, 0.1, 1.0)
                && bounded(p.gain, 0.3, 1.0)
                && bounded(p.nocturnal, 0.1, 0.9)
                && (2..=5).contains(&p.count)
                && (MIN_NODES..=MAX_NODES).contains(&g.nodes.len())
                && g.bonds.len() <= MAX_NODES * MAX_NODES / 2
                && bounded(g.loss, 0.6, 2.2))
            {
                return Err("invalid population".into());
            }
            for n in &g.nodes {
                if !(bounded(n.mass, 0.5, 2.0)
                    && bounded(n.anchor, 1e6, 2e7)
                    && bounded(n.pan, 0.05, 0.95)
                    && bounded(n.threshold, 0.15, 0.7))
                {
                    return Err("invalid voice-organ mass".into());
                }
            }
            let mut edges = std::collections::HashSet::new();
            for b in &g.bonds {
                if b.a >= g.nodes.len()
                    || b.b >= g.nodes.len()
                    || b.a == b.b
                    || !bounded(b.stiffness, 3e5, 8e6)
                    || !edges.insert((b.a.min(b.b), b.a.max(b.b)))
                {
                    return Err("invalid voice-organ bond".into());
                }
            }
        }
        Ok(())
    }

    pub fn counts(&self) -> (usize, usize) {
        (
            self.solids.len(),
            self.populations.iter().map(|p| p.count).sum(),
        )
    }

    pub fn build(&self, sr: f32) -> Result<World, String> {
        self.validate()?;
        if !sr.is_finite() || !(16_000.0..=192_000.0).contains(&sr) {
            return Err("alien worlds need a sample rate from 16000 to 192000 Hz".into());
        }
        let climate = Climate {
            wind_base: self.wind as f32,
            wind_var: 0.18,
            rain_base: self.rain as f32,
            rain_var: 0.12,
            day_len: self.day_seconds as f32,
            day_start: 0.35,
        };
        let mut world = World::new(sr, self.seed, climate, 16, 28, 0.25);
        let mut rng = Random(u64::from(self.seed) ^ 0x7e22a1);
        let phases = std::array::from_fn::<_, 4, _>(|_| rng.range(0.0, TAU));
        let field = world.field_mut();
        for y in 0..28 {
            for x in 0..16 {
                let nx = x as f64 / 16.0;
                let ny = y as f64 / 28.0;
                let bump = (nx * 9.4 + phases[0]).sin() * (ny * 7.1 + phases[1]).sin()
                    + 0.5 * (nx * 17.3 + phases[2]).sin() * (ny * 13.9 + phases[3]).sin();
                field.set_terrain(
                    x,
                    y,
                    ((27 - y) as f64 * self.slope
                        + x as f64 * self.cross_slope
                        + self.relief * bump) as f32,
                );
                if y == 27 {
                    field.set_drain(x, y, 6.0);
                }
            }
        }
        let source_x = 2 + rng.index(12);
        for dx in 0..2 {
            field.set_source(source_x + dx, 0, self.spring as f32);
        }
        field.set_swell(self.swell as f32, (5.0 + self.room * 3.0) as f32, 0.0);
        field.geology(4000, self.rain as f32 * 0.025);
        field.enable_history();
        world.set_flows(
            self.flows[0].eddies(),
            self.flows[1].eddies(),
            [self.flows[0].gain as f32, self.flows[1].gain as f32],
        );
        world.enable_rain(0.12, false);
        let bodies: Vec<_> = self
            .solids
            .iter()
            .map(|s| ResonantBody {
                matter: Matter {
                    wave_speed: s.wave_speed as f32,
                    internal_loss: s.loss as f32,
                    air_loss: 1.0,
                },
                length: s.length as f32,
                thickness: s.thickness as f32,
                nodes: 10,
                pan: s.pan as f32,
                gain: s.gain as f32,
            })
            .collect();
        world.set_resonant_bodies(&bodies);
        for p in &self.populations {
            let scene = HearingScene {
                positions: (0..p.count)
                    .map(|_| Point {
                        x: rng.range(-p.spread, p.spread) as f32,
                        y: rng.range(1.0, p.spread + 1.0) as f32,
                    })
                    .collect(),
                listener: Point { x: 0.0, y: -1.0 },
                barrier: Some(Barrier {
                    x: 0.0,
                    y_min: 0.0,
                    y_max: p.spread as f32,
                    transmission: p.transmission as f32,
                }),
                sound_speed: 343.0,
                hearing_threshold: 0.001,
                masking_level: 0.001,
            };
            world.add_population(
                p.species(sr),
                scene,
                p.coupling as f32,
                p.gain as f32,
                p.nocturnal as f32,
            )?;
        }
        world.set_reverb(self.room as f32, self.reflection as f32);
        Ok(world)
    }
}

/// Fixed listening gain after the shared engine's bounded output. This bounds
/// digital peaks, not total mechanical energy in the multi-physics world.
pub struct Soundscape {
    world: World,
    gain: f32,
    aperture: f32,
}
impl Soundscape {
    pub fn new(spec: &WorldSpec, sr: f32, gain: f64) -> Result<Self, String> {
        if !bounded(gain, 0.0, 0.5 / 0.9) {
            return Err("invalid listening gain".into());
        }
        Ok(Self {
            world: spec.build(sr)?,
            gain: gain as f32,
            aperture: spec.aperture as f32,
        })
    }
    pub fn next_sample(&mut self) -> (f32, f32) {
        let (l, r) = self.world.next_sample();
        // A bounded stereo observation aperture. This is a simple listener
        // approximation, alongside the engine's resolved population paths.
        let mid = (l + r) * 0.5;
        let side = (l - r) * 0.5 * self.aperture;
        ((mid + side) * self.gain, (mid - side) * self.gain)
    }
    pub fn physics_report(&self) -> PhysicsReport {
        self.world.physics_report()
    }
}
