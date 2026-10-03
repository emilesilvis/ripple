//! Aggregate observations on the UI thread, independently of log retention.
//! 100 ms bins bound storage; windows advance in simulation time, not wall time.
use crate::events::{Kind, Record};
use crossterm::style::Color;
use std::collections::{BTreeSet, VecDeque};

const TICKS_PER_SECOND: u64 = 10;
const WINDOW_TICKS: u64 = 100;
const HISTORY_SECONDS: u64 = 20;

#[derive(Default)]
struct Bucket {
    tick: u64,
    events: u64,
    calls: u64,
    heard: u64,
    masked: u64,
    nudges: u64,
    contacts: u64,
    bubbles: u64,
    drops: u64,
    waves: u64,
    embers: u64,
    callers: BTreeSet<(usize, usize)>,
    links: BTreeSet<(usize, usize, usize)>,
    starts: [u64; 2],
    candidate_events: [u64; 2],
    nodes: [BTreeSet<usize>; 2],
}

#[derive(Default)]
pub(super) struct Activity {
    buckets: VecDeque<Bucket>,
    seen: bool,
    latest: f64,
    water: Option<Kind>,
    weather: Option<Kind>,
    resonators: [Option<(f64, f64)>; 2],
}

impl Activity {
    pub fn record(&mut self, record: &Record) {
        self.seen = true;
        self.latest = self.latest.max(record.time_seconds);
        let tick = (record.time_seconds * TICKS_PER_SECOND as f64) as u64;
        while self
            .buckets
            .front()
            .is_some_and(|b| b.tick + HISTORY_SECONDS * TICKS_PER_SECOND < tick)
        {
            self.buckets.pop_front();
        }
        match record.event {
            Kind::Started { .. } => return,
            event @ Kind::Water { .. } => {
                self.water = Some(event);
                return;
            }
            event @ Kind::Weather { .. } => {
                self.weather = Some(event);
                return;
            }
            Kind::Resonators {
                candidate,
                energy,
                weather,
            } => {
                self.resonators[candidate as usize] = Some((energy, weather));
                return;
            }
            _ => {}
        }
        if self.buckets.back().is_none_or(|b| b.tick != tick) {
            self.buckets.push_back(Bucket {
                tick,
                ..Bucket::default()
            });
        }
        let b = self.buckets.back_mut().unwrap();
        b.events += 1;
        match record.event {
            Kind::CallStarted {
                population, caller, ..
            } => {
                b.calls += 1;
                b.callers.insert((population, caller));
            }
            Kind::CallHeard {
                population,
                caller,
                receiver,
                clock_shift,
                ..
            } => {
                b.heard += 1;
                b.nudges += u64::from(clock_shift != 0.0);
                b.links.insert((population, caller, receiver));
            }
            Kind::CallMasked { .. } => b.masked += 1,
            Kind::Contact { .. } => b.contacts += 1,
            Kind::Bubble { voiced: true, .. } => b.bubbles += 1,
            Kind::RainImpact { .. } => b.drops += 1,
            Kind::WaveBreak { .. } => b.waves += 1,
            Kind::FirePop { .. } => b.embers += 1,
            Kind::Excitation {
                candidate,
                node,
                started,
                ..
            } => {
                let i = candidate as usize;
                b.candidate_events[i] += 1;
                if started {
                    b.starts[i] += 1;
                    b.nodes[i].insert(node);
                }
            }
            _ => {}
        }
    }

    fn recent(&self, now: f64) -> Bucket {
        let tick = (now * TICKS_PER_SECOND as f64) as u64;
        let start = tick.saturating_sub(WINDOW_TICKS - 1);
        let mut sum = Bucket::default();
        for b in self
            .buckets
            .iter()
            .filter(|b| b.tick >= start && b.tick <= tick)
        {
            sum.events += b.events;
            sum.calls += b.calls;
            sum.heard += b.heard;
            sum.masked += b.masked;
            sum.nudges += b.nudges;
            sum.contacts += b.contacts;
            sum.bubbles += b.bubbles;
            sum.drops += b.drops;
            sum.waves += b.waves;
            sum.embers += b.embers;
            sum.callers.extend(&b.callers);
            sum.links.extend(&b.links);
            for i in 0..2 {
                sum.starts[i] += b.starts[i];
                sum.candidate_events[i] += b.candidate_events[i];
                sum.nodes[i].extend(&b.nodes[i]);
            }
        }
        sum
    }

