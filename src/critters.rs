//! The living voice — small agents that drive an oscillator to sing.
//!
//! A critter is a nonlinear oscillator (a voice) plus a behavioural clock. On
//! its own it just sings now and then. The interesting part is the *chorus*:
//! when audible calls reach another critter, they nudge its clock. Distance,
//! travel time, barriers and background noise determine who can hear whom.
//! Whether that produces synchrony is an observable result, not a guarantee.

use crate::acoustics::{AcousticPath, HearingScene, SignalDelay};
use crate::dsp::{pan, Noise};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::f32::consts::{PI, TAU};

/// A species: the fixed parameters that make a cricket a cricket. Timing is in
/// seconds, pitch in Hz, ratios relative to the carrier.
#[derive(Clone)]
pub struct Species {
    pub carrier_lo: f32,
    pub carrier_hi: f32,
    pub phrase_gap_lo: f32,
    pub phrase_gap_hi: f32,
    pub syllables: (usize, usize),
    pub syl_dur_lo: f32,
    pub syl_dur_hi: f32,
    pub syl_gap_lo: f32,
    pub syl_gap_hi: f32,
    pub sweep_lo: f32,
    pub sweep_hi: f32,
    pub pulse_rate: f32, // 0 = pure tone; >0 = amplitude-pulsed (tick-tick)
    pub harmonics: Vec<(f32, f32)>,
    pub vib_rate: f32,
    pub vib_depth: f32,
    pub amp_lo: f32,
    pub amp_hi: f32,
    pub reverb_send: f32,
}

impl Species {
    pub fn cricket() -> Self {
        Self {
            carrier_lo: 3900.0,
            carrier_hi: 4800.0,
            // Fairly uniform natural tempo, so the coupling can win and the
            // chorus falls into rhythm.
            phrase_gap_lo: 0.7,
            phrase_gap_hi: 1.0,
            syllables: (1, 1),
            syl_dur_lo: 0.22,
            syl_dur_hi: 0.34,
            syl_gap_lo: 0.15,
            syl_gap_hi: 0.4,
            sweep_lo: 1.0,
            sweep_hi: 1.0,
            pulse_rate: 26.0,
            harmonics: vec![(2.0, 0.15)],
            vib_rate: 0.0,
            vib_depth: 0.0,
            amp_lo: 0.05,
            amp_hi: 0.10,
            reverb_send: 0.05,
        }
    }
    pub fn frog() -> Self {
        Self {
            carrier_lo: 220.0,
            carrier_hi: 430.0,
            phrase_gap_lo: 2.5,
            phrase_gap_hi: 8.0,
            syllables: (1, 2),
            syl_dur_lo: 0.18,
            syl_dur_hi: 0.45,
            syl_gap_lo: 0.12,
            syl_gap_hi: 0.3,
            sweep_lo: 0.86,
            sweep_hi: 0.92,
            pulse_rate: 30.0,
            harmonics: vec![(2.0, 0.3)],
            vib_rate: 0.0,
            vib_depth: 0.0,
            amp_lo: 0.16,
            amp_hi: 0.28,
            reverb_send: 0.12,
        }
    }
    pub fn songbird() -> Self {
        Self {
            carrier_lo: 1800.0,
            carrier_hi: 4200.0,
            phrase_gap_lo: 1.5,
            phrase_gap_hi: 7.0,
            syllables: (2, 6),
            syl_dur_lo: 0.04,
            syl_dur_hi: 0.14,
            syl_gap_lo: 0.05,
            syl_gap_hi: 0.16,
            sweep_lo: 0.75,
            sweep_hi: 1.4,
            pulse_rate: 0.0,
            harmonics: vec![],
            vib_rate: 30.0,
            vib_depth: 0.06,
            amp_lo: 0.10,
            amp_hi: 0.20,
            reverb_send: 0.18,
        }
    }
    pub fn gull() -> Self {
        Self {
            carrier_lo: 1100.0,
            carrier_hi: 2000.0,
            phrase_gap_lo: 4.0,
            phrase_gap_hi: 14.0,
            syllables: (1, 3),
            syl_dur_lo: 0.35,
            syl_dur_hi: 0.7,
            syl_gap_lo: 0.3,
            syl_gap_hi: 0.9,
            sweep_lo: 0.62,
            sweep_hi: 0.78,
            pulse_rate: 0.0,
            harmonics: vec![(2.0, 0.35), (3.0, 0.15)],
            vib_rate: 6.0,
            vib_depth: 0.03,
            amp_lo: 0.12,
            amp_hi: 0.22,
            reverb_send: 0.22,
        }
    }
    pub fn owl() -> Self {
        Self {
            carrier_lo: 320.0,
            carrier_hi: 430.0,
            phrase_gap_lo: 14.0,
            phrase_gap_hi: 40.0,
            syllables: (2, 4),
            syl_dur_lo: 0.25,
            syl_dur_hi: 0.5,
            syl_gap_lo: 0.35,
            syl_gap_hi: 0.8,
            sweep_lo: 0.95,
            sweep_hi: 0.95,
            pulse_rate: 0.0,
            harmonics: vec![(2.0, 0.18)],
            vib_rate: 0.0,
            vib_depth: 0.0,
            amp_lo: 0.20,
            amp_hi: 0.28,
            reverb_send: 0.3,
        }
    }
}

