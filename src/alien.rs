//! Discover small elastic networks inside deliberately calm acoustic laws.
//!
//! There are no note lists, species, recorded sounds or prescribed overtones.
//! Positive masses and springs define M x'' + C x' + K x = F. A symmetric
//! eigensolve discovers its modes; mutations change both material and topology.
//! The finite search favors a spectral roughness proxy, with a penalty for
//! losing spectral diversity. This is an aesthetic prior, not proof of pleasure.
//!
//! Shared excitation power, at most two active drivers, soft excitation
//! envelopes, frequency-dependent loss and a mechanical energy ceiling apply
//! to BOTH candidates. The normalized velocity pickups bound digital output.
//! This is a small invented material/actuation model, not an alien biosphere.

use std::f64::consts::{PI, TAU};
use std::path::{Path, PathBuf};

mod analysis;
pub mod atlas;
pub mod worlds;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Matrix = [[f64; MAX_NODES]; MAX_NODES];
const MAX_NODES: usize = 12;
const MIN_NODES: usize = 4;
const MAX_ENERGY: f64 = 0.025;
const MAX_PEAK: f64 = 0.5;
const MATCH_SECONDS: f64 = 12.0;
const SEARCH_ROUNDS: usize = 18;
const CHILDREN: usize = 6;

// A specified integer PRNG keeps saved lineages independent of rand versions.
#[derive(Clone)]
struct Random(u64);

