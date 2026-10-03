//! Physical assembly interfaces shared by discovered and named worlds.
use super::*;
use crate::acoustics::HearingScene;

pub struct ResonantBody {
    pub matter: Matter,
    pub length: f32,
    pub thickness: f32,
    pub nodes: usize,
    pub pan: f32,
    pub gain: f32,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsReport {
    pub contacts: u64,
    pub cloud_events: u64,
    pub emitted_calls: u64,
    pub heard_calls: u64,
    pub retained_water: f64,
    pub moved_sediment: f64,
    pub wind_work: f64,
    pub contact_residual: f64,
}

impl World {
    /// Set physical eddy scales instead of choosing named environmental voices.
    pub fn set_flows(&mut self, water: Eddies, air: Eddies, gains: [f32; 2]) {
        self.stream = Some((
            Turbulence::new(water, self.sr, self.seed ^ 0x57, gains[0]),
            0.12,
        ));
        self.wind = Some(Turbulence::new(air, self.sr, self.seed ^ 0x1d, gains[1]));
    }

    /// Suspended bodies share the existing passive contact solver. Geometry
    /// and material determine their spectra; no desired pitches are supplied.
    pub fn set_resonant_bodies(&mut self, bodies: &[ResonantBody]) {
        self.chimes = bodies
            .iter()
            .enumerate()
            .map(|(i, b)| Chime {
                body: Body::bar(
                    &b.matter,
                    b.length,
                    b.thickness,
                    b.nodes,
                    self.sr,
                    self.seed ^ (0x3a + i as u32),
                    b.gain,
                ),
                pan: b.pan,
                length_m: b.length,
            })
            .collect();
        self.chime_rig = Some(ChimeRig::new(
            &bodies.iter().map(|b| b.length).collect::<Vec<_>>(),
            self.seed,
        ));
    }

    pub fn add_population(
        &mut self,
        species: Species,
        scene: HearingScene,
        coupling: f32,
        gain: f32,
        nocturnal: f32,
    ) -> Result<(), String> {
        let chorus = Chorus::with_scene(
            species,
            coupling,
            self.sr,
            self.seed.wrapping_add(0x600d + self.choruses.len() as u32),
            scene,
        )?;
        self.choruses.push(Voices {
            chorus,
            gain,
            nocturnal,
        });
        Ok(())
    }

    pub fn physics_report(&self) -> PhysicsReport {
        let history = self.field.history_stats();
        let contact = self
            .chime_rig
            .as_ref()
            .map(ChimeRig::diagnostics)
            .unwrap_or_default();
        let mut report = PhysicsReport {
            contacts: self.chime_strikes,
            cloud_events: self.cloud_events,
            retained_water: history.retained_water_m3,
            moved_sediment: history.suspended_solid_m3 + history.exported_solid_m3,
            wind_work: contact.wind_work_j,
            contact_residual: contact.integration_residual_j,
            ..PhysicsReport::default()
        };
        for population in &self.choruses {
            let stats = population.chorus.hearing_stats();
            report.emitted_calls += stats.emitted_calls;
            report.heard_calls += stats.heard_calls;
        }
        report
    }
}
