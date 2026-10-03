//! Observations only: event capture never drives physics or consumes randomness.
//! Fixed-capacity records cross the audio boundary; formatting and I/O do not.
use serde::Serialize;

const CAPACITY: usize = 256;
pub const BATCH_SIZE: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Kind {
    Started {
        seed: u32,
    },
    RainImpact {
        x: f32,
        y: f32,
        surface: &'static str,
        strength: f32,
    },
    Bubble {
        cause: &'static str,
        radius_m: f32,
        strength: f32,
        pan: f32,
        voiced: bool,
    },
    WaveBreak {
        pan: f32,
        strength: f32,
    },
    Contact {
        body: usize,
        position: f32,
        velocity_kick: f32,
    },
    FirePop {
        position: f32,
        strength: f32,
    },
    CallStarted {
        population: usize,
        caller: usize,
        frequency_hz: f32,
        level: f32,
    },
    CallEnded {
        population: usize,
        caller: usize,
    },
    CallHeard {
        population: usize,
        caller: usize,
        receiver: usize,
        level: f32,
        clock_shift: f32,
    },
    CallMasked {
        population: usize,
        caller: usize,
        receiver: usize,
        level: f32,
        threshold: f32,
    },
    Weather {
        air: f32,
        gust: f32,
        rain: f32,
        daylight: f32,
    },
    Water {
        flow: f32,
        speed: f32,
        surface_m3: f64,
        retained_m3: f64,
        suspended_m3: f64,
        exported_m3: f64,
    },
    Excitation {
        candidate: u8,
        node: usize,
        started: bool,
        duration: f64,
    },
    Resonators {
        candidate: u8,
        energy: f64,
        weather: f64,
    },
}