impl Random {
    fn bits(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn unit(&mut self) -> f64 {
        (self.bits() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn index(&mut self, count: usize) -> usize {
        (self.bits() % count as u64) as usize
    }

    fn log_range(&mut self, lo: f64, hi: f64) -> f64 {
        self.range(lo.ln(), hi.ln()).exp()
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    mass: f64,
    anchor: f64,
    pan: f64,
    threshold: f64,
}

impl Node {
    fn random(rng: &mut Random) -> Self {
        Self {
            mass: rng.log_range(0.5, 2.0),
            anchor: rng.log_range(1.0e6, 2.0e7),
            pan: rng.range(0.05, 0.95),
            threshold: rng.range(0.15, 0.7),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Bond {
    a: usize,
    b: usize,
    stiffness: f64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Genome {
    nodes: Vec<Node>,
    bonds: Vec<Bond>,
    loss: f64,
}

impl Genome {
    fn random(rng: &mut Random) -> Self {
        let count = 6 + rng.index(4);
        let mut genome = Self {
            nodes: (0..count).map(|_| Node::random(rng)).collect(),
            bonds: Vec::new(),
            loss: rng.range(0.6, 2.2),
        };
        for i in 1..count {
            genome.bonds.push(Bond {
                a: rng.index(i),
                b: i,
                stiffness: rng.log_range(3.0e5, 8.0e6),
            });
        }
        genome
    }

    fn mutate(&self, rng: &mut Random) -> Self {
        let mut child = self.clone();
        let n = child.nodes.len();
        match rng.index(5) {
            0 if n < MAX_NODES => {
                child.nodes.push(Node::random(rng));
                child.bonds.push(Bond {
                    a: rng.index(n),
                    b: n,
                    stiffness: rng.log_range(3.0e5, 8.0e6),
                });
            }
            1 if n > MIN_NODES => {
                let removed = rng.index(n);
                child.nodes.remove(removed);
                child.bonds.retain(|b| b.a != removed && b.b != removed);
                for bond in &mut child.bonds {
                    bond.a -= usize::from(bond.a > removed);
                    bond.b -= usize::from(bond.b > removed);
                }
            }
            2 => {
                let a = rng.index(n);
                let b = (a + 1 + rng.index(n - 1)) % n;
                if let Some(i) = child
                    .bonds
                    .iter()
                    .position(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
                {
                    child.bonds.remove(i);
                } else {
                    child.bonds.push(Bond {
                        a,
                        b,
                        stiffness: rng.log_range(3.0e5, 8.0e6),
                    });
                }
            }
            _ => {}
        }
        let i = rng.index(child.nodes.len());
        let node = &mut child.nodes[i];
        node.mass = (node.mass * rng.range(-0.3, 0.3).exp()).clamp(0.5, 2.0);
        node.anchor = (node.anchor * rng.range(-0.45, 0.45).exp()).clamp(1.0e6, 2.0e7);
        node.threshold = (node.threshold * rng.range(-0.2, 0.2).exp()).clamp(0.15, 0.7);
        if !child.bonds.is_empty() {
            let i = rng.index(child.bonds.len());
            child.bonds[i].stiffness =
                (child.bonds[i].stiffness * rng.range(-0.45, 0.45).exp()).clamp(3.0e5, 8.0e6);
        }
        child.loss = (child.loss * rng.range(-0.12, 0.12).exp()).clamp(0.6, 2.2);
        child
    }
}

#[derive(Clone)]
struct Structure {
    omega: [f64; MAX_NODES],
    shapes: Matrix,
    pickup: [[f64; 2]; MAX_NODES],
    beta: [f64; MAX_NODES],
    roughness: f64,
    diversity: f64,
    score: f64,
}

fn diagonalize(mut matrix: Matrix, n: usize) -> ([f64; MAX_NODES], Matrix) {
    let mut vectors = [[0.0; MAX_NODES]; MAX_NODES];
    for (i, row) in vectors.iter_mut().enumerate().take(n) {
        row[i] = 1.0;
    }
    for _ in 0..80 * n * n {
        let (mut p, mut q, mut off) = (0, 0, 0.0);
        let scale = (0..n).map(|i| matrix[i][i].abs()).fold(1.0, f64::max);
        for (i, row) in matrix.iter().enumerate().take(n) {
            for (j, value) in row.iter().enumerate().take(n).skip(i + 1) {
                if value.abs() > off {
                    (p, q, off) = (i, j, value.abs());
                }
            }
        }
        if off < scale * 1e-12 {
            return (std::array::from_fn(|i| matrix[i][i]), vectors);
        }
        let angle = 0.5 * (2.0 * matrix[p][q]).atan2(matrix[q][q] - matrix[p][p]);
        let (s, c) = angle.sin_cos();
        let (pp, qq, pq) = (matrix[p][p], matrix[q][q], matrix[p][q]);
        matrix[p][p] = c * c * pp - 2.0 * s * c * pq + s * s * qq;
        matrix[q][q] = s * s * pp + 2.0 * s * c * pq + c * c * qq;
        matrix[p][q] = 0.0;
        matrix[q][p] = 0.0;
        for i in 0..n {
            if i != p && i != q {
                let (ip, iq) = (matrix[i][p], matrix[i][q]);
                matrix[i][p] = c * ip - s * iq;
                matrix[p][i] = matrix[i][p];
                matrix[i][q] = s * ip + c * iq;
                matrix[q][i] = matrix[i][q];
            }
            let (vp, vq) = (vectors[i][p], vectors[i][q]);
            vectors[i][p] = c * vp - s * vq;
            vectors[i][q] = s * vp + c * vq;
        }
    }
    unreachable!("bounded positive spring network did not converge")
}

impl Structure {
    fn solve(genome: &Genome) -> Self {
        let n = genome.nodes.len();
        let mut matrix = [[0.0; MAX_NODES]; MAX_NODES];
        for (i, node) in genome.nodes.iter().enumerate() {
            matrix[i][i] = node.anchor / node.mass;
        }
        for bond in &genome.bonds {
            let (a, b) = (bond.a, bond.b);
            let (ma, mb) = (genome.nodes[a].mass, genome.nodes[b].mass);
            matrix[a][a] += bond.stiffness / ma;
            matrix[b][b] += bond.stiffness / mb;
            matrix[a][b] -= bond.stiffness / (ma * mb).sqrt();
            matrix[b][a] = matrix[a][b];
        }
        let (eigenvalues, shapes) = diagonalize(matrix, n);
        let mut omega = [0.0; MAX_NODES];
        let mut beta = [0.0; MAX_NODES];
        let mut pickup = [[0.0; 2]; MAX_NODES];
        let mut weights = [0.0; MAX_NODES];
        // Each physical pickup has norm <= 1 in mass-normalized coordinates.
        // Orthogonal modal rotation preserves that norm, giving a peak bound.
        let norm = genome
            .nodes
            .iter()
            .map(|node| 1.0 / node.mass)
            .sum::<f64>()
            .sqrt();
        for mode in 0..n {
            omega[mode] = eigenvalues[mode].sqrt();
            // Rayleigh loss: progressively shorter high-frequency ringing.
            beta[mode] = genome.loss + 8.0e-8 * eigenvalues[mode];
            for (i, node) in genome.nodes.iter().enumerate() {
                let angle = node.pan * PI * 0.5;
                let projection = shapes[i][mode] / (node.mass.sqrt() * norm);
                pickup[mode][0] += projection * angle.cos();
                pickup[mode][1] += projection * angle.sin();
            }
            weights[mode] =
                (pickup[mode][0].powi(2) + pickup[mode][1].powi(2)).sqrt() / beta[mode].sqrt();
        }
        let total = weights[..n].iter().sum::<f64>().max(1e-12);
        for weight in &mut weights[..n] {
            *weight /= total;
        }
        // Nearly identical modes count as one audible resonance. Otherwise a
        // search could score four copies of the same pitch as a rich spectrum.
        let mut overlap = 0.0;
        for i in 0..n {
            for j in 0..n {
                let distance = (omega[i] / omega[j]).ln() / 0.04;
                overlap += weights[i] * weights[j] * (-distance * distance).exp();
            }
        }
        let diversity = 1.0 / overlap;
        // Sethares-style pairwise sensory roughness; a proxy, not a listener.
        // https://sethares.engr.wisc.edu/papers/consance.html
        let mut roughness = 0.0;
        for i in 0..n {
            for j in i + 1..n {
                let (f1, f2) = (omega[i] / TAU, omega[j] / TAU);
                let separation = (f1 - f2).abs() * 0.24 / (0.021 * f1.min(f2) + 19.0);
                roughness += 2.0
                    * weights[i]
                    * weights[j]
                    * ((-3.5 * separation).exp() - (-5.75 * separation).exp());
            }
        }
        // Do not win by reducing the network to one dominant resonance.
        let score = roughness + 0.04 * (4.0 - diversity).max(0.0).powi(2);
        Self {
            omega,
            shapes,
            pickup,
            beta,
            roughness,
            diversity,
            score,
        }
    }
}

fn discover(parent: &Genome, seed: u64) -> Genome {
    let mut rng = Random(seed);
    let mut best = parent.clone();
    let mut score = Structure::solve(&best).score;
    for _ in 0..SEARCH_ROUNDS {
        let ancestor = best.clone();
        for _ in 0..CHILDREN {
            let child = ancestor.mutate(&mut rng);
            let candidate_score = Structure::solve(&child).score;
            if candidate_score < score {
                (best, score) = (child, candidate_score);
            }
        }
    }
    best
}

/// A compact, replayable history. A = keep the parent, B = keep the discovery.
/// The model version is part of the file format; future laws must use v2.
#[derive(Clone, Debug, PartialEq)]
pub struct Discovery {
    pub seed: u32,
    choices: Vec<u8>,
}

impl Discovery {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            choices: Vec::new(),
        }
    }

    pub fn generation(&self) -> usize {
        self.choices.len()
    }

    pub fn keep(&mut self, condition: usize) -> Result<()> {
        if self.choices.len() >= 256 {
            return Err(
                "this lineage has reached 256 generations; save it and start a new seed".into(),
            );
        }
        self.choices.push(u8::from(condition != 0));
        Ok(())
    }

    fn genomes(&self) -> [Genome; 2] {
        let mut rng = Random(u64::from(self.seed));
        let mut parent = Genome::random(&mut rng);
        for &choice in &self.choices {
            let seed = rng.bits();
            if choice == 1 {
                parent = discover(&parent, seed);
            }
        }
        let child = discover(&parent, rng.bits());
        [parent, child]
    }

    pub fn encode(&self) -> String {
        let choices: String = self
            .choices
            .iter()
            .map(|v| if *v == 0 { 'A' } else { 'B' })
            .collect();
        format!("ripple-alien-v1\nseed {}\nchoices {}\n", self.seed, choices)
    }

    pub fn load(path: &Path) -> Result<Self> {
        if std::fs::metadata(path)?.len() > 512 {
            return Err("alien discovery file is too large".into());
        }
        let data = std::fs::read_to_string(path)?;
        Self::decode(&data)
    }

    fn decode(data: &str) -> Result<Self> {
        let mut lines = data.lines();
        if lines.next() != Some("ripple-alien-v1") {
            return Err("unsupported alien discovery version".into());
        }
        let seed = lines
            .next()
            .and_then(|s| s.strip_prefix("seed "))
            .ok_or("missing discovery seed")?
            .parse::<u32>()?;
        let text = lines
            .next()
            .and_then(|s| s.strip_prefix("choices "))
            .ok_or("missing discovery lineage")?;
        if text.len() > 256 || lines.next().is_some() {
            return Err("discovery lineage is too long or contains extra data".into());
        }
        let choices = text
            .bytes()
            .map(|c| match c {
                b'A' => Ok(0),
                b'B' => Ok(1),
                _ => Err("lineage choices must be A or B"),
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Self { seed, choices })
    }

    pub fn save(&self, directory: &Path) -> Result<PathBuf> {
        use std::io::Write;
        std::fs::create_dir_all(directory)?;
        let mut hash = u64::from(self.seed);
        for choice in &self.choices {
            hash = hash
                .wrapping_mul(0x100000001b3)
                .wrapping_add(u64::from(*choice) + 1);
        }
        let path = directory.join(format!(
            "alien-{}-g{}-{hash:016x}.alien",
            self.seed,
            self.generation()
        ));
        let data = self.encode();
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => file.write_all(data.as_bytes())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if std::fs::read_to_string(&path)? != data {
                    return Err("a different discovery already exists at the save path".into());
                }
            }
            Err(e) => return Err(e.into()),
        }
        Ok(path)
    }
}

#[derive(Clone, Copy, Default)]
struct Mode {
    z: f64,
    v: f64,
    zz: f64,
    zv: f64,
    vz: f64,
    vv: f64,
}

#[derive(Clone, Copy, Default)]
struct Driver {
    charge: f64,
    elapsed: f64,
    duration: f64,
}

struct Landscape {
    genome: Genome,
    structure: Structure,
    modes: [Mode; MAX_NODES],
    drivers: [Driver; MAX_NODES],
    rng: Random,
    weather_rng: Random,
    dt: f64,
    weather: f64,
    weather_target: f64,
    weather_clock: f64,
    start_cooldown: f64,
    energy_budget: f64,
    released: u64,
    active_drivers: usize,
    gain: f64,
    events: crate::events::EventLog,
    candidate: u8,
}

impl Landscape {
    fn new(genome: Genome, sr: f32, seed: u32) -> Self {
        assert!(sr.is_finite() && sr >= 8000.0);
        let structure = Structure::solve(&genome);
        let dt = 1.0 / f64::from(sr);
        let mut modes = [Mode::default(); MAX_NODES];
        for (i, mode) in modes.iter_mut().enumerate().take(genome.nodes.len()) {
            let (omega, beta) = (structure.omega[i], structure.beta[i]);
            let wd = (omega * omega - beta * beta).sqrt();
            let (s, c) = (wd * dt).sin_cos();
            let decay = (-beta * dt).exp();
            *mode = Mode {
                z: 0.0,
                v: 0.0,
                zz: decay * (c + beta / wd * s),
                zv: decay * omega / wd * s,
                vz: -decay * omega / wd * s,
                vv: decay * (c - beta / wd * s),
            };
        }
        let mut rng = Random(u64::from(seed) ^ 0xa17e05eed);
        let mut drivers = [Driver::default(); MAX_NODES];
        for (i, driver) in drivers.iter_mut().enumerate().take(genome.nodes.len()) {
            driver.charge = rng.range(0.65, 1.0) * genome.nodes[i].threshold;
        }
        Self {
            genome,
            structure,
            modes,
            drivers,
            rng,
            weather_rng: Random(u64::from(seed) ^ 0x5a7e3),
            dt,
            weather: 0.5,
            weather_target: 0.5,
            weather_clock: 0.0,
            start_cooldown: 0.0,
            energy_budget: 0.05,
            released: 0,
            active_drivers: 0,
            gain: 1.0,
            events: crate::events::EventLog::new(sr),
            candidate: 0,
        }
    }

    fn energy(&self) -> f64 {
        self.modes[..self.genome.nodes.len()]
            .iter()
            .map(|m| 0.5 * (m.z * m.z + m.v * m.v))
            .sum()
    }

    fn next(&mut self) -> (f32, f32) {
        let n = self.genome.nodes.len();
        self.weather_clock -= self.dt;
        if self.weather_clock <= 0.0 {
            self.weather_target = self.weather_rng.range(0.25, 1.0);
            self.weather_clock = self.weather_rng.range(3.0, 9.0);
        }
        self.weather += (self.weather_target - self.weather) * self.dt / 3.0;
        self.start_cooldown -= self.dt;
        self.energy_budget = (self.energy_budget + 0.25 * self.dt).min(0.1);
        self.active_drivers = self.drivers[..n]
            .iter()
            .filter(|d| d.elapsed < d.duration)
            .count();
        for i in 0..n {
            let d = &mut self.drivers[i];
            d.charge = (d.charge + self.weather * self.dt / n as f64)
                .min(self.genome.nodes[i].threshold * 1.1);
            if d.elapsed >= d.duration
                && d.charge >= self.genome.nodes[i].threshold
                && self.active_drivers < 2
                && self.start_cooldown <= 0.0
            {
                d.charge -= self.genome.nodes[i].threshold;
                d.elapsed = 0.0;
                d.duration = self.genome.nodes[i].threshold * 1.5 + 0.25;
                self.start_cooldown = 0.35;
                self.active_drivers += 1;
                self.released += 1;
                self.events.emit(crate::events::Kind::Excitation {
                    candidate: self.candidate,
                    node: i,
                    started: true,
                    duration: d.duration,
                });
            }
            if d.elapsed >= d.duration {
                continue;
            }
            // Raised-cosine force amplitude: excitation starts and stops at rest.
            let envelope = (PI * d.elapsed / d.duration).sin().powi(2);
            d.elapsed += self.dt;
            if d.elapsed >= d.duration {
                self.events.emit(crate::events::Kind::Excitation {
                    candidate: self.candidate,
                    node: i,
                    started: false,
                    duration: d.duration,
                });
            }
            let mass = self.genome.nodes[i].mass;
            let mut impulse =
                (self.rng.unit() * 2.0 - 1.0) * envelope * (0.5 * self.dt * mass).sqrt();
            let velocity = (0..n)
                .map(|j| self.structure.shapes[i][j] * self.modes[j].v)
                .sum::<f64>()
                / mass.sqrt();
            let cost = impulse * velocity + impulse * impulse / (2.0 * mass);
            let available = self
                .energy_budget
                .min((MAX_ENERGY - self.energy()).max(0.0));
            if cost > available {
                let directed = velocity * impulse.signum();
                impulse = impulse.signum()
                    * mass
                    * ((directed * directed + 2.0 * available / mass).sqrt() - directed);
            }
            let cost = impulse * velocity + impulse * impulse / (2.0 * mass);
            self.energy_budget = (self.energy_budget - cost.max(0.0)).max(0.0);
            for j in 0..n {
                self.modes[j].v += self.structure.shapes[i][j] * impulse / mass.sqrt();
            }
        }
        let mut out = [0.0; 2];
        for (i, mode) in self.modes.iter_mut().enumerate().take(n) {
            (mode.z, mode.v) = (
                mode.zz * mode.z + mode.zv * mode.v,
                mode.vz * mode.z + mode.vv * mode.v,
            );
            for (ear, sample) in out.iter_mut().enumerate() {
                *sample += mode.v * self.structure.pickup[i][ear];
            }
        }
        if self.events.summary_due() {
            let energy = self.energy();
            self.events.emit(crate::events::Kind::Resonators {
                candidate: self.candidate,
                energy,
                weather: self.weather,
            });
        }
        self.events.advance();
        ((out[0] * self.gain) as f32, (out[1] * self.gain) as f32)
    }
}

pub struct LiveDemo {
    landscapes: [Landscape; 2],
    condition: usize,
    mix: f64,
    blend_step: f64,
}

impl LiveDemo {
    #[cfg(test)]
    pub fn new(sr: f32, seed: u32) -> Self {
        Self::from_discovery(sr, &Discovery::new(seed))
    }

    pub fn from_discovery(sr: f32, discovery: &Discovery) -> Self {
        let genomes = discovery.genomes();
        // Listen to a deterministic prefix off the audio thread, then restart.
        // Match RMS, subject to the analytical peak ceiling; no live AGC pumps
        // quiet passages up and no clipping conceals an unstable simulation.
        let rms = genomes.each_ref().map(|genome| {
            let mut scene = Landscape::new(genome.clone(), sr, discovery.seed);
            let samples = (f64::from(sr) * MATCH_SECONDS) as usize;
            let mut squares = 0.0;
            for _ in 0..samples {
                let (l, r) = scene.next();
                squares += (f64::from(l).powi(2) + f64::from(r).powi(2)) * 0.5;
            }
            (squares / samples as f64).sqrt().max(1e-12)
        });
        let max_gain = MAX_PEAK / (2.0 * MAX_ENERGY).sqrt();
        let target = 0.025_f64.min(rms[0].min(rms[1]) * max_gain);
        let mut landscapes = genomes.map(|g| Landscape::new(g, sr, discovery.seed));
        for (i, scene) in landscapes.iter_mut().enumerate() {
            scene.gain = target / rms[i];
            scene.candidate = i as u8;
        }
        Self {
            landscapes,
            condition: 1,
            mix: 1.0,
            blend_step: 1.0 / (0.1 * f64::from(sr)),
        }
    }

    pub fn next_pair(&mut self) -> [(f32, f32); 2] {
        self.landscapes.each_mut().map(Landscape::next)
    }

    pub fn observe(&mut self) {
        for scene in &mut self.landscapes {
            scene.events.enable();
        }
    }

    pub fn drain_events(&mut self, batch: &mut crate::events::Batch) {
        for scene in &mut self.landscapes {
            batch.lost += scene.events.take_lost();
        }
        while !batch.is_full() {
            let index = match (
                self.landscapes[0].events.peek(),
                self.landscapes[1].events.peek(),
            ) {
                (Some(a), Some(b)) => usize::from(b.sample < a.sample),
                (Some(_), None) => 0,
                (None, Some(_)) => 1,
                (None, None) => break,
            };
            batch.push(self.landscapes[index].events.pop().unwrap());
        }
    }

    pub fn select(&mut self, condition: usize) {
        self.condition = usize::from(condition != 0);
    }

    pub(crate) fn visualize(&self, frame: &mut crate::visual::Frame) {
        frame.clear();
        frame.ready = true;
        frame.candidate = Some(self.condition);
        frame.mix = self.mix;
        for (candidate, scene) in self.landscapes.iter().enumerate() {
            for (index, mode) in scene.modes[..scene.genome.nodes.len()].iter().enumerate() {
                frame.resonances.push(crate::visual::Resonance {
                    candidate,
                    hz: scene.structure.omega[index] / TAU,
                    energy: 0.5 * (mode.z * mode.z + mode.v * mode.v),
                });
            }
        }
    }

    pub fn next_sample(&mut self) -> (f32, f32) {
        let [a, b] = self.next_pair();
        self.mix += (self.condition as f64 - self.mix) * self.blend_step;
        let mix = self.mix as f32;
        (a.0 + (b.0 - a.0) * mix, a.1 + (b.1 - a.1) * mix)
    }

    pub fn metrics(&self) -> [(&'static str, [f64; 2], &'static str); 4] {
        [
            (
                "Moving masses",
                self.landscapes
                    .each_ref()
                    .map(|l| l.genome.nodes.len() as f64),
                "count",
            ),
            (
                "Elastic bonds",
                self.landscapes
                    .each_ref()
                    .map(|l| l.genome.bonds.len() as f64),
                "count",
            ),
            (
                "Roughness proxy",
                self.landscapes.each_ref().map(|l| l.structure.roughness),
                "",
            ),
            (
                "Effective resonances",
                self.landscapes.each_ref().map(|l| l.structure.diversity),
                "",
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masses_and_bonds_determine_modes() {
        let mut genome = Genome {
            nodes: vec![
                Node {
                    mass: 1.0,
                    anchor: 4.0e6,
                    pan: 0.5,
                    threshold: 0.3
                };
                2
            ],
            bonds: Vec::new(),
            loss: 1.0,
        };
        let independent = Structure::solve(&genome);
        assert!((independent.omega[0] - 2000.0).abs() < 1e-7);
        genome.bonds.push(Bond {
            a: 0,
            b: 1,
            stiffness: 2.0e6,
        });
        let coupled = Structure::solve(&genome);
        let mut frequencies = coupled.omega[..2].to_vec();
        frequencies.sort_by(f64::total_cmp);
        assert!((frequencies[0] - 2000.0).abs() < 1e-7);
        assert!((frequencies[1] - 8.0e6_f64.sqrt()).abs() < 1e-7);
    }

    #[test]
    fn discovery_changes_structure_and_improves_the_joint_proxy() {
        let mut structural_changes = 0;
        for seed in 0..12 {
            let [parent, child] = Discovery::new(seed).genomes();
            assert!(Structure::solve(&child).score < Structure::solve(&parent).score);
            assert!(Structure::solve(&child).diversity > 3.0);
            if parent.nodes.len() != child.nodes.len() || parent.bonds.len() != child.bonds.len() {
                structural_changes += 1;
            }
        }
        assert!(structural_changes >= 8);
    }

    #[test]
    fn lineage_round_trip_reproduces_the_chosen_parent_and_audio() {
        let mut discovery = Discovery::new(42);
        let [_, chosen] = discovery.genomes();
        discovery.keep(1).unwrap();
        assert_eq!(discovery.genomes()[0], chosen);
        let restored = Discovery::decode(&discovery.encode()).unwrap();
        assert_eq!(discovery, restored);
        let mut a = LiveDemo::from_discovery(8000.0, &discovery);
        let mut b = LiveDemo::from_discovery(8000.0, &restored);
        for _ in 0..16000 {
            assert_eq!(a.next_pair(), b.next_pair());
        }
        for text in [
            "",
            "ripple-alien-v2\nseed 42\nchoices \n",
            "ripple-alien-v1\nseed 42\nchoices X\n",
        ] {
            assert!(Discovery::decode(text).is_err());
        }
    }

    #[test]
    fn driven_networks_remain_audible_bounded_and_sparse() {
        for seed in [0, 1, 42, 12345, u32::MAX] {
            let mut demo = LiveDemo::new(8000.0, seed);
            let mut squares = [0.0; 2];
            let mut peak = [0.0_f32; 2];
            for _ in 0..8000 * 20 {
                for (i, (l, r)) in demo.next_pair().into_iter().enumerate() {
                    assert!(l.is_finite() && r.is_finite());
                    squares[i] += f64::from(l * l + r * r) * 0.5;
                    peak[i] = peak[i].max(l.abs()).max(r.abs());
                    assert!(demo.landscapes[i].energy() <= MAX_ENERGY + 1e-8);
                    assert!(demo.landscapes[i].active_drivers <= 2);
                }
            }
            for i in 0..2 {
                let rms = (squares[i] / 160000.0).sqrt();
                eprintln!(
                    "seed {seed} condition {i}: rms {rms:.6}, peak {:.5}",
                    peak[i]
                );
                assert!(rms > 0.0001);
                assert!(peak[i] < MAX_PEAK as f32);
                assert!(demo.landscapes[i].released <= 60);
            }
        }
    }

    #[test]
    fn matching_and_switching_preserve_the_evolving_pair() {
        let mut demo = LiveDemo::new(48_000.0, 12345);
        let mut reference = LiveDemo::new(48_000.0, 12345);
        demo.observe();
        let mut last_sample = 0;
        let mut starts = [0; 2];
        let mut squares = [0.0; 2];
        for frame in 0..(48_000.0 * MATCH_SECONDS) as usize {
            if frame % 48_000 == 0 {
                demo.select((frame / 48_000) % 2);
            }
            let heard = demo.next_sample();
            assert!(heard.0.abs() <= MAX_PEAK as f32 && heard.1.abs() <= MAX_PEAK as f32);
            for (i, (l, r)) in reference.next_pair().into_iter().enumerate() {
                squares[i] += (f64::from(l).powi(2) + f64::from(r).powi(2)) * 0.5;
            }
            if frame % 256 == 255 || frame + 1 == (48_000.0 * MATCH_SECONDS) as usize {
                let mut batch = crate::events::Batch::new(0);
                demo.drain_events(&mut batch);
                assert_eq!(batch.lost, 0);
                for record in batch.records() {
                    assert!(record.sample >= last_sample && record.sample <= frame as u64);
                    last_sample = record.sample;
                    if let crate::events::Kind::Excitation {
                        candidate,
                        started: true,
                        ..
                    } = record.event
                    {
                        starts[candidate as usize] += 1;
                    }
                }
            }
        }
        assert!((squares[0] / squares[1] - 1.0).abs() < 1e-6);
        for i in 0..2 {
            assert_eq!(starts[i], demo.landscapes[i].released);
            assert_eq!(
                demo.landscapes[i].energy(),
                reference.landscapes[i].energy()
            );
            assert_eq!(
                demo.landscapes[i].released,
                reference.landscapes[i].released
            );
        }
        assert_eq!(demo.landscapes[0].weather, demo.landscapes[1].weather);
    }

    #[test]
    fn saving_is_repeatable_and_never_replaces_a_different_discovery() {
        let directory =
            std::env::temp_dir().join(format!("ripple-alien-save-{}", std::process::id()));
        let mut discovery = Discovery::new(231);
        discovery.keep(0).unwrap();
        let path = discovery.save(&directory).unwrap();
        assert_eq!(Discovery::load(&path).unwrap(), discovery);
        assert_eq!(discovery.save(&directory).unwrap(), path);
        std::fs::write(&path, "a different file").unwrap();
        assert!(discovery.save(&directory).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a different file");
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
