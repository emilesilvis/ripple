//! ripple — a tiny simulated world whose gentle soundscapes emerge from
//! moving water, air, matter and creatures. The terminal player lets you
//! choose a world and listen to it evolve.

mod acoustics;
mod alien;
mod audio;
mod bubbles;
mod contacts;
mod critters;
mod dsp;
mod events;
mod field;
mod matter;
mod presets;
mod sky;
mod tui;
mod visual;
mod voices;
mod world;

use rand::{Rng, SeedableRng};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
        r#"ripple — a world you can hear

usage:
  ripple [world]              open the live world player (default: {default})
  ripple discover [file.alien]
                             discover calm resonator networks, or reopen a saved pair
  ripple atlas [directory]    browse and discover full alien worlds
  ripple scout [count] [directory]
                             explore offline (default: 24 candidates, discoveries/atlas)
  ripple render-alien <file.world> <out.wav> [seconds]
                             render a saved alien world (default: 30 seconds)
  ripple list                list worlds
  ripple render [world] [out.wav] [seconds]
                             render a world (default: {default}, ripple.wav, 30)
  ripple probe [world] [seconds]
                             inspect field physics (default: {default}, 20)
  ripple sync [world] [seconds]
                             inspect chorus clocks (default: night-meadow, 60)
  ripple trace [world|file.world] [seconds]
                             emit simulation events as JSON lines (default: {default}, 10)

--seed <u32> is accepted anywhere. A fresh seed is chosen unless supplied.

live controls:
  Up/Down or j/k      highlight a world
  Enter               play highlighted world
  Space               pause / resume
  +/-                 volume
  r                   restart with the same seed
  n                   restart with a new seed
  v                   switch between the visualization and browser
  l                   toggle the live event log (in every player)
  q or Esc            quit

visualization:
  Opens automatically and adapts to the soundscape; no view settings.
  v or Up/Down        browse worlds; Enter plays with its visualization
  l                   open the event log; l again returns to the picture
  Space, +/-, r, q     pause, volume, restart, quit
  a/b in discovery    compare candidates; the picture follows the audio blend

event log controls:
  Tab / Shift-Tab     filter All, Calls, Contacts, Water, Weather, Resonators
  h / End             hold the view / return to the live tail
  Arrows or PgUp/Dn    scroll history; audio keeps playing
  Space, +/-, r, q     pause audio, volume, restart, quit
  l                   return to the visualization

discovery controls:
  a/b                 compare parent / automatically selected descendant
  e                   keep the audible candidate and search its descendants
  s                   save the pair and lineage in discoveries/
  Space, +/-, r, n, q work as in the world player.
  Saved discoveries contain their seed; do not combine a file with --seed.

atlas controls:
  Up/Down or j/k      browse saved worlds; Enter listens
  f / Tab             favourite / cycle Atlas, Favourites, All saved
  g                   explore 24 global candidates
  e                   explore 12 neighbours of the highlighted world
  p / x               revisit parent / cancel search
  Space, +/-, r, q     pause, volume, replay, quit
  Accepted worlds save automatically; favourites survive further searches."#,
        default = presets::DEFAULT
    );
}

/// Remove a global seed flag without disturbing the command's positional args.
fn arguments(args: impl IntoIterator<Item = String>) -> Result<(Vec<String>, Option<u32>)> {
    let mut remaining = Vec::new();
    let mut seed = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let seed_text = if arg == "--seed" {
            Some(
                args.next()
                    .ok_or("--seed requires an unsigned 32-bit integer")?,
            )
        } else {
            arg.strip_prefix("--seed=").map(str::to_owned)
        };
        if let Some(value) = seed_text {
            if seed.is_some() {
                return Err("--seed may only be specified once".into());
            }
            seed = Some(
                value
                    .parse::<u32>()
                    .map_err(|_| "--seed requires an unsigned 32-bit integer")?,
            );
        } else if arg.starts_with("--") && arg != "--help" {
            return Err(format!("unknown option: {arg}").into());
        } else {
            remaining.push(arg);
        }
    }
    Ok((remaining, seed))
}