#[derive(Clone, Copy)]
struct Syllable {
    delay: u32,
    freq: f32,
    dur: f32,
    sweep: f32,
}

const NO_SYLLABLE: Syllable = Syllable {
    delay: 0,
    freq: 0.0,
    dur: 0.0,
    sweep: 1.0,
};

/// One agent. Holds its fixed voice, a behavioural clock (`phrase_phase`),
/// and whatever syllable it is currently singing.
pub struct Critter {
    sr: f32,
    rng: Noise,
    sp: Species,
    // Behavioural clock: rises 0->1, then the critter sings and it resets.
    phrase_phase: f32,
    phrase_inc: f32,
    // Currently sounding syllable.
    active: bool,
    t: f32,
    dur: f32,
    phase: f32,
    freq: f32,
    sweep: f32,
    amp: f32,
    pulse_phase: f32,
    pulse_inc: f32,
    // Syllables scheduled later in this phrase.
    pending: [Syllable; 8],
    pending_len: usize,
    // An onset is emitted only once this syllable produces a nonzero sample.
    onset_pending: bool,
    emitted_level: Option<f32>,
}

impl Critter {
    pub fn new(sp: Species, sr: f32, seed: u32) -> Self {
        let mut rng = Noise::new(seed);
        let period = rng.range(sp.phrase_gap_lo, sp.phrase_gap_hi);
        Self {
            sr,
            phrase_phase: rng.unit(), // desync the start
            phrase_inc: 1.0 / (period * sr),
            active: false,
            t: 0.0,
            dur: 0.0,
            phase: 0.0,
            freq: 0.0,
            sweep: 1.0,
            amp: 0.0,
            pulse_phase: 0.0,
            pulse_inc: 0.0,
            pending: [NO_SYLLABLE; 8],
            pending_len: 0,
            onset_pending: false,
            emitted_level: None,
            rng,
            sp,
        }
    }

    /// The behavioural clock's position — read by the chorus for coupling.
    #[inline]
    pub fn phase(&self) -> f32 {
        self.phrase_phase
    }

    /// A neighbour nudges this critter's clock (pulse coupling). Positive pulls
    /// it toward firing (crickets synchronise); negative pushes it back (frogs
    /// take turns).
    #[inline]
    pub fn kick(&mut self, amount: f32) {
        if !self.active {
            self.phrase_phase = (self.phrase_phase + amount).clamp(0.0, 0.999);
        }
    }