impl Kind {
    pub fn category(&self) -> usize {
        match self {
            Self::CallStarted { .. }
            | Self::CallEnded { .. }
            | Self::CallHeard { .. }
            | Self::CallMasked { .. } => 1,
            Self::Contact { .. } | Self::FirePop { .. } => 2,
            Self::RainImpact { .. }
            | Self::Bubble { .. }
            | Self::WaveBreak { .. }
            | Self::Water { .. } => 3,
            Self::Weather { .. } => 4,
            Self::Excitation { .. } | Self::Resonators { .. } => 5,
            Self::Started { .. } => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Record {
    pub sequence: u64,
    pub sample: u64,
    pub time_seconds: f64,
    #[serde(flatten)]
    pub event: Kind,
}

const EMPTY: Record = Record {
    sequence: 0,
    sample: 0,
    time_seconds: 0.0,
    event: Kind::Started { seed: 0 },
};

pub struct Batch {
    pub generation: u64,
    records: [Record; BATCH_SIZE],
    len: usize,
    pub lost: u64,
}

impl Batch {
    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            records: [EMPTY; BATCH_SIZE],
            len: 0,
            lost: 0,
        }
    }
    pub fn records(&self) -> &[Record] {
        &self.records[..self.len]
    }
    pub fn is_full(&self) -> bool {
        self.len == BATCH_SIZE
    }
    pub fn push(&mut self, record: Record) {
        assert!(!self.is_full());
        self.records[self.len] = record;
        self.len += 1;
    }
}

pub struct EventLog {
    slots: Box<[Record]>,
    head: usize,
    len: usize,
    lost: u64,
    enabled: bool,
    frame: u64,
    sr: f64,
    sequence: u64,
    next_summary: u64,
}

impl EventLog {
    pub fn new(sr: f32) -> Self {
        Self {
            slots: vec![EMPTY; CAPACITY].into_boxed_slice(),
            head: 0,
            len: 0,
            lost: 0,
            enabled: false,
            frame: 0,
            sr: f64::from(sr),
            sequence: 0,
            next_summary: 0,
        }
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    pub fn emit(&mut self, event: Kind) {
        if !self.enabled {
            return;
        }
        if self.len == CAPACITY {
            self.head = (self.head + 1) % CAPACITY;
            self.len -= 1;
            self.lost += 1;
        }
        let i = (self.head + self.len) % CAPACITY;
        self.slots[i] = Record {
            sequence: self.sequence,
            sample: self.frame,
            time_seconds: self.frame as f64 / self.sr,
            event,
        };
        self.sequence += 1;
        self.len += 1;
    }
    pub fn advance(&mut self) {
        self.frame += 1;
    }
    pub fn summary_due(&mut self) -> bool {
        if self.enabled && self.frame >= self.next_summary {
            self.next_summary = self.frame + self.sr as u64;
            true
        } else {
            false
        }
    }
    pub fn peek(&self) -> Option<Record> {
        (self.len > 0).then(|| self.slots[self.head])
    }
    pub fn pop(&mut self) -> Option<Record> {
        let result = self.peek()?;
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
        Some(result)
    }
    pub fn take_lost(&mut self) -> u64 {
        std::mem::take(&mut self.lost)
    }
    pub fn drain(&mut self, batch: &mut Batch) {
        batch.lost += self.take_lost();
        while !batch.is_full() {
            let Some(record) = self.pop() else {
                break;
            };
            batch.push(record);
        }
    }
}

/// Offline JSONL trace. Drain every control block and report any capture loss.
pub fn trace(
    mut world: crate::world::World,
    sr: u32,
    seconds: f32,
    output: impl std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut output = std::io::BufWriter::new(output);
    world.observe();
    for i in 0..(f64::from(seconds) * f64::from(sr)) as u64 {
        world.next_sample();
        if i % 32 == 0 {
            write_pending(&mut world, &mut output)?;
        }
    }
    write_pending(&mut world, &mut output)?;
    output.flush()?;
    Ok(())
}

fn write_pending(
    world: &mut crate::world::World,
    output: &mut impl std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let mut batch = Batch::new(0);
        world.drain_events(&mut batch);
        if batch.lost > 0 {
            serde_json::to_writer(
                &mut *output,
                &serde_json::json!({"kind":"events_lost", "count":batch.lost}),
            )?;
            output.write_all(b"\n")?;
        }
        for record in batch.records() {
            serde_json::to_writer(&mut *output, record)?;
            output.write_all(b"\n")?;
        }
        if !batch.is_full() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "long sample-exact observation and event/physics integration audit"]
    fn observing_named_and_generated_worlds_preserves_audio_and_actual_events() {
        let sr = 16_000.0;
        let mut totals = [0; 4];
        for name in ["glade", "brook", "storm", "night-meadow", "generated"] {
            let build = || {
                if name == "generated" {
                    crate::alien::worlds::WorldSpec::random(12345)
                        .build(sr)
                        .unwrap()
                } else {
                    crate::presets::build(name, sr, 12345).unwrap()
                }
            };
            let mut heard = build();
            let mut reference = build();
            heard.observe();
            heard.observe(); // Enabling twice must not create a second timeline.
            let mut counts = [0; 4];
            let mut sequence = 0;
            let mut last_sample = 0;
            for frame in 0..sr as u64 * 30 {
                assert_eq!(
                    heard.next_sample(),
                    reference.next_sample(),
                    "{name} sample {frame}"
                );
                if frame % 128 == 127 {
                    loop {
                        let mut batch = Batch::new(0);
                        heard.drain_events(&mut batch);
                        assert_eq!(batch.lost, 0, "{name}");
                        for record in batch.records() {
                            assert_eq!(record.sequence, sequence);
                            assert!(record.sample >= last_sample && record.sample <= frame);
                            assert_eq!(record.time_seconds, record.sample as f64 / sr as f64);
                            sequence += 1;
                            last_sample = record.sample;
                            match record.event {
                                Kind::Started { seed } => {
                                    assert_eq!(sequence, 1);
                                    assert_eq!(seed, 12345);
                                }
                                Kind::Contact { .. } => counts[0] += 1,
                                Kind::Bubble {
                                    cause,
                                    voiced: true,
                                    ..
                                } if cause != "rain" => counts[1] += 1,
                                Kind::CallStarted { .. } => counts[2] += 1,
                                Kind::CallHeard { .. } => counts[3] += 1,
                                _ => {}
                            }
                        }
                        if !batch.is_full() {
                            break;
                        }
                    }
                }
            }
            let physics = heard.physics_report();
            assert_eq!(
                counts,
                [
                    physics.contacts,
                    physics.cloud_events,
                    physics.emitted_calls,
                    physics.heard_calls
                ],
                "{name}"
            );
            assert!(sequence > 60, "{name}");
            for (total, count) in totals.iter_mut().zip(counts) {
                *total += count;
            }
            println!(
                "{name}: {} events; contacts/clouds/calls/heard {counts:?}; audio identical",
                sequence
            );
        }
        assert!(totals.iter().all(|count| *count > 0), "{totals:?}");
    }

    #[test]
    fn bounded_capture_reports_loss_and_preserves_remaining_time_order() {
        let mut log = EventLog::new(100.0);
        log.emit(Kind::Started { seed: 1 });
        assert!(log.peek().is_none());
        log.enable();
        for i in 0..300 {
            log.emit(Kind::Started { seed: i });
            log.advance();
        }
        let mut batch = Batch::new(3);
        log.drain(&mut batch);
        assert_eq!(batch.lost, 44);
        assert_eq!(batch.records()[0].sample, 44);
        assert_eq!(batch.records()[0].time_seconds, 0.44);
        let mut count = batch.records().len();
        let mut last = batch.records().last().unwrap().sample;
        while log.peek().is_some() {
            let mut batch = Batch::new(3);
            log.drain(&mut batch);
            assert_eq!(batch.lost, 0);
            for e in batch.records() {
                assert!(e.sample > last);
                last = e.sample;
                count += 1;
            }
        }
        assert_eq!(count, CAPACITY);
        assert_eq!(last, 299);
    }
}
