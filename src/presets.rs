//! Named worlds — the initial conditions you can drop into the same engine.
//!
//! A preset never says "make a stream sound" — nor even where the stream
//! *is*. It raises terrain (a tilted block with random undulations —
//! tectonics), opens a spring, sets the climate, and then runs a geological
//! epoch: the water itself carves its channel and scours its rocky bed
//! before the audible world begins. What you hear is whatever geography the
//! water made.

use crate::critters::Species;
use crate::dsp::Noise;
use crate::field::Surface;
use crate::sky::Climate;
use crate::world::World;

pub struct PresetInfo {
    pub name: &'static str,
    pub description: &'static str,
}

pub const DEFAULT: &str = "glade";

pub const PRESETS: &[PresetInfo] = &[
    PresetInfo {
        name: "glade",
        description: "a brook through a clearing, birds and leaves by day",
    },
    PresetInfo {
        name: "brook",
        description: "just water finding its way down a rocky channel",
    },
    PresetInfo {
        name: "cozy-rain",
        description: "rain on a tin roof, a swelling brook, wind chimes",
    },
    PresetInfo {
        name: "night-meadow",
        description: "crickets in rhythm, frogs taking turns, a far owl",
    },
    PresetInfo {
        name: "shore",
        description: "an ocean swell breaking up a beach, gulls, sea wind",
    },
    PresetInfo {
        name: "hearth",
        description: "a campfire on a still night, crickets, a breeze",
    },
    PresetInfo {
        name: "mountain",
        description: "wind over a high ridge, leaves, the odd bird",
    },
    PresetInfo {
        name: "storm",
        description: "a passing downpour: heavy rain, gusting wind, a full brook",
    },
];

/// The chime-maker's pitches: C-major pentatonic, C4..A5. Only the *tuning
/// intent* lives here — each pitch is inverted into a bar length, and the
/// hung bar rings whatever the lattice decides.
fn pentatonic() -> Vec<f32> {
    [0, 4, 7, 9, 12, 16, 19]
        .iter()
        .map(|s| 261.63 * 2f32.powf(*s as f32 / 12.0))
        .collect()
}

/// Tectonics: raise a tilted block with gentle random undulations. No valley
/// is drawn — where the water will run is not decided here.
fn uplift(w: &mut World, seed: u32, tilt: f32, bumpiness: f32) {
    let mut rng = Noise::new(seed ^ 0x9e01_7ec7);
    let (p1, p2, p3, p4) = (
        rng.range(0.0, 6.28),
        rng.range(0.0, 6.28),
        rng.range(0.0, 6.28),
        rng.range(0.0, 6.28),
    );
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    for y in 0..height {
        let base = (height - 1 - y) as f32 * tilt;
        for x in 0..width {
            let (nx, ny) = (x as f32 / width as f32, y as f32 / height as f32);
            let bumps = (nx * 9.4 + p1).sin() * (ny * 7.1 + p2).sin()
                + 0.5 * (nx * 17.3 + p3).sin() * (ny * 13.9 + p4).sin();
            f.set_terrain(x, y, base + bumpiness * bumps);
            f.set_surface(x, y, Surface::Open);
        }
    }
}

/// Open a spring near the top of the slope and let the world's edge drain,
/// then run the geological epoch: the spring and the climate's rain carve a
/// channel and scour its bed wherever the water actually chooses to run.
fn spring_and_geology(w: &mut World, rainfall: f32) {
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    let cx = width / 2;
    for dx in 0..=1 {
        f.set_source(cx + dx, 0, 0.9);
        f.set_source(cx - dx, 0, 0.9);
    }
    for x in 0..width {
        f.set_drain(x, height - 1, 6.0);
    }
    f.geology(30_000, rainfall);
}

/// A flat clearing with open ground everywhere — rain soaks in, water pools.
fn flat_ground(w: &mut World) {
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    for y in 0..height {
        for x in 0..width {
            f.set_terrain(x, y, 0.0);
            f.set_surface(x, y, Surface::Open);
        }
    }
}

/// A shore: the continental ramp — deep water at the left edge rising to dry
/// sand at the right — with a swell forced along the deep edge. Where the
/// surf zone lies is not marked: a geological epoch of waves wears the shoal
/// where they actually break, and that wear is the roughness the foam
/// churns against.
fn build_beach(w: &mut World) {
    {
        let f = w.field_mut();
        let width = f.width();
        let height = f.height();
        for y in 0..height {
            for x in 0..width {
                let t = x as f32 / (width - 1) as f32;
                let terr = -0.6 + 1.0 * t;
                f.set_terrain(x, y, terr);
                f.set_surface(x, y, Surface::Open);
                if terr < 0.0 {
                    f.set_depth(x, y, -terr);
                }
            }
        }
        f.set_swell(0.28, 7.5, 0.0);
        f.geology(30_000, 0.0);
    }
    // The epoch drained the basin; refill the sea to the waterline over
    // whatever bed the waves have left.
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    for y in 0..height {
        for x in 0..width {
            let terr = f.terrain_at(x, y);
            if terr < 0.0 {
                f.set_depth(x, y, -terr);
            }
        }
    }
}

