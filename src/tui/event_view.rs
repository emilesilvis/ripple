//! A bounded, scrollable view of events from the world currently playing.
use super::*;
use crate::events::{Batch, Kind, Record};
use std::collections::VecDeque;

const HISTORY: usize = 2048;
const FILTERS: [&str; 6] = ["All", "Calls", "Contacts", "Water", "Weather", "Resonators"];

#[derive(Default)]
pub(super) struct EventView {
    pub visible: bool,
    generation: u64,
    history: VecDeque<Record>,
    held: Option<Vec<Record>>,
    scroll: usize,
    filter: usize,
    received: u64,
    lost: u64,
    activity: super::activity::Activity,
}

impl EventView {
    pub fn receive(&mut self, batch: Batch, generation: u64) {
        self.set_generation(generation);
        if batch.generation != generation {
            return;
        }
        self.lost += batch.lost;
        for record in batch.records() {
            self.activity.record(record);
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(*record);
            self.received += 1;
        }
    }

    pub fn set_generation(&mut self, generation: u64) {
        if self.generation == generation {
            return;
        }
        self.generation = generation;
        self.history.clear();
        self.held = None;
        self.scroll = 0;
        self.received = 0;
        self.lost = 0;
        self.activity = super::activity::Activity::default();
    }

    pub fn activity_lines(
        &self,
        seconds: f64,
        candidate: Option<usize>,
        width: u16,
        rows: usize,
    ) -> Vec<(Color, String)> {
        self.activity
            .lines(seconds, candidate, width, rows, self.lost)
    }