    /// Begin a phrase: choose a voice for it and lay out its syllables.
    fn start_phrase(&mut self) {
        let base = self.rng.range(self.sp.carrier_lo, self.sp.carrier_hi);
        self.amp = self.rng.range(self.sp.amp_lo, self.sp.amp_hi);
        let n = if self.sp.syllables.1 > self.sp.syllables.0 {
            self.sp.syllables.0
                + (self.rng.unit() * (self.sp.syllables.1 - self.sp.syllables.0 + 1) as f32)
                    as usize
        } else {
            self.sp.syllables.0
        };
        let mut at = 0.0f32;
        self.pending_len = 0;
        for k in 0..n.max(1) {
            let syl = Syllable {
                delay: (at * self.sr) as u32,
                freq: base * self.rng.range(0.94, 1.06),
                dur: self.rng.range(self.sp.syl_dur_lo, self.sp.syl_dur_hi),
                sweep: self.rng.range(self.sp.sweep_lo, self.sp.sweep_hi),
            };
            if k == 0 {
                self.begin_syllable(syl);
            } else if self.pending_len < self.pending.len() {
                self.pending[self.pending_len] = syl;
                self.pending_len += 1;
            }
            at += syl.dur + self.rng.range(self.sp.syl_gap_lo, self.sp.syl_gap_hi);
        }
        // Choose how long until the next phrase.
        let period = self.rng.range(self.sp.phrase_gap_lo, self.sp.phrase_gap_hi);
        self.phrase_inc = 1.0 / (period * self.sr);
    }

    fn begin_syllable(&mut self, syl: Syllable) {
        self.active = true;
        self.t = 0.0;
        self.dur = syl.dur;
        self.phase = 0.0;
        self.freq = syl.freq;
        self.sweep = syl.sweep;
        self.pulse_phase = 0.0;
        self.pulse_inc = TAU * self.sp.pulse_rate / self.sr;
        self.onset_pending = true;
    }

    fn process_mono(&mut self) -> f32 {
        self.emitted_level = None;
        // Release any scheduled syllables whose delay has elapsed.
        let mut i = 0;
        while i < self.pending_len {
            if self.pending[i].delay == 0 {
                let syl = self.pending[i];
                self.pending[i] = self.pending[self.pending_len - 1];
                self.pending_len -= 1;
                self.begin_syllable(syl);
            } else {
                self.pending[i].delay -= 1;
                i += 1;
            }
        }

        // Advance the behavioural clock; fire a phrase when it wraps.
        self.phrase_phase += self.phrase_inc;
        if self.phrase_phase >= 1.0 {
            self.phrase_phase -= 1.0;
            if !self.active && self.pending_len == 0 {
                self.start_phrase();
            }
        }

        if !self.active {
            return 0.0;
        }

        let x = self.t / self.dur;
        let env = (PI * x).sin(); // gentle rise and fall, no edges
        let vib = if self.sp.vib_depth > 0.0 {
            1.0 + self.sp.vib_depth * (TAU * self.sp.vib_rate * self.t).sin()
        } else {
            1.0
        };
        let freq = self.freq * (1.0 + (self.sweep - 1.0) * x) * vib;

        // Carrier plus any harmonics.
        let mut s = self.phase.sin();
        for (ratio, gain) in &self.sp.harmonics {
            s += (self.phase * ratio).sin() * gain;
        }

        // Optional amplitude pulse train (the cricket/frog "tick-tick").
        let gate = if self.sp.pulse_rate > 0.0 {
            let g = self.pulse_phase.sin().max(0.0);
            self.pulse_phase += self.pulse_inc;
            g * g * g
        } else {
            1.0
        };

        let out = s * env * gate * self.amp;
        if self.onset_pending && out.abs() > 1e-9 {
            // The onset packet carries the call's nominal peak level. This is
            // an event-level audibility model, not an auditory nerve model.
            self.emitted_level = Some(self.amp);
            self.onset_pending = false;
        }
        self.phase += TAU * freq / self.sr;
        self.t += 1.0 / self.sr;
        if self.t >= self.dur {
            self.active = false;
        }
        out
    }
}

/// Counts are receiver events: one emitted call can be heard by many animals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HearingStats {
    pub emitted_calls: u64,
    pub arrived_calls: u64,
    pub heard_calls: u64,
    pub masked_calls: u64,
}

#[derive(Clone, Copy)]
struct Arrival {
    at: u64,
    source: usize,
    receiver: usize,
    level: f32,
    gain: f32,
}