/// Build a hut roof over the ground the water did *not* claim: cells the
/// epoch left unscoured get a corrugated sheet overhead, so the emergent
/// streambed stays open to the sky and the rain keeps feeding it.
fn roof_off_the_stream(w: &mut World) {
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    for y in 0..height {
        for x in 0..width {
            if f.surface_at(x as f32 / width as f32, y as f32 / height as f32) != Surface::Rock {
                f.set_surface(x, y, Surface::Roof);
            }
        }
    }
}

pub fn build(name: &str, sr: f32, seed: u32) -> Option<World> {
    let mut w = match name {
        "glade" => {
            let mut w = World::new(sr, seed, Climate::calm_day(), 20, 44, 0.2);
            uplift(&mut w, seed, 0.05, 0.12);
            spring_and_geology(&mut w, 0.015);
            w.enable_stream(1.8);
            w.enable_leaves(0.7);
            w.enable_wind(0.9);
            w.add_chorus(Species::songbird(), 3, 0.0, 0.6, 0.0);
            w.add_chimes(&pentatonic(), 0.14);
            w.set_reverb(1.1, 0.45);
            w
        }
        "brook" => {
            let mut w = World::new(sr, seed, Climate::calm_day(), 20, 44, 0.2);
            uplift(&mut w, seed, 0.05, 0.12);
            spring_and_geology(&mut w, 0.015);
            w.enable_stream(2.2);
            w.set_reverb(1.0, 0.4);
            w
        }
        "cozy-rain" => {
            let mut w = World::new(sr, seed, Climate::rainy(), 20, 44, 0.2);
            uplift(&mut w, seed, 0.05, 0.12);
            spring_and_geology(&mut w, 0.03);
            roof_off_the_stream(&mut w);
            w.enable_rain(1.4, true);
            w.enable_stream(1.2);
            w.add_chimes(&pentatonic(), 0.16);
            w.set_reverb(1.2, 0.5);
            w
        }
        "night-meadow" => {
            let mut w = World::new(sr, seed, Climate::still_night(), 16, 16, 0.3);
            flat_ground(&mut w);
            w.enable_leaves(0.9);
            // Crickets pull toward each other (positive coupling → rhythm).
            w.add_chorus(Species::cricket(), 7, 0.04, 9.0, 1.0);
            // Frogs push apart (negative coupling → call and response).
            w.add_chorus(Species::frog(), 4, -0.04, 5.0, 1.0);
            w.add_chorus(Species::owl(), 1, 0.0, 5.0, 1.0);
            w.set_reverb(1.4, 0.4);
            w
        }
        "shore" => {
            let mut w = World::new(sr, seed, Climate::windy(), 44, 12, 0.5);
            build_beach(&mut w);
            w.enable_surf(3.0);
            w.enable_wind(1.3);
            w.add_chorus(Species::gull(), 3, 0.0, 5.0, 0.0);
            w.set_reverb(1.3, 0.4);
            w
        }
        "hearth" => {
            let mut w = World::new(sr, seed, Climate::still_night(), 16, 16, 0.3);
            flat_ground(&mut w);
            w.enable_fire(1.1);
            w.enable_wind(0.7);
            w.add_chorus(Species::cricket(), 4, 0.03, 4.0, 1.0);
            w.add_chorus(Species::owl(), 1, 0.0, 4.0, 1.0);
            w.set_reverb(1.0, 0.35);
            w
        }
        "mountain" => {
            let mut w = World::new(sr, seed, Climate::windy(), 16, 16, 0.3);
            flat_ground(&mut w);
            w.enable_wind(2.6);
            w.enable_leaves(0.8);
            w.add_chorus(Species::songbird(), 2, 0.0, 0.4, 0.0);
            w.set_reverb(1.6, 0.45);
            w
        }
        "storm" => {
            let mut w = World::new(sr, seed, Climate::rainy(), 20, 44, 0.2);
            uplift(&mut w, seed, 0.05, 0.12);
            spring_and_geology(&mut w, 0.03);
            w.enable_rain(1.5, false);
            w.enable_stream(1.6);
            w.enable_wind(2.2);
            w.enable_leaves(0.7);
            w.set_reverb(1.3, 0.5);
            w
        }
        _ => return None,
    };
    // The geological epoch defines the starting bed; only then does live
    // rainfall retention and sediment transport begin.
    w.field_mut().enable_history();
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brook must *emerge*: from a tilted, bumpy, unmarked plain, the
    /// geological epoch has to leave a scoured rocky bed, and the spring's
    /// water has to audibly churn along it once the world runs.
    #[test]
    fn a_channel_is_carved_and_a_brook_runs_in_it() {
        let mut w = build("brook", 48_000.0, 12345).unwrap();
        {
            let f = w.field_mut();
            let (width, height) = (f.width(), f.height());
            let mut rock = 0;
            for y in 0..height {
                for x in 0..width {
                    if f.surface_at(x as f32 / width as f32, y as f32 / height as f32)
                        == Surface::Rock
                    {
                        rock += 1;
                    }
                }
            }
            let total = width * height;
            assert!(
                rock > total / 50,
                "geology should scour some cells to rock (got {rock})"
            );
            assert!(
                rock < total / 2,
                "geology should not scour everything (got {rock}/{total})"
            );
        }
        // Let the spring refill its channel, then listen to the water.
        w.probe(12.0);
        let (flow_e, _) = w.field_mut().flow();
        assert!(
            flow_e > 0.02,
            "the brook should be running and churning (flow_e = {flow_e})"
        );
    }
}