    fn history(
        &self,
        now: f64,
        candidate: Option<usize>,
    ) -> ([u64; HISTORY_SECONDS as usize], u64) {
        let mut bins = [0; HISTORY_SECONDS as usize];
        let current = now as u64;
        for b in &self.buckets {
            let second = b.tick / TICKS_PER_SECOND;
            if second > current || current - second >= HISTORY_SECONDS {
                continue;
            }
            let i = (HISTORY_SECONDS - 1 - (current - second)) as usize;
            bins[i] += candidate.map_or(b.events, |i| b.candidate_events[i]);
        }
        let peak = bins.iter().copied().max().unwrap_or(0);
        (bins, peak)
    }

    pub fn lines(
        &self,
        seconds: f64,
        candidate: Option<usize>,
        width: u16,
        rows: usize,
        lost: u64,
    ) -> Vec<(Color, String)> {
        if rows == 0 {
            return Vec::new();
        }
        if !self.seen {
            return vec![(Color::DarkGrey, " Live activity: waiting...".into())];
        }
        let now = seconds.max(self.latest);
        let sum = self.recent(now);
        let count = candidate.map_or(sum.events, |i| sum.candidate_events[i]);
        let rate = count as f64 / now.clamp(0.1, 10.0);
        let partial = if lost > 0 { " (partial)" } else { "" };
        let narrow = width < 54;
        let title = if let Some(i) = candidate {
            if narrow {
                format!(
                    " Live {}: {rate:.1}/s{partial}",
                    if i == 0 { "A" } else { "B" }
                )
            } else {
                format!(
                    " Live {} / 10s: {rate:.1} events/s{partial}",
                    if i == 0 { "A" } else { "B" }
                )
            }
        } else if narrow {
            format!(" Live: {rate:.1}/s{partial}")
        } else {
            format!(" Live / 10s: {rate:.1} events/s{partial}")
        };
        let mut lines = vec![(if lost > 0 { Color::Yellow } else { Color::Cyan }, title)];
        if let Some(i) = candidate {
            lines.push((
                Color::Green,
                format!(
                    " Forces {} | nodes {} (10s)",
                    sum.starts[i],
                    sum.nodes[i].len()
                ),
            ));
            if let Some((energy, weather)) = self.resonators[i] {
                lines.push((
                    Color::White,
                    format!(" Energy {energy:.6} | weather {weather:.2}"),
                ));
            }
            lines.push((
                Color::DarkGrey,
                format!(
                    " Force starts / 10s: A {} | B {}",
                    sum.starts[0], sum.starts[1]
                ),
            ));
        } else {
            let heard = if sum.heard + sum.masked == 0 {
                "--".into()
            } else {
                format!(
                    "{:.0}%",
                    100.0 * sum.heard as f64 / (sum.heard + sum.masked) as f64
                )
            };
            lines.push((
                Color::Green,
                if narrow {
                    format!(" Callers {} | heard {heard}", sum.callers.len())
                } else {
                    let nudges = if width >= 76 {
                        format!(" | nudges {}", sum.nudges)
                    } else {
                        String::new()
                    };
                    format!(
                        " Calls {} | callers {} | heard {heard} | links {}{nudges}",
                        sum.calls,
                        sum.callers.len(),
                        sum.links.len()
                    )
                },
            ));
            let mut physical = format!(
                " Contacts {} | bubbles {} | drops {}",
                sum.contacts, sum.bubbles, sum.drops
            );
            if sum.embers > 0 {
                physical = format!(
                    " Ember pops {} | contacts {} | bubbles {}",
                    sum.embers, sum.contacts, sum.bubbles
                );
            } else if sum.waves > 0 {
                physical = format!(
                    " Wave breaks {} | bubbles {} | contacts {}",
                    sum.waves, sum.bubbles, sum.contacts
                );
            }
            lines.push((Color::White, physical));
            if let Some(Kind::Water {
                flow,
                retained_m3,
                suspended_m3,
                exported_m3,
                ..
            }) = self.water
            {
                let sediment = (suspended_m3 + exported_m3) * 1e6;
                let state = if flow == 0.0 && retained_m3 == 0.0 && sediment == 0.0 {
                    if let Some(Kind::Weather {
                        air,
                        rain,
                        daylight,
                        ..
                    }) = self.weather
                    {
                        format!(" Air {air:.2} | rain {rain:.2} | daylight {daylight:.2}")
                    } else {
                        " Water is still.".into()
                    }
                } else {
                    format!(
                        " Flow {flow:.2} | soil {:.2}L | sediment {sediment:.1}cm3",
                        retained_m3 * 1000.0
                    )
                };
                lines.push((Color::White, state));
            }
        }
        let (bins, peak) = self.history(now, candidate);
        let glyphs = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let graph: String = bins
            .iter()
            .map(|&n| {
                if n == 0 {
                    ' '
                } else {
                    glyphs[(n * 8).div_ceil(peak).min(8) as usize]
                }
            })
            .collect();
        lines.push((
            Color::DarkGrey,
            format!(" Events/s, 20s [{graph}] peak {peak}"),
        ));
        lines.truncate(rows);
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(time_seconds: f64, event: Kind) -> Record {
        Record {
            sequence: 0,
            sample: (time_seconds * 48_000.0) as u64,
            time_seconds,
            event,
        }
    }

    #[test]
    fn windows_expire_in_simulation_time_and_count_distinct_directed_interactions() {
        let mut activity = Activity::default();
        for t in [1.0, 2.0] {
            activity.record(&record(
                t,
                Kind::CallStarted {
                    population: 0,
                    caller: 2,
                    frequency_hz: 440.0,
                    level: 0.1,
                },
            ));
            activity.record(&record(
                t,
                Kind::CallHeard {
                    population: 0,
                    caller: 2,
                    receiver: 3,
                    level: 0.1,
                    clock_shift: 0.05,
                },
            ));
        }
        activity.record(&record(
            2.0,
            Kind::CallHeard {
                population: 1,
                caller: 2,
                receiver: 3,
                level: 0.1,
                clock_shift: 0.0,
            },
        ));
        activity.record(&record(
            2.0,
            Kind::CallMasked {
                population: 0,
                caller: 2,
                receiver: 4,
                level: 0.001,
                threshold: 0.01,
            },
        ));
        activity.record(&record(
            2.0,
            Kind::Weather {
                air: 0.5,
                gust: 0.5,
                rain: 0.0,
                daylight: 0.5,
            },
        ));
        let sum = activity.recent(2.0);
        assert_eq!(
            (
                sum.calls,
                sum.callers.len(),
                sum.heard,
                sum.masked,
                sum.links.len(),
                sum.nudges
            ),
            (2, 1, 3, 1, 2, 2)
        );
        assert_eq!(sum.events, 6); // State snapshots are not activity pulses.
        let text = activity.lines(2.0, None, 80, 5, 0);
        assert!(text[1].1.contains("heard 75%"));
        assert_eq!(text, activity.lines(2.0, None, 80, 5, 0)); // Pause cannot decay it.
        assert_eq!(activity.recent(11.0).calls, 1);
        assert_eq!(activity.recent(12.0).events, 0);
        assert_eq!(activity.history(22.0, None).1, 0);
    }

    #[test]
    fn bins_are_bounded_without_discarding_busy_seconds_or_mixing_candidates() {
        let mut activity = Activity::default();
        for n in 0..10_000 {
            activity.record(&record(
                1.0,
                Kind::Excitation {
                    candidate: (n % 2) as u8,
                    node: n % 4,
                    started: true,
                    duration: 0.5,
                },
            ));
        }
        assert_eq!(activity.buckets.len(), 1);
        assert_eq!(activity.recent(1.0).starts, [5000, 5000]);
        assert_eq!(activity.recent(1.0).nodes[0].len(), 2);
        assert_eq!(activity.history(1.0, Some(1)).1, 5000);
        assert!(activity.lines(1.0, Some(0), 80, 5, 4)[0]
            .1
            .contains("partial"));
        for n in 20..10_000 {
            activity.record(&record(
                n as f64 / 10.0,
                Kind::FirePop {
                    position: 0.5,
                    strength: 0.1,
                },
            ));
        }
        assert!(activity.buckets.len() <= 201);
        let sum = activity.recent(999.9);
        assert_eq!(sum.events, 100);
        assert_eq!(sum.starts, [0, 0]);
    }
}
