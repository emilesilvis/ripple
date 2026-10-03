//! A quality-diversity archive plus an immutable, browsable world library.
//! New cells win by being different; occupied cells improve by calmness proxy.
pub use super::analysis::Character;
use super::analysis::{measure, ANALYSIS_RATE};
use super::worlds::{Soundscape, WorldSpec};
use super::Random;
use crate::world::PhysicsReport;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const DEFAULT_DIRECTORY: &str = "discoveries/atlas";
const FORMAT: &str = "ripple-world-v2";
const MAX_FILE_BYTES: u64 = 64 * 1024;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    format: String,
    pub spec: WorldSpec,
    pub character: Character,
    pub physics: PhysicsReport,
    pub gain: f64,
    pub parent: Option<String>,
}

fn valid_id(id: &str) -> bool {
    id.len() == 16
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Entry {
    pub fn evaluate(spec: WorldSpec, parent: Option<String>, cancel: &AtomicBool) -> Result<Self> {
        let mut scene = Soundscape::new(&spec, ANALYSIS_RATE as f32, 0.5 / 0.9)?;
        let mut character = measure(&mut scene, cancel)?;
        let adjustment = (0.035 / character.rms).min(1.0);
        character.rms *= adjustment;
        character.peak *= adjustment;
        let result = Self {
            format: FORMAT.into(),
            spec,
            character,
            physics: scene.physics_report(),
            gain: adjustment * (0.5 / 0.9),
            parent,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn id(&self) -> String {
        // Stable FNV-1a identity over the exact versioned recipe. Never use a
        // platform-dependent DefaultHasher or file name as world identity.
        let mut hash = 0xcbf29ce484222325u64;
        for b in FORMAT
            .bytes()
            .chain(serde_json::to_vec(&self.spec).unwrap())
        {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        format!("{hash:016x}")
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != FORMAT {
            return Err("unsupported world version".into());
        }
        self.spec.validate()?;
        self.character.validate()?;
        if !self.gain.is_finite()
            || !(0.0..=0.5 / 0.9).contains(&self.gain)
            || self.parent.as_ref().is_some_and(|id| !valid_id(id))
        {
            return Err("invalid gain or parent identity".into());
        }
        for v in [
            self.physics.retained_water,
            self.physics.moved_sediment,
            self.physics.wind_work,
            self.physics.contact_residual,
        ] {
            if !v.is_finite() {
                return Err("non-finite physics diagnostic".into());
            }
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        if std::fs::metadata(path)?.len() > MAX_FILE_BYTES {
            return Err("world file is too large".into());
        }
        let result: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        result.validate()?;
        Ok(result)
    }

    pub fn play(&self, sr: f32) -> std::result::Result<Soundscape, String> {
        Soundscape::new(&self.spec, sr, self.gain)
    }
}

/// Publish a complete file without ever replacing an existing file. This also
/// works with two explorers writing the same library simultaneously.
fn save_immutable(path: &Path, data: &[u8]) -> Result<()> {
    if path.exists() {
        if std::fs::read(path)? == data {
            return Ok(());
        }
        return Err(format!("a different world already exists at {}", path.display()).into());
    }
    let parent = path.parent().ok_or("save path has no directory")?;
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".pending-{}-{}",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(data)?;
        file.sync_all()?;
        match std::fs::hard_link(&temp, path) {
            Ok(()) => Ok(()),
            Err(e)
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    && std::fs::read(path)? == data =>
            {
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    })();
    let _ = std::fs::remove_file(temp);
    result
}

#[derive(Clone)]
pub struct Library {
    pub directory: PathBuf,
    pub entries: BTreeMap<String, Entry>,
    pub favourites: BTreeSet<String>,
    cells: BTreeMap<[u8; 4], String>,
    pub warnings: Vec<String>,
}

impl Library {
    pub fn open(directory: &Path) -> Result<Self> {
        let mut library = Self {
            directory: directory.to_owned(),
            entries: BTreeMap::new(),
            favourites: BTreeSet::new(),
            cells: BTreeMap::new(),
            warnings: Vec::new(),
        };
        let worlds = directory.join("worlds");
        if !worlds.exists() {
            return Ok(library);
        }
        let mut paths: Vec<_> = std::fs::read_dir(worlds)?
            .filter_map(|v| v.ok().map(|v| v.path()))
            .filter(|p| p.extension().is_some_and(|v| v == "world"))
            .collect();
        paths.sort();
        for path in paths {
            match Entry::load(&path) {
                Ok(entry) if path.file_stem().is_some_and(|s| s == entry.id().as_str()) => {
                    let id = entry.id();
                    if directory.join("favourites").join(&id).is_file() {
                        library.favourites.insert(id);
                    }
                    library.insert(entry);
                }
                Ok(_) => library.warnings.push(format!(
                    "{}: identity does not match recipe",
                    path.display()
                )),
                Err(e) => library.warnings.push(format!("{}: {e}", path.display())),
            }
        }
        Ok(library)
    }

    pub fn occupied(&self) -> usize {
        self.cells.len()
    }
    pub fn champions(&self) -> Vec<String> {
        self.cells.values().cloned().collect()
    }

    pub fn accepts(&self, entry: &Entry) -> bool {
        if self.entries.contains_key(&entry.id()) {
            return false;
        }
        self.cells
            .get(&entry.character.cell())
            .is_none_or(|id| entry.character.score() < self.entries[id].character.score())
    }

    pub fn insert(&mut self, entry: Entry) {
        let id = entry.id();
        let cell = entry.character.cell();
        let improves = self
            .cells
            .get(&cell)
            .is_none_or(|old| entry.character.score() < self.entries[old].character.score());
        if improves {
            self.cells.insert(cell, id.clone());
        }
        self.entries.entry(id).or_insert(entry);
    }

    pub fn save(&mut self, entry: Entry) -> Result<()> {
        entry.validate()?;
        let path = self
            .directory
            .join("worlds")
            .join(format!("{}.world", entry.id()));
        save_immutable(&path, &serde_json::to_vec_pretty(&entry)?)?;
        self.insert(entry);
        Ok(())
    }

    pub fn toggle_favourite(&mut self, id: &str) -> Result<bool> {
        if !self.entries.contains_key(id) {
            return Err("world is not in this library".into());
        }
        let path = self.directory.join("favourites").join(id);
        if self.favourites.contains(id) {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            self.favourites.remove(id);
            Ok(false)
        } else {
            save_immutable(&path, b"favourite\n")?;
            self.favourites.insert(id.to_owned());
            Ok(true)
        }
    }

    /// Search a whole population of regions, not an ascending chain of one
    /// winner. Half the global proposals are fresh starts, half archive mutants.
    /// A local search uses the selected world as its parent for every proposal.
    pub fn scout(
        &mut self,
        seed: u32,
        count: usize,
        parent: Option<Entry>,
        cancel: &AtomicBool,
        mut update: impl FnMut(Progress, Option<Entry>) -> bool,
    ) -> Result<Progress> {
        let mut rng = Random(u64::from(seed) ^ 0xd17e_u64);
        let mut progress = Progress {
            total: count,
            ..Progress::default()
        };
        let mut local_cells = BTreeMap::<[u8; 4], f64>::new();
        for _ in 0..count {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let chosen = if let Some(p) = &parent {
                Some(p.clone())
            } else if !self.cells.is_empty() && rng.unit() < 0.5 {
                let id = self
                    .cells
                    .values()
                    .nth(rng.index(self.cells.len()))
                    .unwrap();
                Some(self.entries[id].clone())
            } else {
                None
            };
            let spec = if let Some(p) = &chosen {
                p.spec.mutate(rng.bits())
            } else {
                WorldSpec::random(rng.bits() as u32)
            };
            let ancestry = chosen.as_ref().map(Entry::id);
            let mut accepted = None;
            match Entry::evaluate(spec, ancestry, cancel) {
                Ok(entry)
                    if if parent.is_some() {
                        !self.entries.contains_key(&entry.id())
                            && local_cells
                                .get(&entry.character.cell())
                                .is_none_or(|score| entry.character.score() < *score)
                    } else {
                        self.accepts(&entry)
                    } =>
                {
                    local_cells.insert(entry.character.cell(), entry.character.score());
                    self.save(entry.clone())?;
                    accepted = Some(entry);
                    progress.accepted += 1;
                }
                Ok(_) => {}
                Err(_) if cancel.load(Ordering::Relaxed) => break,
                Err(error) => {
                    progress.rejected += 1;
                    progress.last_error = Some(error.to_string());
                }
            }
            progress.done += 1;
            progress.occupied = self.occupied();
            if !update(progress.clone(), accepted) {
                break;
            }
        }
        Ok(progress)
    }
}

#[derive(Clone, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub accepted: usize,
    pub occupied: usize,
    pub rejected: usize,
    pub last_error: Option<String>,
}

pub fn render(entry: &Entry, path: &Path, seconds: f32) -> Result<()> {
    let sr = 48_000;
    let mut scene = entry.play(sr as f32)?;
    let mut writer = hound::WavWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?,
        hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )?;
    let count = (seconds * sr as f32) as usize;
    let mut peak = 0.0_f32;
    let mut squares = 0.0;
    for _ in 0..count {
        let (l, r) = scene.next_sample();
        for sample in [l, r] {
            if !sample.is_finite() || sample.abs() > 0.500001 {
                return Err("alien audio exceeded its output bound".into());
            }
            peak = peak.max(sample.abs());
            squares += f64::from(sample).powi(2);
            writer.write_sample((sample * i16::MAX as f32) as i16)?;
        }
    }
    writer.finalize()?;
    println!(
        "{} | {} | peak {peak:.4} RMS {:.5}",
        entry.id(),
        entry.character.label(),
        (squares / (count * 2) as f64).sqrt()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ripple-atlas-test-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn example(seed: u32, brightness: f64, roughness: f64) -> Entry {
        Entry {
            format: FORMAT.into(),
            spec: WorldSpec::random(seed),
            character: Character {
                brightness,
                texture: 0.6,
                motion: 0.2,
                width: 0.1,
                roughness,
                high_fraction: 0.05,
                onsets_per_second: 1.0,
                rms: 0.02,
                peak: 0.1,
            },
            physics: PhysicsReport::default(),
            gain: 0.4,
            parent: None,
        }
    }

    #[test]
    fn archive_preserves_distinct_regions_and_favourites_survive_replacement_and_reload() {
        let path = directory();
        let mut library = Library::open(&path).unwrap();
        let old = example(1, 300.0, 0.05);
        let id = old.id();
        library.save(old.clone()).unwrap();
        library.toggle_favourite(&id).unwrap();
        let worse = example(2, 310.0, 0.08);
        assert!(!library.accepts(&worse));
        let different = example(3, 1800.0, 0.1);
        assert!(
            library.accepts(&different),
            "a different region must not compete with the global best"
        );
        library.save(different).unwrap();
        let better = example(4, 310.0, 0.01);
        assert!(library.accepts(&better));
        let winner = better.id();
        library.save(better).unwrap();
        assert_eq!(library.occupied(), 2);
        assert!(library.champions().contains(&winner));
        let reopened = Library::open(&path).unwrap();
        assert!(reopened.warnings.is_empty(), "{:?}", reopened.warnings);
        assert_eq!(reopened.entries.len(), 3);
        assert!(reopened.favourites.contains(&id));
        assert!(reopened.entries.contains_key(&id));
        assert_eq!(reopened.champions(), library.champions());
        // Repeated saves do not replace content or alter the chosen favourite.
        library.save(old.clone()).unwrap();
        let saved = path.join("worlds").join(format!("{id}.world"));
        let expected = std::fs::read(&saved).unwrap();
        let mut changed = old;
        changed.gain = 0.3;
        assert!(library.save(changed).is_err());
        assert_eq!(std::fs::read(saved).unwrap(), expected);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn invalid_recipes_and_versions_are_rejected_and_corrupt_files_are_reported() {
        let original = example(12345, 300.0, 0.01);
        let mut data = serde_json::to_value(&original).unwrap();
        data["spec"]["populations"][0]["organ"]["bonds"][0]["a"] = serde_json::json!(usize::MAX);
        let bad: Entry = serde_json::from_value(data).unwrap();
        assert!(bad.validate().is_err());
        let mut version = original.clone();
        version.format = "ripple-world-v99".into();
        assert!(version.validate().is_err());
        let mut gain = original.clone();
        gain.gain = f64::INFINITY;
        assert!(gain.validate().is_err());
        let path = directory();
        let mut library = Library::open(&path).unwrap();
        library.save(original).unwrap();
        std::fs::write(path.join("worlds/broken.world"), b"{broken").unwrap();
        let reopened = Library::open(&path).unwrap();
        assert_eq!(reopened.entries.len(), 1);
        assert_eq!(reopened.warnings.len(), 1);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn exact_recipe_round_trip_replays_audio_and_cancelled_search_does_no_work() {
        let entry = example(42, 300.0, 0.01);
        let copy: Entry = serde_json::from_slice(&serde_json::to_vec(&entry).unwrap()).unwrap();
        assert_eq!(copy.id(), entry.id());
        let mut a = entry.play(16_000.0).unwrap();
        let mut b = copy.play(16_000.0).unwrap();
        for _ in 0..16_000 {
            assert_eq!(a.next_sample(), b.next_sample());
        }
        let path = directory();
        let mut library = Library::open(&path).unwrap();
        let p = library
            .scout(1, 10, None, &AtomicBool::new(true), |_, _| {
                panic!("cancelled")
            })
            .unwrap();
        assert_eq!(p.done, 0);
        assert!(!path.exists());
    }

    #[test]
    #[ignore = "rendered quality-diversity and shared-physics integration audit"]
    fn generated_worlds_cover_regions_and_exercise_all_four_shared_mechanisms() {
        let path = directory();
        let mut library = Library::open(&path).unwrap();
        let progress = library
            .scout(12345, 16, None, &AtomicBool::new(false), |_, _| true)
            .unwrap();
        println!(
            "{} regions, {} saved, {} rejected",
            library.occupied(),
            library.entries.len(),
            progress.rejected
        );
        assert_eq!(progress.done, 16);
        assert_eq!(progress.rejected, 0);
        assert!(library.occupied() >= 6);
        let mut brightness = BTreeSet::new();
        let mut width = BTreeSet::new();
        let mut contacts = 0;
        let mut heard = 0;
        for entry in library.entries.values() {
            let cell = entry.character.cell();
            brightness.insert(cell[0]);
            width.insert(cell[3]);
            let p = entry.physics;
            contacts += p.contacts;
            heard += p.heard_calls;
            assert!(p.cloud_events > 0 && p.retained_water > 0.0 && p.moved_sediment > 0.0);
            assert!(p.wind_work > 0.0 && p.contact_residual.abs() < 1e-7);
        }
        assert!(contacts > 0 && heard > 0);
        assert!(brightness.len() >= 2 && width.len() >= 2);
        let reopened = Library::open(&path).unwrap();
        assert!(reopened.warnings.is_empty(), "{:?}", reopened.warnings);
        assert_eq!(reopened.champions(), library.champions());
        // Longer listening at the real player rate checks the stored gain's
        // lifetime bound beyond the short canonical analysis window.
        for id in library.champions().iter().take(3) {
            let mut scene = library.entries[id].play(48_000.0).unwrap();
            let mut squares = 0.0;
            for _ in 0..48_000 * 30 {
                let (l, r) = scene.next_sample();
                assert!(l.is_finite() && r.is_finite() && l.abs().max(r.abs()) <= 0.500001);
                squares += f64::from(l).powi(2) + f64::from(r).powi(2);
            }
            assert!(squares > 0.01);
        }
        std::fs::remove_dir_all(path).unwrap();
    }
}