fn duration(arg: Option<&String>, default: f32) -> Result<f32> {
    let seconds = match arg {
        Some(text) => text
            .parse::<f32>()
            .map_err(|_| format!("invalid duration: {text}"))?,
        None => default,
    };
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("duration must be finite and greater than zero".into());
    }
    Ok(seconds)
}

fn check_length(args: &[String], max: usize, usage: &str) -> Result<()> {
    if args.len() > max {
        return Err(format!("too many arguments; usage: {usage}").into());
    }
    Ok(())
}

fn check_world(name: &str) -> Result<()> {
    if presets::PRESETS.iter().any(|p| p.name == name) {
        Ok(())
    } else {
        Err(format!("unknown world: {name}; use `ripple list` to see the worlds").into())
    }
}

fn main() -> Result<()> {
    let (args, seed) = arguments(std::env::args().skip(1))?;
    match args.first().map(String::as_str) {
        Some("list") => {
            check_length(&args, 1, "ripple list")?;
            print_list();
            Ok(())
        }
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some("discover") => {
            check_length(&args, 2, "ripple discover [file.alien] [--seed <u32>]")?;
            let discovery = match args.get(1) {
                Some(path) => {
                    if seed.is_some() {
                        return Err("a saved discovery contains its seed; omit --seed".into());
                    }
                    alien::Discovery::load(std::path::Path::new(path))?
                }
                None => alien::Discovery::new(seed.unwrap_or_else(fresh_seed)),
            };
            tui::discover(discovery)
        }
        Some("atlas") => {
            check_length(&args, 2, "ripple atlas [directory] [--seed <u32>]")?;
            let directory = args
                .get(1)
                .map(String::as_str)
                .unwrap_or(alien::atlas::DEFAULT_DIRECTORY);
            tui::library::run(
                std::path::Path::new(directory),
                seed.unwrap_or_else(fresh_seed),
            )
        }
        Some("scout") => {
            check_length(&args, 3, "ripple scout [count] [directory] [--seed <u32>]")?;
            let count = args
                .get(1)
                .map(|s| s.parse::<usize>())
                .transpose()?
                .unwrap_or(24);
            if !(1..=512).contains(&count) {
                return Err("candidate count must be between 1 and 512".into());
            }
            let directory = args
                .get(2)
                .map(String::as_str)
                .unwrap_or(alien::atlas::DEFAULT_DIRECTORY);
            let mut library = alien::atlas::Library::open(std::path::Path::new(directory))?;
            for warning in &library.warnings {
                eprintln!("Skipped: {warning}");
            }
            let seed = seed.unwrap_or_else(fresh_seed);
            println!("Exploring with seed {seed}; saving in {directory}");
            library.scout(
                seed,
                count,
                None,
                &std::sync::atomic::AtomicBool::new(false),
                |p, entry| {
                    if let Some(e) = entry {
                        println!("  {} {}", e.id(), e.character.label());
                    }
                    println!(
                        "{}/{} explored | {} kept | {} regions | {} rejected",
                        p.done, p.total, p.accepted, p.occupied, p.rejected
                    );
                    if let Some(error) = p.last_error {
                        eprintln!("Last rejected candidate: {error}");
                    }
                    true
                },
            )?;
            Ok(())
        }
        Some("render-alien") => {
            check_length(
                &args,
                4,
                "ripple render-alien <file.world> <out.wav> [seconds]",
            )?;
            if seed.is_some() {
                return Err("a saved world contains its seed; omit --seed".into());
            }
            let input = args
                .get(1)
                .ok_or("render-alien needs a saved .world file")?;
            let output = args.get(2).ok_or("render-alien needs an output WAV path")?;
            let entry = alien::atlas::Entry::load(std::path::Path::new(input))?;
            alien::atlas::render(
                &entry,
                std::path::Path::new(output),
                duration(args.get(3), 30.0)?,
            )
        }
        Some("trace") => {
            check_length(&args, 3, "ripple trace [world|file.world] [seconds]")?;
            let name = args.get(1).map(String::as_str).unwrap_or(presets::DEFAULT);
            let seconds = duration(args.get(2), 10.0)?;
            let world = if std::path::Path::new(name)
                .extension()
                .is_some_and(|e| e == "world")
            {
                if seed.is_some() {
                    return Err("a saved world contains its seed; omit --seed".into());
                }
                alien::atlas::Entry::load(std::path::Path::new(name))?
                    .spec
                    .build(48_000.0)?
            } else {
                check_world(name)?;
                presets::build(name, 48_000.0, seed.unwrap_or_else(fresh_seed)).unwrap()
            };
            events::trace(world, 48_000, seconds, std::io::stdout().lock())
        }
        Some(command @ ("probe" | "sync")) => {
            check_length(&args, 3, "ripple probe|sync [world] [seconds]")?;
            let default = if command == "sync" {
                "night-meadow"
            } else {
                presets::DEFAULT
            };
            let name = args.get(1).map(String::as_str).unwrap_or(default);
            check_world(name)?;
            let seconds = duration(args.get(2), if command == "sync" { 60.0 } else { 20.0 })?;
            let seed = seed.unwrap_or_else(fresh_seed);
            println!("seed: {seed}");
            let mut world = presets::build(name, 48_000.0, seed).unwrap();
            if command == "sync" {
                world.probe_sync(seconds);
            } else {
                world.probe(seconds);
            }
            Ok(())
        }
        Some("render") => {
            // Preserve the existing loose ordering of world, path and duration.
            check_length(&args, 4, "ripple render [world] [out.wav] [seconds]")?;
            let mut name = presets::DEFAULT;
            let mut path = "ripple.wav";
            let mut seconds = 30.0f32;
            for arg in &args[1..] {
                if arg.parse::<f32>().is_ok() {
                    seconds = duration(Some(arg), 30.0)?;
                } else if presets::PRESETS.iter().any(|p| p.name == arg.as_str()) {
                    name = arg;
                } else {
                    path = arg;
                }
            }
            let seed = seed.unwrap_or_else(fresh_seed);
            println!("seed: {seed}");
            let world = presets::build(name, 48_000.0, seed).unwrap();
            audio::render_wav(world, 48_000, path, seconds)
        }
        name => {
            check_length(&args, 1, "ripple [world]")?;
            let name = name.unwrap_or(presets::DEFAULT);
            check_world(name)?;
            tui::run(name, seed.unwrap_or_else(fresh_seed))
        }
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    fn parse(values: &[&str]) -> Result<(Vec<String>, Option<u32>)> {
        arguments(values.iter().map(|v| v.to_string()))
    }

    #[test]
    fn global_seed_preserves_positionals_before_or_after_the_command() {
        let expected = (vec!["probe".to_string(), "brook".to_string()], Some(42));
        assert_eq!(
            parse(&["--seed", "42", "probe", "brook"]).unwrap(),
            expected
        );
        assert_eq!(
            parse(&["probe", "brook", "--seed", "42"]).unwrap(),
            expected
        );
        assert_eq!(parse(&["probe", "--seed=42", "brook"]).unwrap(), expected);
    }

    #[test]
    fn malformed_seeds_and_nonphysical_durations_fail_before_running() {
        for args in [
            vec!["--seed"],
            vec!["--seed", "-1"],
            vec!["--seed", "4294967296"],
            vec!["--seed", "1", "--seed", "2"],
        ] {
            assert!(parse(&args).is_err());
        }
        for seconds in ["NaN", "inf", "0", "-1", "tomorrow"] {
            assert!(duration(Some(&seconds.to_string()), 30.0).is_err());
        }
        assert_eq!(duration(None, 30.0).unwrap(), 30.0);
        assert_eq!(duration(Some(&"0.5".to_string()), 30.0).unwrap(), 0.5);
    }
}