// Reverse time ordering makes BinaryHeap a queue of the earliest arrivals.
impl Ord for Arrival {
    fn cmp(&self, other: &Self) -> Ordering {
        (other.at, other.source, other.receiver).cmp(&(self.at, self.source, self.receiver))
    }
}
impl PartialOrd for Arrival {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Arrival {
    fn eq(&self, other: &Self) -> bool {
        (self.at, self.source, self.receiver) == (other.at, other.source, other.receiver)
    }
}
impl Eq for Arrival {}

/// A population with fixed positions. Calls affect neighbours only after an
/// actual syllable has started and its propagation delay has elapsed. The
/// listener receives the same source waveform over the same kind of path.
pub struct Chorus {
    critters: Vec<Critter>,
    coupling: f32,
    reverb_send: f32,
    scene: HearingScene,
    paths: Vec<AcousticPath>,
    listener_paths: Vec<AcousticPath>,
    listener_delays: Vec<SignalDelay>,
    arrivals: BinaryHeap<Arrival>,
    sample: u64,
    stats: HearingStats,
}

impl Chorus {
    pub fn new(sp: Species, count: usize, coupling: f32, sr: f32, seed: u32) -> Self {
        Self::with_scene(sp, coupling, sr, seed, HearingScene::meadow(count))
            .expect("default hearing scene is valid")
    }

    pub fn with_scene(
        sp: Species,
        coupling: f32,
        sr: f32,
        seed: u32,
        scene: HearingScene,
    ) -> Result<Self, String> {
        scene.validate(sr)?;
        if !coupling.is_finite() {
            return Err("coupling must be finite".into());
        }
        let count = scene.positions.len();
        let reverb_send = sp.reverb_send;
        let critters = (0..count)
            .map(|i| {
                Critter::new(
                    sp.clone(),
                    sr,
                    seed.wrapping_add((i as u32).wrapping_mul(2_654_435_761)),
                )
            })
            .collect();
        let paths = scene
            .positions
            .iter()
            .flat_map(|&from| scene.positions.iter().map(move |&to| (from, to)))
            .map(|(from, to)| scene.path(from, to, sr))
            .collect();
        let listener_paths: Vec<_> = scene
            .positions
            .iter()
            .map(|&from| scene.path(from, scene.listener, sr))
            .collect();
        let listener_delays = listener_paths
            .iter()
            .map(|path| SignalDelay::new(path.delay_samples))
            .collect();
        Ok(Self {
            critters,
            coupling,
            reverb_send,
            scene,
            paths,
            listener_paths,
            listener_delays,
            arrivals: BinaryHeap::with_capacity(count * count * 2),
            sample: 0,
            stats: HearingStats::default(),
        })
    }

    pub fn reverb_send(&self) -> f32 {
        self.reverb_send
    }

    /// Linear background level at each animal's ears. The world supplies an
    /// estimate from its environmental sounds; audibility requires a 2:1
    /// call/background ratio as well as the absolute hearing threshold.
    pub fn set_masking_level(&mut self, level: f32) {
        self.scene.masking_level = if level.is_finite() {
            level.max(0.0)
        } else {
            0.0
        };
    }

    pub(crate) fn hearing_stats(&self) -> HearingStats {
        self.stats
    }

    /// Kuramoto order of behavioural clocks, not a claim of audible synchrony.
    pub fn order(&self) -> f32 {
        let (mut x, mut y) = (0.0f32, 0.0f32);
        for critter in &self.critters {
            let phase = TAU * critter.phase();
            x += phase.cos();
            y += phase.sin();
        }
        x.hypot(y) / self.critters.len().max(1) as f32
    }

    pub fn process(&mut self) -> (f32, f32) {
        self.process_observed(0, |_| {})
    }

