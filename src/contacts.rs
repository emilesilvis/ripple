//! Wind supplies work, suspended matter moves, and contact excites the bars.
//!
//! This is a horizontal, small-angle suspension model, not a full rigid-body
//! solver. Each bar and the sail/clapper assembly has a mass, pendulum length,
//! quadratic air drag, and suspension loss. A rigid normal impulse dissipates
//! energy according to restitution; a fraction of that loss excites the existing
//! bending lattice. The lattice's strike input is a velocity kick, not joules.
//! Its effective nodal mass below is a calibration, and lattice motion does not
//! feed back into the low-frequency suspension. Numerical penetration correction
//! is recorded separately instead of being hidden in the energy budget.

use crate::dsp::Noise;
use std::f64::consts::TAU;

const GRAVITY: f64 = 9.81;
const RESTITUTION: f64 = 0.45;
const MAX_STEP: f64 = 1.0 / 1_500.0;
const ACOUSTIC_FRACTION: f64 = 0.12;
const EFFECTIVE_NODE_MASS_KG: f64 = 0.015;

#[derive(Clone)]
struct Suspended {
    origin: [f64; 2],
    position: [f64; 2],
    velocity: [f64; 2],
    mass: f64,
    length: f64,
    radius: f64,
    drag: f64,
}

impl Suspended {
    fn new(origin: [f64; 2], mass: f64, length: f64, radius: f64, area: f64) -> Self {
        Self {
            origin,
            position: origin,
            velocity: [0.0; 2],
            mass,
            length,
            radius,
            // 1/2 air density (1.2 kg/m³), drag coefficient 1, sail area.
            drag: 0.6 * area,
        }
    }

    fn kinetic_energy(&self) -> f64 {
        0.5 * self.mass * dot(self.velocity, self.velocity)
    }

    fn energy(&self) -> f64 {
        let displacement = sub(self.position, self.origin);
        self.kinetic_energy()
            + 0.5 * self.mass * GRAVITY / self.length * dot(displacement, displacement)
    }

    /// The exact constant-wind quadratic-drag solution. Work from the air is
    /// wind · impulse; their difference from kinetic energy becomes drag heat.
    fn drag_step(&mut self, dt: f64, wind: [f64; 2], ledger: &mut Diagnostics) {
        let before = self.kinetic_energy();
        let relative = sub(self.velocity, wind);
        let scale = 1.0 / (1.0 + self.drag / self.mass * dot(relative, relative).sqrt() * dt);
        let old_velocity = self.velocity;
        self.velocity = add(wind, mul(relative, scale));
        let work = self.mass * dot(wind, sub(self.velocity, old_velocity));
        ledger.wind_work_j += work;
        ledger.damping_loss_j += work - (self.kinetic_energy() - before);
    }

    fn step(&mut self, dt: f64, wind: [f64; 2], ledger: &mut Diagnostics) {
        self.drag_step(dt * 0.5, wind, ledger);
        let before = self.kinetic_energy();
        self.velocity = mul(self.velocity, (-0.15 * dt).exp());
        ledger.damping_loss_j += before - self.kinetic_energy();
        // Exact unforced linear-pendulum evolution: no artificial energy growth.
        let omega = (GRAVITY / self.length).sqrt();
        let (sine, cosine) = (omega * dt).sin_cos();
        let displacement = sub(self.position, self.origin);
        self.position = add(
            self.origin,
            add(mul(displacement, cosine), mul(self.velocity, sine / omega)),
        );
        self.velocity = sub(mul(self.velocity, cosine), mul(displacement, omega * sine));
        self.drag_step(dt * 0.5, wind, ledger);
        let angle = dot(
            sub(self.position, self.origin),
            sub(self.position, self.origin),
        )
        .sqrt()
            / self.length;
        ledger.max_suspension_angle_rad = ledger.max_suspension_angle_rad.max(angle);
    }
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: [f64; 2], b: f64) -> [f64; 2] {
    [a[0] * b, a[1] * b]
}

/// A passive, frictionless, normal impulse. Returns the kinetic energy lost.
fn collide(a: &mut Suspended, b: &mut Suspended, normal: [f64; 2]) -> f64 {
    let approach_speed = dot(sub(a.velocity, b.velocity), normal);
    if approach_speed <= 0.0 {
        return 0.0;
    }
    let inverse_mass = a.mass.recip() + b.mass.recip();
    let impulse = (1.0 + RESTITUTION) * approach_speed / inverse_mass;
    a.velocity = sub(a.velocity, mul(normal, impulse / a.mass));
    b.velocity = add(b.velocity, mul(normal, impulse / b.mass));
    0.5 * approach_speed * approach_speed / inverse_mass * (1.0 - RESTITUTION * RESTITUTION)
}

