//! Named worlds — the initial conditions you can drop into the same engine.
//!
//! A preset never says "make a stream sound". It lays out terrain, opens a
//! spring, sets the weather, and populates the ground. The sound is whatever
//! that world does once it starts running.

use crate::critters::Species;
use crate::field::Surface;
use crate::sky::Climate;
use crate::world::World;

pub struct PresetInfo {
    pub name: &'static str,
    pub description: &'static str,
}

pub const DEFAULT: &str = "glade";

pub const PRESETS: &[PresetInfo] = &[
    PresetInfo { name: "glade", description: "a brook through a clearing, birds and leaves by day" },
    PresetInfo { name: "brook", description: "just water finding its way down a rocky channel" },
    PresetInfo { name: "cozy-rain", description: "rain on a tin roof, a swelling brook, wind chimes" },
    PresetInfo { name: "night-meadow", description: "crickets in rhythm, frogs taking turns, a far owl" },
    PresetInfo { name: "shore", description: "an ocean swell breaking up a beach, gulls, sea wind" },
    PresetInfo { name: "hearth", description: "a campfire on a still night, crickets, a breeze" },
    PresetInfo { name: "mountain", description: "wind over a high ridge, leaves, the odd bird" },
    PresetInfo { name: "storm", description: "a passing downpour: heavy rain, gusting wind, a full brook" },
];

/// C-major pentatonic chime pitches, C4..A5 — sweet and safe.
fn pentatonic() -> Vec<f32> {
    [0, 4, 7, 9, 12, 16, 19].iter().map(|s| 261.63 * 2f32.powf(*s as f32 / 12.0)).collect()
}

/// Carve a rocky brook: a channel sloping downhill, a spring at the top, an
/// outlet at the bottom, rough stones along the bed.
fn carve_brook(w: &mut World) {
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    let cx = width / 2;
    for y in 0..height {
        // Overall downhill slope so water always has somewhere to go.
        let base = (height - 1 - y) as f32 * 0.05;
        for x in 0..width {
            // A gentle V-shaped valley focuses water into the middle.
            let banks = ((x as isize - cx as isize).abs() as f32) * 0.06;
            f.set_terrain(x, y, base + banks);
            // Stones only in the channel, where the water will run.
            if (x as isize - cx as isize).abs() <= 2 {
                f.set_surface(x, y, Surface::Rock);
                f.set_rough(x, y, 1.0);
            }
        }
    }
    // A spring at the top of the channel and an outlet at the very bottom.
    for dx in 0..=1 {
        f.set_source(cx + dx, 0, 0.9);
        f.set_source(cx.wrapping_sub(dx), 0, 0.9);
    }
    for x in 0..width {
        f.set_drain(x, height - 1, 6.0);
        f.set_depth(x, height - 1, 0.0);
    }
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

/// A beach: deep water at the left edge rising to dry sand at the right, with a
/// swell forced along the deep edge. Waves travel in and break where it shoals.
fn build_beach(w: &mut World) {
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    for y in 0..height {
        for x in 0..width {
            // Terrain rises from -0.6 (deep) to +0.4 (dry) across the width.
            let t = x as f32 / (width - 1) as f32;
            let terr = -0.6 + 1.0 * t;
            f.set_terrain(x, y, terr);
            f.set_surface(x, y, Surface::Rock);
            // Foam churns most in the shallow surf zone.
            f.set_rough(x, y, if terr > -0.3 && terr < 0.2 { 1.0 } else { 0.2 });
            // Start the sea filled to the waterline.
            if terr < 0.0 {
                f.set_depth(x, y, -terr);
            }
        }
    }
    f.set_swell(0.28, 7.5, 0.0);
}

pub fn build(name: &str, sr: f32, seed: u32) -> Option<World> {
    let w = match name {
        "glade" => {
            let mut w = World::new(sr, seed, Climate::calm_day(), 20, 44, 0.2);
            carve_brook(&mut w);
            w.enable_stream(2.4);
            w.enable_leaves(0.7);
            w.enable_wind(0.9);
            w.add_chorus(Species::songbird(), 3, 0.0, 0.6, 0.0);
            w.add_chimes(&pentatonic(), 0.14);
            w.set_reverb(1.1, 0.45);
            w
        }
        "brook" => {
            let mut w = World::new(sr, seed, Climate::calm_day(), 20, 44, 0.2);
            carve_brook(&mut w);
            w.enable_stream(3.0);
            w.set_reverb(1.0, 0.4);
            w
        }
        "cozy-rain" => {
            let mut w = World::new(sr, seed, Climate::rainy(), 20, 44, 0.2);
            carve_brook(&mut w);
            // A roof over the near half of the clearing to drum on.
            {
                let f = w.field_mut();
                let width = f.width();
                let height = f.height();
                for y in 0..height {
                    for x in 0..width {
                        // A wide roof overhead; leave the channel open so the
                        // brook still runs and a little rain feeds it.
                        if (x as isize - (width / 2) as isize).abs() > 3 {
                            f.set_surface(x, y, Surface::Roof);
                        }
                    }
                }
            }
            w.enable_rain(1.4, true);
            w.enable_stream(1.6);
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
            carve_brook(&mut w);
            flat_open_top(&mut w);
            w.enable_rain(1.5, false);
            w.enable_stream(2.2);
            w.enable_wind(2.2);
            w.enable_leaves(0.7);
            w.set_reverb(1.3, 0.5);
            w
        }
        _ => return None,
    };
    Some(w)
}

/// Make sure the ground around a brook is open, so a downpour reaches the water
/// and swells it (rain → runoff → a louder stream, all on its own).
fn flat_open_top(w: &mut World) {
    let f = w.field_mut();
    let width = f.width();
    let height = f.height();
    for y in 0..height {
        for x in 0..width {
            if f.surface_at(x as f32 / width as f32, y as f32 / height as f32) != Surface::Rock {
                f.set_surface(x, y, Surface::Open);
            }
        }
    }
}