    pub(crate) fn process_observed(
        &mut self,
        population: usize,
        mut emit: impl FnMut(crate::events::Kind),
    ) -> (f32, f32) {
        use crate::events::Kind;
        // Receive before advancing voices. The queue includes every source,
        // so simultaneous onsets are not silently reduced to one caller.
        while self.arrivals.peek().is_some_and(|a| a.at <= self.sample) {
            let arrival = self.arrivals.pop().unwrap();
            self.stats.arrived_calls += 1;
            let threshold = self
                .scene
                .hearing_threshold
                .max(2.0 * self.scene.masking_level);
            if arrival.level > threshold {
                self.stats.heard_calls += 1;
                let before = self.critters[arrival.receiver].phase();
                self.critters[arrival.receiver].kick(self.coupling * arrival.gain);
                emit(Kind::CallHeard {
                    population,
                    caller: arrival.source,
                    receiver: arrival.receiver,
                    level: arrival.level,
                    clock_shift: self.critters[arrival.receiver].phase() - before,
                });
            } else {
                self.stats.masked_calls += 1;
                emit(Kind::CallMasked {
                    population,
                    caller: arrival.source,
                    receiver: arrival.receiver,
                    level: arrival.level,
                    threshold,
                });
            }
        }

        let (mut left, mut right) = (0.0, 0.0);
        let count = self.critters.len();
        for source in 0..count {
            let was_active = self.critters[source].active;
            let out = self.critters[source].process_mono();
            if let Some(level) = self.critters[source].emitted_level {
                self.stats.emitted_calls += 1;
                emit(Kind::CallStarted {
                    population,
                    caller: source,
                    frequency_hz: self.critters[source].freq,
                    level,
                });
                for receiver in 0..count {
                    if receiver != source {
                        let path = self.paths[source * count + receiver];
                        self.arrivals.push(Arrival {
                            at: self.sample + path.delay_samples as u64,
                            source,
                            receiver,
                            level: level * path.gain,
                            gain: path.gain,
                        });
                    }
                }
            }
            if was_active && !self.critters[source].active {
                emit(Kind::CallEnded {
                    population,
                    caller: source,
                });
            }
            let path = self.listener_paths[source];
            let received = self.listener_delays[source].process(out) * path.gain;
            let (l, r) = pan(received, path.pan);
            left += l;
            right += r;
        }
        self.sample += 1;
        (left, right)
    }
}

#[cfg(test)]
mod hearing_tests {
    use super::*;
    use crate::acoustics::Point;

    fn pair(sr: f32, distance: f32, threshold: f32) -> Chorus {
        let mut scene = HearingScene::meadow(2);
        scene.positions = vec![
            Point { x: 0.0, y: 0.0 },
            Point {
                x: distance,
                y: 0.0,
            },
        ];
        scene.sound_speed = 10.0;
        scene.hearing_threshold = threshold;
        scene.masking_level = 0.0;
        let mut species = Species::cricket();
        species.carrier_lo = 73.0;
        species.carrier_hi = 73.0;
        species.pulse_rate = 0.0;
        let mut chorus = Chorus::with_scene(species, 0.1, sr, 91, scene).unwrap();
        chorus.critters[0].phrase_phase = 0.999;
        chorus.critters[0].phrase_inc = 0.01;
        chorus.critters[1].phrase_phase = 0.2;
        chorus.critters[1].phrase_inc = 0.0;
        chorus
    }

    #[test]
    fn an_actual_emission_must_precede_delayed_hearing() {
        let mut chorus = pair(1_000.0, 1.0, 0.001);
        let mut emission_at = None;
        let mut events = Vec::new();
        for sample in 0..150 {
            chorus.process_observed(7, |event| events.push((sample, event)));
            if chorus.hearing_stats().emitted_calls > 0 && emission_at.is_none() {
                emission_at = Some(sample);
            }
            if emission_at.is_none_or(|at| sample < at + 100) {
                assert_eq!(chorus.hearing_stats().heard_calls, 0);
                assert_eq!(chorus.critters[1].phase(), 0.2);
            } else {
                assert_eq!(sample, emission_at.unwrap() + 100);
                assert_eq!(chorus.hearing_stats().heard_calls, 1);
                assert!(chorus.critters[1].phase() > 0.2);
                let started = events
                    .iter()
                    .find(|(_, event)| {
                        matches!(
                            event,
                            crate::events::Kind::CallStarted {
                                population: 7,
                                caller: 0,
                                ..
                            }
                        )
                    })
                    .unwrap();
                let heard = events.iter().find(|(_, event)| matches!(event,
                    crate::events::Kind::CallHeard { population: 7, caller: 0, receiver: 1, clock_shift, .. } if *clock_shift > 0.0)).unwrap();
                assert_eq!(started.0, emission_at.unwrap());
                assert_eq!(heard.0, started.0 + 100);
                return;
            }
        }
        panic!("expected a delayed received call");
    }

