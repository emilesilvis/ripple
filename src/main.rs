//! ripple — a tiny simulated world whose gentle soundscapes emerge from a few
//! axiomatic laws.
//!
//! Nothing here is a "rain sound" or a "wind sound". There is water that flows,
//! air that moves, materials that ring when struck, and creatures that sing.
//! You choose a world (a set of initial conditions), press play, and listen to
//! what it does.
//!
//!   cargo run --release                       # the default world (glade)
//!   cargo run --release -- shore              # a named world
//!   cargo run --release -- list               # all worlds
//!   cargo run --release -- render shore out.wav 60

mod audio;
mod critters;
mod dsp;
mod field;
mod matter;
mod presets;
mod sky;
mod voices;
mod world;

use rand::{Rng, SeedableRng};

fn fresh_seed() -> u32 {
    rand::rngs::SmallRng::from_entropy().gen()
}

fn print_list() {
    println!("worlds:");
    for p in presets::PRESETS {
        println!("  {:14} {}", p.name, p.description);
    }
    println!("\n(the default world is `{}`)", presets::DEFAULT);
}

fn print_help() {
    println!(
        "ripple — a world you can hear\n\nusage:\n  ripple                       run the default world ({default})\n  ripple <world>               run a named world\n  ripple list                  list the worlds\n  ripple render [world] [out.wav] [seconds]\n                               render to a WAV file (default: {default}, ripple.wav, 30)\n\npress Ctrl+C to stop live playback.",
        default = presets::DEFAULT
    );
}

fn description_of(name: &str) -> String {
    presets::PRESETS
        .iter()
        .find(|p| p.name == name)
        .map(|p| format!("{} — {}", p.name, p.description))
        .unwrap_or_else(|| name.to_string())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("list") => {
            print_list();
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some("probe") => {
            let name = args.get(2).cloned().unwrap_or_else(|| presets::DEFAULT.to_string());
            let seconds = args.get(3).and_then(|s| s.parse::<f32>().ok()).unwrap_or(20.0);
            let mut world = presets::build(&name, 48_000.0, fresh_seed())
                .ok_or_else(|| format!("unknown world: {name}"))?;
            world.probe(seconds);
            Ok(())
        }
        Some("sync") => {
            let name = args.get(2).cloned().unwrap_or_else(|| "night-meadow".to_string());
            let seconds = args.get(3).and_then(|s| s.parse::<f32>().ok()).unwrap_or(60.0);
            let mut world = presets::build(&name, 48_000.0, fresh_seed())
                .ok_or_else(|| format!("unknown world: {name}"))?;
            world.probe_sync(seconds);
            Ok(())
        }
        Some("render") => {
            let mut name = presets::DEFAULT.to_string();
            let mut path = "ripple.wav".to_string();
            let mut seconds = 30.0f32;
            for a in &args[2..] {
                if let Ok(s) = a.parse::<f32>() {
                    seconds = s;
                } else if presets::PRESETS.iter().any(|p| p.name == a.as_str()) {
                    name = a.clone();
                } else {
                    path = a.clone();
                }
            }
            let sr = 48_000u32;
            let world = presets::build(&name, sr as f32, fresh_seed())
                .ok_or_else(|| format!("unknown world: {name}"))?;
            audio::render_wav(world, sr, &path, seconds)
        }
        arg => {
            let name = arg.unwrap_or(presets::DEFAULT).to_string();
            if presets::build(&name, 48_000.0, 0).is_none() {
                eprintln!("unknown world: {name}\n");
                print_list();
                std::process::exit(1);
            }
            let desc = description_of(&name);
            let seed = fresh_seed();
            audio::run_live(move |sr| presets::build(&name, sr, seed).unwrap(), &desc)
        }
    }
}