/// A collision that may excite a bar's existing bending lattice.
#[derive(Clone, Copy, Debug)]
pub struct Impact {
    pub bar: usize,
    /// Velocity increment at one effective lattice node, calibrated from the
    /// acoustic fraction of lost collision energy; suitable for Body::strike.
    pub velocity_kick: f32,
    /// Height of the clapper along the hanging bar, normalized to bar length.
    pub position: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub impacts: u64,
    pub wind_work_j: f64,
    pub damping_loss_j: f64,
    pub collision_loss_j: f64,
    pub mechanical_energy_j: f64,
    /// Signed energy introduced by numerical penetration correction.
    pub constraint_work_j: f64,
    pub constraint_absolute_work_j: f64,
    /// E - (wind work - heat - collision loss + numerical correction).
    /// The normal public constructor starts at zero mechanical energy.
    pub integration_residual_j: f64,
    /// Horizontal displacement / suspension length. Above ~0.3 rad the
    /// linearized pendulum approximation needs particular caution.
    pub max_suspension_angle_rad: f64,
}

/// A sail/clapper surrounded by independently suspended bars. Its seed only
/// determines assembly orientation; impacts follow motion and contact.
#[derive(Clone)]
pub struct ChimeRig {
    clapper: Suspended,
    bars: Vec<Suspended>,
    contact_position: Vec<f32>,
    touching: Vec<bool>,
    ledger: Diagnostics,
}

impl ChimeRig {
    pub fn new(lengths_m: &[f32], seed: u32) -> Self {
        let mut rng = Noise::new(seed ^ 0xc01d_ca11);
        let orientation = rng.unit() as f64 * TAU;
        let bars = lengths_m
            .iter()
            .enumerate()
            .map(|(i, &length)| {
                let angle = orientation + i as f64 * TAU / lengths_m.len().max(1) as f64;
                let length = length.max(0.08) as f64;
                Suspended::new(
                    [0.070 * angle.cos(), 0.070 * angle.sin()],
                    0.55 * length,
                    0.20 + length * 0.5,
                    0.013,
                    length * 0.015,
                )
            })
            .collect();
        Self {
            clapper: Suspended::new([0.0; 2], 0.12, 0.48, 0.032, 0.030),
            bars,
            // Bar tops are 0.20 m below the mounting; clapper is at 0.48 m.
            contact_position: lengths_m
                .iter()
                .map(|length| (0.28 / length.max(0.08)).clamp(0.05, 0.95))
                .collect(),
            touching: vec![false; lengths_m.len()],
            ledger: Diagnostics::default(),
        }
    }

    /// Advance mechanics with a physical horizontal wind velocity in m/s.
    /// Control blocks are internally subdivided for contact resolution. The
    /// callback fires at new contacts, including real diminishing rebounds;
    /// sustained contact cannot repeatedly invent a new acoustic strike.
    pub fn step(&mut self, dt: f32, wind_mps: [f32; 2], mut strike: impl FnMut(Impact)) {
        if dt <= 0.0 || !dt.is_finite() || wind_mps.iter().any(|value| !value.is_finite()) {
            return;
        }
        let steps = (dt as f64 / MAX_STEP).ceil().max(1.0) as usize;
        let h = dt as f64 / steps as f64;
        let wind = [wind_mps[0] as f64, wind_mps[1] as f64];
        for _ in 0..steps {
            self.clapper.step(h, wind, &mut self.ledger);
            for bar in &mut self.bars {
                bar.step(h, wind, &mut self.ledger);
            }
            for i in 0..self.bars.len() {
                let bar = &mut self.bars[i];
                let offset = sub(bar.position, self.clapper.position);
                let distance = dot(offset, offset).sqrt();
                let contact_distance = bar.radius + self.clapper.radius;
                if distance > contact_distance + 1e-5 {
                    self.touching[i] = false;
                }
                if distance >= contact_distance {
                    continue;
                }
                let normal = if distance > 1e-12 {
                    mul(offset, distance.recip())
                } else {
                    [1.0, 0.0]
                };
                let lost = collide(&mut self.clapper, bar, normal);
                self.ledger.collision_loss_j += lost;
                if lost > 0.0 && !self.touching[i] {
                    self.touching[i] = true;
                    self.ledger.impacts += 1;
                    strike(Impact {
                        bar: i,
                        velocity_kick: (2.0 * lost * ACOUSTIC_FRACTION / EFFECTIVE_NODE_MASS_KG)
                            .sqrt() as f32,
                        position: self.contact_position[i],
                    });
                }
                // The correction only removes the small overlap introduced by
                // a finite step. Its potential-energy cost is visible in CSV.
                let before = self.clapper.energy() + bar.energy();
                let inverse_mass = self.clapper.mass.recip() + bar.mass.recip();
                let correction = (contact_distance - distance) / inverse_mass;
                self.clapper.position = sub(
                    self.clapper.position,
                    mul(normal, correction / self.clapper.mass),
                );
                bar.position = add(bar.position, mul(normal, correction / bar.mass));
                let correction_work = self.clapper.energy() + bar.energy() - before;
                self.ledger.constraint_work_j += correction_work;
                self.ledger.constraint_absolute_work_j += correction_work.abs();
            }
        }
    }