    #[test]
    fn near_fire_clock_without_sound_cannot_send_a_call() {
        let mut chorus = pair(1_000.0, 1.0, 0.001);
        chorus.critters[0].phrase_phase = 0.95;
        chorus.critters[0].phrase_inc = 0.0;
        for _ in 0..500 {
            chorus.process();
        }
        assert_eq!(chorus.hearing_stats(), HearingStats::default());
        assert_eq!(chorus.critters[1].phase(), 0.2);
    }

    #[test]
    fn a_masked_call_arrives_but_cannot_nudge_the_receiver() {
        let mut chorus = pair(1_000.0, 1.0, 0.001);
        chorus.set_masking_level(1.0);
        for _ in 0..150 {
            chorus.process();
        }
        assert!(chorus.hearing_stats().arrived_calls > 0);
        assert_eq!(chorus.hearing_stats().heard_calls, 0);
        assert_eq!(chorus.critters[1].phase(), 0.2);
    }

    #[test]
    fn distance_can_make_a_call_inaudible() {
        let mut near = pair(1_000.0, 1.0, 0.025);
        let mut far = pair(1_000.0, 20.0, 0.025);
        for _ in 0..2_500 {
            near.process();
            far.process();
        }
        assert!(near.hearing_stats().heard_calls > 0);
        assert!(far.hearing_stats().arrived_calls > 0);
        assert_eq!(far.hearing_stats().heard_calls, 0);
        assert_eq!(far.critters[1].phase(), 0.2);
    }

    #[test]
    fn simultaneous_callers_both_reach_the_queue() {
        let mut chorus = pair(1_000.0, 1.0, 0.001);
        chorus.critters[1].phrase_phase = 0.999;
        chorus.critters[1].phrase_inc = 0.01;
        for _ in 0..150 {
            chorus.process();
        }
        assert_eq!(chorus.hearing_stats().emitted_calls, 2);
        assert_eq!(chorus.hearing_stats().heard_calls, 2);
    }

    #[test]
    fn barrier_changes_local_hearing_and_remains_deterministic() {
        let mut open = Chorus::with_scene(
            Species::cricket(),
            0.12,
            48_000.0,
            12345,
            HearingScene::two_groups(false),
        )
        .unwrap();
        let mut barrier = Chorus::with_scene(
            Species::cricket(),
            0.12,
            48_000.0,
            12345,
            HearingScene::two_groups(true),
        )
        .unwrap();
        let mut repeat = Chorus::with_scene(
            Species::cricket(),
            0.12,
            48_000.0,
            12345,
            HearingScene::two_groups(true),
        )
        .unwrap();
        let open_capacity = open.arrivals.capacity();
        let barrier_capacity = barrier.arrivals.capacity();
        let mut events = Vec::new();
        for _ in 0..48_000 * 3 {
            let a = open.process();
            let b = barrier.process_observed(0, |event| events.push(event));
            assert_eq!(b, repeat.process());
            for sample in [a.0, a.1, b.0, b.1] {
                assert!(sample.is_finite() && sample.abs() < 1.0);
            }
        }
        assert!(open.hearing_stats().heard_calls > 0);
        assert_eq!(open.hearing_stats().masked_calls, 0);
        assert!(barrier.hearing_stats().heard_calls > 0);
        assert!(barrier.hearing_stats().masked_calls > 0);
        assert_eq!(barrier.hearing_stats(), repeat.hearing_stats());
        let stats = barrier.hearing_stats();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, crate::events::Kind::CallStarted { .. }))
                .count() as u64,
            stats.emitted_calls
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, crate::events::Kind::CallHeard { .. }))
                .count() as u64,
            stats.heard_calls
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, crate::events::Kind::CallMasked { .. }))
                .count() as u64,
            stats.masked_calls
        );
        assert_eq!(open.arrivals.capacity(), open_capacity);
        assert_eq!(barrier.arrivals.capacity(), barrier_capacity);
    }
}