    fn hold(&mut self) {
        if self.held.is_none() {
            self.held = Some(self.history.iter().copied().collect());
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        if code == KeyCode::Char('l') {
            self.visible = !self.visible;
            return true;
        }
        if !self.visible {
            return false;
        }
        match code {
            KeyCode::Tab => {
                self.filter = (self.filter + 1) % FILTERS.len();
                self.scroll = 0;
            }
            KeyCode::BackTab => {
                self.filter = (self.filter + FILTERS.len() - 1) % FILTERS.len();
                self.scroll = 0;
            }
            KeyCode::Char('h') => {
                if self.held.is_some() {
                    self.held = None;
                    self.scroll = 0;
                } else {
                    self.hold();
                }
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::PageUp => {
                self.hold();
                let rows = terminal::size()
                    .map(|(_, h)| h as usize)
                    .unwrap_or(24)
                    .saturating_sub(5);
                let count = self
                    .held
                    .as_ref()
                    .unwrap()
                    .iter()
                    .filter(|r| self.filter == 0 || r.event.category() == self.filter)
                    .count();
                self.scroll = (self.scroll + if code == KeyCode::PageUp { 10 } else { 1 })
                    .min(count.saturating_sub(rows));
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::PageDown => {
                self.scroll =
                    self.scroll
                        .saturating_sub(if code == KeyCode::PageDown { 10 } else { 1 });
            }
            KeyCode::End => {
                self.held = None;
                self.scroll = 0;
            }
            // Playback controls remain available; selection/search keys belong
            // to the underlying screen, where their target is visible.
            KeyCode::Char(' ' | '+' | '=' | '-' | '_' | 'r' | 'a' | 'b' | 'A' | 'B') => {
                return false
            }
            _ => {}
        }
        true
    }

    fn lines(
        &self,
        height: usize,
        width: usize,
        title: &str,
        seconds: f64,
        paused: bool,
    ) -> Vec<(Color, String)> {
        let mut lines = vec![
            (
                Color::Cyan,
                format!(" ripple / live events / {}", FILTERS[self.filter]),
            ),
            (
                Color::White,
                format!(
                    " {} | {seconds:.1}s | {title}",
                    if paused { "Paused" } else { "Playing" }
                ),
            ),
            (
                if self.lost > 0 {
                    Color::Yellow
                } else {
                    Color::DarkGrey
                },
                if width < 64 {
                    format!(
                        " {} | missed {} | {} seen",
                        if self.held.is_some() { "Held" } else { "Live" },
                        self.lost,
                        self.received
                    )
                } else {
                    format!(
                        " {} | recent {} of {} | missed {}",
                        if self.held.is_some() { "Held" } else { "Live" },
                        self.history.len(),
                        self.received,
                        self.lost
                    )
                },
            ),
        ];
        let records: Vec<_> = match &self.held {
            Some(held) => held.iter().collect(),
            None => self.history.iter().collect(),
        };
        let records: Vec<_> = records
            .into_iter()
            .filter(|r| self.filter == 0 || r.event.category() == self.filter)
            .collect();
        let rows = height.saturating_sub(5);
        let end = records
            .len()
            .saturating_sub(self.scroll.min(records.len().saturating_sub(rows)));
        let start = end.saturating_sub(rows);
        for record in &records[start..end] {
            let color = match record.event.category() {
                1 => Color::Green,
                2 => Color::Yellow,
                3 => Color::Cyan,
                4 => Color::DarkCyan,
                5 => Color::Magenta,
                _ => Color::White,
            };
            lines.push((
                color,
                format!(" {:8.3} {}", record.time_seconds, describe(record.event)),
            ));
        }
        if records.is_empty() && rows > 0 {
            lines.push((Color::DarkGrey, " Waiting for matching events...".into()));
        }
        while lines.len() < height.saturating_sub(2) {
            lines.push((Color::White, String::new()));
        }
        if width >= 64 {
            lines.push((
                Color::DarkGrey,
                " l back | Tab filter | h hold/resume view | End live tail".into(),
            ));
            lines.push((
                Color::DarkGrey,
                " Arrows/PgUp/PgDn scroll | Space pause audio | +/- volume | q quit".into(),
            ));
        } else {
            lines.push((Color::DarkGrey, " l back | Tab filter | h hold".into()));
            lines.push((Color::DarkGrey, " PgUp/Dn | Space pause | q quit".into()));
        }
        lines
    }

    pub fn draw(&self, title: &str, seconds: f64, paused: bool) -> io::Result<()> {
        let (width, height) = terminal::size()?;
        let mut stdout = io::stdout().lock();
        for (row, (color, line)) in self
            .lines(height as usize, width as usize, title, seconds, paused)
            .into_iter()
            .take(height as usize)
            .enumerate()
        {
            let shown: String = line
                .chars()
                .take(width.saturating_sub(1) as usize)
                .collect();
            queue!(
                stdout,
                MoveTo(0, row as u16),
                SetForegroundColor(color),
                Clear(ClearType::CurrentLine),
                Print(shown)
            )?;
        }
        queue!(stdout, ResetColor, Clear(ClearType::FromCursorDown))?;
        stdout.flush()
    }
}

fn describe(event: Kind) -> String {
    match event {
        Kind::Started { seed } => format!("world started; seed {seed}"),
        Kind::RainImpact { x, y, surface, strength } => format!("rain hit {surface} at ({x:.1}, {y:.1}); strength {strength:.3}"),
        Kind::Bubble { cause, radius_m, strength, pan, voiced } => format!("{cause} bubble/packet; {:.2} mm, strength {strength:.3}, pan {pan:+.2}{}", radius_m * 1000.0, if voiced { "" } else { " (voice pool busy)" }),
        Kind::WaveBreak { pan, strength } => format!("wave broke; strength {strength:.3}, pan {pan:+.2}"),
        Kind::Contact { body, position, velocity_kick } => format!("body {} struck at {position:.2}; kick {velocity_kick:+.3}", body + 1),
        Kind::FirePop { position, strength } => format!("ember popped at {position:.2}; strength {strength:.3}"),
        Kind::CallStarted { population, caller, frequency_hz, .. } => format!("group {} / {} called at {frequency_hz:.0} Hz", population + 1, caller + 1),
        Kind::CallEnded { population, caller } => format!("group {} / {} finished its call", population + 1, caller + 1),
        Kind::CallHeard { population, caller, receiver, clock_shift, .. } => format!("group {} / {} heard {}; clock {clock_shift:+.4}", population + 1, receiver + 1, caller + 1),
        Kind::CallMasked { population, caller, receiver, level, threshold } => format!("group {} / {} could not hear {}; level {level:.4}, threshold {threshold:.4}", population + 1, receiver + 1, caller + 1),
        Kind::Weather { air, gust, rain, daylight } => format!("weather: air {air:.3}, gust {gust:.3}, rain {rain:.3}, daylight {daylight:.3}"),
        Kind::Water { flow, surface_m3, retained_m3, suspended_m3, exported_m3, .. } => format!("water: flow {flow:.3}, surface {surface_m3:.3} m3, soil {retained_m3:.3} m3, sediment {:.1} cm3", (suspended_m3 + exported_m3) * 1e6),
        Kind::Excitation { candidate, node, started, duration } => format!("{} / node {} force {}; duration {duration:.2}s", if candidate == 0 { "A" } else { "B" }, node + 1, if started { "started" } else { "ended" }),
        Kind::Resonators { candidate, energy, weather } => format!("{} resonators: energy {energy:.6}, weather {weather:.3}", if candidate == 0 { "A" } else { "B" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_summary_outlives_log_history_and_ignores_hold_and_filter() {
        let mut log = crate::events::EventLog::new(48_000.0);
        log.enable();
        let mut view = EventView::default();
        view.handle_key(KeyCode::Char('l'));
        view.handle_key(KeyCode::Char('h'));
        view.handle_key(KeyCode::Tab);
        for _ in 0..50 {
            for _ in 0..64 {
                log.emit(Kind::CallStarted {
                    population: 0,
                    caller: 0,
                    frequency_hz: 440.0,
                    level: 0.1,
                });
                log.advance();
            }
            let mut batch = Batch::new(0);
            log.drain(&mut batch);
            view.receive(batch, 0);
        }
        assert_eq!(view.history.len(), HISTORY);
        assert!(view.held.as_ref().unwrap().is_empty());
        let lines = view.activity_lines(1.0, None, 80, 5);
        assert!(lines[1].1.contains("Calls 3200 | callers 1"));
        let mut gap = Batch::new(0);
        gap.lost = 3;
        view.receive(gap, 0);
        assert!(view.activity_lines(1.0, None, 32, 1)[0]
            .1
            .contains("partial"));
        view.set_generation(1);
        assert!(view.activity_lines(0.0, None, 80, 5)[0]
            .1
            .contains("waiting"));
        let mut stale = Batch::new(0);
        log.drain(&mut stale);
        view.receive(stale, 1);
        assert!(view.activity_lines(0.0, None, 80, 5)[0]
            .1
            .contains("waiting"));
    }

    #[test]
    fn holding_filtering_and_world_changes_do_not_mix_timelines() {
        let mut log = crate::events::EventLog::new(100.0);
        log.enable();
        log.emit(Kind::Started { seed: 1 });
        let mut batch = Batch::new(1);
        log.drain(&mut batch);
        let mut view = EventView::default();
        view.receive(batch, 1);
        assert!(view.handle_key(KeyCode::Char('l')));
        view.handle_key(KeyCode::Char('h'));
        log.advance();
        log.emit(Kind::CallEnded {
            population: 0,
            caller: 0,
        });
        let mut batch = Batch::new(1);
        log.drain(&mut batch);
        view.receive(batch, 1);
        assert_eq!(view.history.len(), 2);
        assert_eq!(view.held.as_ref().unwrap().len(), 1);
        view.handle_key(KeyCode::End);
        view.handle_key(KeyCode::Tab);
        let text = view
            .lines(8, 32, "test", 0.01, false)
            .into_iter()
            .map(|(_, l)| l)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("finished its call"));
        assert!(!text.contains("world started"));
        assert!(text.contains("q quit"));
        view.set_generation(2);
        let mut stale = Batch::new(1);
        stale.lost = 5;
        view.receive(stale, 2);
        assert!(view.history.is_empty());
        assert_eq!(view.received, 0);
        assert_eq!(view.lost, 0);
    }
}