    pub fn diagnostics(&self) -> Diagnostics {
        let mut result = self.ledger;
        result.mechanical_energy_j =
            self.clapper.energy() + self.bars.iter().map(Suspended::energy).sum::<f64>();
        result.integration_residual_j = result.mechanical_energy_j
            - (result.wind_work_j - result.damping_loss_j - result.collision_loss_j
                + result.constraint_work_j);
        result
    }
}

#[cfg(test)]
fn varying_wind(t: f64, seed: u32) -> [f32; 2] {
    // Continuous forcing, independent of any collision clock or contact state.
    let phase = (seed % 997) as f64 / 997.0 * TAU;
    let speed = 2.3 + 0.9 * (1.4 * t + phase).sin() + 0.4 * (4.9 * t + phase * 0.37).sin();
    let direction = 0.55 * t + phase;
    [
        (speed * direction.cos()) as f32,
        (speed * direction.sin()) as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_air_cannot_strike_a_motionless_chime() {
        let mut rig = ChimeRig::new(&[0.7, 0.65, 0.6, 0.55, 0.5], 12345);
        for _ in 0..15_000 {
            rig.step(1.0 / 1_500.0, [0.0; 2], |_| panic!("unforced strike"));
        }
        let d = rig.diagnostics();
        assert_eq!(d.impacts, 0);
        assert_eq!(d.mechanical_energy_j, 0.0);
        assert_eq!(d.wind_work_j, 0.0);
    }

    #[test]
    fn collision_conserves_momentum_and_loses_only_restitution_energy() {
        let mut a = Suspended::new([0.0; 2], 0.12, 0.48, 0.03, 0.03);
        let mut b = Suspended::new([0.05, 0.0], 0.4, 0.6, 0.01, 0.01);
        a.velocity = [0.6, 0.3];
        b.velocity = [-0.2, -0.1];
        let before = a.kinetic_energy() + b.kinetic_energy();
        let momentum = add(mul(a.velocity, a.mass), mul(b.velocity, b.mass));
        let loss = collide(&mut a, &mut b, [1.0, 0.0]);
        let after = a.kinetic_energy() + b.kinetic_energy();
        let after_momentum = add(mul(a.velocity, a.mass), mul(b.velocity, b.mass));
        assert!(loss > 0.0 && after < before);
        assert!((before - after - loss).abs() < 1e-12);
        assert!((momentum[0] - after_momentum[0]).abs() < 1e-12);
        assert!((momentum[1] - after_momentum[1]).abs() < 1e-12);
        assert!((b.velocity[0] - a.velocity[0] - 0.8 * RESTITUTION).abs() < 1e-12);
        assert_eq!(collide(&mut a, &mut b, [1.0, 0.0]), 0.0);
    }

    fn driven(dt: f32) -> Diagnostics {
        let mut rig = ChimeRig::new(&[0.7, 0.65, 0.6, 0.55, 0.5], 12345);
        for frame in 0..(12.0 / dt).round() as usize {
            rig.step(
                dt,
                varying_wind(frame as f64 * dt as f64, 12345),
                |impact| {
                    assert!(impact.velocity_kick.is_finite() && impact.velocity_kick > 0.0);
                },
            );
        }
        rig.diagnostics()
    }

    #[test]
    fn forcing_creates_collisions_with_a_closed_mechanical_budget() {
        let d = driven(1.0 / 1_500.0);
        assert!(d.impacts > 10, "{d:?}");
        assert!(d.wind_work_j > 0.0 && d.collision_loss_j > 0.0);
        assert!(d.damping_loss_j > 0.0);
        assert!(d.integration_residual_j.abs() < 1e-8, "{d:?}");
        assert!(d.constraint_absolute_work_j / d.wind_work_j < 0.03, "{d:?}");
        assert!(d.max_suspension_angle_rad < 0.3, "{d:?}");
    }

    #[test]
    fn halving_contact_step_keeps_impact_energy_and_work_close() {
        let coarse = driven(1.0 / 1_500.0);
        let fine = driven(1.0 / 3_000.0);
        let relative = |a: f64, b: f64| (a - b).abs() / b.abs().max(1e-12);
        assert!(
            relative(coarse.wind_work_j, fine.wind_work_j) < 0.08,
            "{coarse:?} / {fine:?}"
        );
        assert!(
            relative(coarse.collision_loss_j, fine.collision_loss_j) < 0.12,
            "{coarse:?} / {fine:?}"
        );
        assert!(fine.constraint_absolute_work_j < coarse.constraint_absolute_work_j);
    }
}
