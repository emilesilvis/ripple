//! Live world selection. The audio callback owns the world; construction and
//! disposal happen on the UI side so changing scenes cannot block playback.

use crate::{alien, presets, world::World};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{self, IsTerminal, Write};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};

mod activity;
mod event_view;
mod visualization;
pub mod library;
use event_view::EventView;
use visualization::Visualization;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Metrics = [(&'static str, [f64; 2], &'static str); 4];

enum Source {
    World(Box<World>),
    Alien(Box<alien::LiveDemo>),
    Discovered(Box<alien::worlds::Soundscape>),
    Silence,
}

impl From<World> for Source {
    fn from(world: World) -> Self {
        Self::World(Box::new(world))
    }
}

impl Source {
    fn visualize(&self, frame: &mut crate::visual::Frame) {
        match self {
            Self::World(world) => world.visualize(frame),
            Self::Alien(demo) => demo.visualize(frame),
            Self::Discovered(world) => world.visualize(frame),
            Self::Silence => frame.clear(),
        }
    }

    fn build(selection: usize, sr: f32, seed: u32, discovery: Option<&alien::Discovery>) -> Self {
        match discovery {
            Some(discovery) => {
                Self::Alien(Box::new(alien::LiveDemo::from_discovery(sr, discovery)))
            }
            None => presets::build(presets::PRESETS[selection].name, sr, seed)
                .unwrap()
                .into(),
        }
    }

    fn next_sample(&mut self) -> (f32, f32) {
        match self {
            Self::World(world) => world.next_sample(),
            Self::Alien(demo) => demo.next_sample(),
            Self::Discovered(world) => world.next_sample(),
            Self::Silence => (0.0, 0.0),
        }
    }

    fn observe(&mut self) {
        match self {
            Self::World(world) => world.observe(),
            Self::Alien(demo) => demo.observe(),
            Self::Discovered(world) => world.observe(),
            Self::Silence => {}
        }
    }

    fn drain_events(&mut self, batch: &mut crate::events::Batch) {
        match self {
            Self::World(world) => world.drain_events(batch),
            Self::Alien(demo) => demo.drain_events(batch),
            Self::Discovered(world) => world.drain_events(batch),
            Self::Silence => {}
        }
    }

    fn select(&mut self, condition: usize) {
        if let Self::Alien(demo) = self {
            demo.select(condition);
        }
    }

    fn metrics(&self) -> Option<Metrics> {
        match self {
            Self::Alien(demo) => Some(demo.metrics()),
            Self::World(_) | Self::Discovered(_) | Self::Silence => None,
        }
    }
}

enum Command {
    Replace {
        generation: u64,
        world: Source,
        paused: bool,
    },
    Volume(f32),
    Pause(bool),
    Condition(usize),
}

#[derive(Clone, Copy, Default)]
struct Snapshot {
    generation: u64,
    seconds: f64,
    peak: f32,
    rms: f64,
    invalid_samples: u64,
    clipped_samples: u64,
    metrics: Option<Metrics>,
}

struct Player {
    world: Source,
    sr: f32,
    commands: Receiver<Command>,
    snapshots: SyncSender<Snapshot>,
    retired: SyncSender<Source>,
    deferred_retirement: Option<Source>,
    event_sender: Option<SyncSender<crate::events::Batch>>,
    events_lost: u64,
    visual: Option<crate::visual::Shared>,
    generation: u64,
    volume: f32,
    volume_target: f32,
    paused: bool,
    fade: f32,
    elapsed_samples: u64,
    meter_samples: u32,
    peak: f32,
    squares: f64,
    invalid_samples: u64,
    clipped_samples: u64,
}

impl Player {
    fn new(
        world: impl Into<Source>,
        sr: f32,
        commands: Receiver<Command>,
        snapshots: SyncSender<Snapshot>,
        retired: SyncSender<Source>,
    ) -> Self {
        Self {
            world: world.into(),
            sr,
            commands,
            snapshots,
            retired,
            deferred_retirement: None,
            event_sender: None,
            events_lost: 0,
            visual: None,
            generation: 0,
            volume: 0.0,
            volume_target: 0.7,
            paused: false,
            fade: 0.0,
            elapsed_samples: 0,
            meter_samples: 0,
            peak: 0.0,
            squares: 0.0,
            invalid_samples: 0,
            clipped_samples: 0,
        }
    }

    fn observing(mut self, sender: SyncSender<crate::events::Batch>) -> Self {
        self.world.observe();
        self.event_sender = Some(sender);
        self
    }

    fn visualizing(mut self, shared: crate::visual::Shared) -> Self {
        self.visual = Some(shared);
        self.publish_visual();
        self
    }

    fn publish_visual(&self) {
        if let Some(shared) = &self.visual {
            // The UI may be copying the previous frame. Skip this observation
            // instead of ever waiting for rendering on the audio thread.
            if let Ok(mut frame) = shared.try_lock() {
                self.world.visualize(&mut frame);
                frame.generation = self.generation;
                frame.seconds = self.elapsed_samples as f64 / self.sr as f64;
            }
        }
    }

    fn publish_events(&mut self) {
        let Some(sender) = &self.event_sender else {
            return;
        };
        let mut batch = crate::events::Batch::new(self.generation);
        self.world.drain_events(&mut batch);
        batch.lost += std::mem::take(&mut self.events_lost);
        if batch.records().is_empty() && batch.lost == 0 {
            return;
        }
        if let Err(TrySendError::Full(batch)) = sender.try_send(batch) {
            self.events_lost = batch.lost + batch.records().len() as u64;
        }
    }

    fn begin_buffer(&mut self) {
        if let Some(old) = self.deferred_retirement.take() {
            if let Err(TrySendError::Full(old)) = self.retired.try_send(old) {
                self.deferred_retirement = Some(old);
            }
        }
        if self.deferred_retirement.is_some() {
            return;
        }
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Replace {
                    generation,
                    mut world,
                    paused,
                } => {
                    if self.event_sender.is_some() {
                        world.observe();
                    }
                    self.events_lost = 0;
                    let old = std::mem::replace(&mut self.world, world);
                    if let Err(TrySendError::Full(old)) = self.retired.try_send(old) {
                        self.deferred_retirement = Some(old);
                    }
                    self.generation = generation;
                    self.paused = paused;
                    self.elapsed_samples = 0;
                    self.volume = 0.0;
                    self.fade = 0.0;
                    self.invalid_samples = 0;
                    self.clipped_samples = 0;
                    self.peak = 0.0;
                    self.squares = 0.0;
                    self.meter_samples = 0;
                }
                Command::Volume(volume) => self.volume_target = volume,
                Command::Pause(paused) => self.paused = paused,
                Command::Condition(condition) => self.world.select(condition),
            }
            if self.deferred_retirement.is_some() {
                break;
            }
        }
    }

    fn next(&mut self) -> (f32, f32) {
        let mut output = (0.0, 0.0);
        if !self.paused {
            let sample = self.world.next_sample();
            self.elapsed_samples += 1;
            self.volume += (self.volume_target - self.volume) / (0.03 * self.sr);
            self.fade = (self.fade + 1.0 / (0.1 * self.sr)).min(1.0);
            output = (
                sample.0 * self.volume * self.fade,
                sample.1 * self.volume * self.fade,
            );
            if !output.0.is_finite() || !output.1.is_finite() {
                self.invalid_samples += 1;
                self.paused = true;
                output = (0.0, 0.0);
            }
        }
        self.peak = self.peak.max(output.0.abs()).max(output.1.abs());
        self.squares +=
            (output.0 as f64 * output.0 as f64 + output.1 as f64 * output.1 as f64) * 0.5;
        if output.0.abs() > 1.0 || output.1.abs() > 1.0 {
            self.clipped_samples += 1;
        }
        self.meter_samples += 1;
        if self.meter_samples >= (self.sr / 10.0) as u32 {
            self.publish_visual();
            let _ = self.snapshots.try_send(Snapshot {
                generation: self.generation,
                seconds: self.elapsed_samples as f64 / self.sr as f64,
                peak: self.peak,
                rms: (self.squares / self.meter_samples as f64).sqrt(),
                invalid_samples: self.invalid_samples,
                clipped_samples: self.clipped_samples,
                metrics: self.world.metrics(),
            });
            self.meter_samples = 0;
            self.peak = 0.0;
            self.squares = 0.0;
        }
        (output.0.clamp(-1.0, 1.0), output.1.clamp(-1.0, 1.0))
    }
}

fn make_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut player: Player,
    errors: SyncSender<String>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            player.begin_buffer();
            for frame in data.chunks_mut(channels) {
                let (l, r) = player.next();
                if channels == 1 {
                    frame[0] = T::from_sample((l + r) * 0.5);
                } else {
                    frame[0] = T::from_sample(l);
                    frame[1] = T::from_sample(r);
                    for sample in &mut frame[2..] {
                        *sample = T::from_sample(0.0);
                    }
                }
            }
            player.publish_events();
        },
        move |error| {
            let _ = errors.try_send(error.to_string());
        },
        None,
    )?)
}

struct TerminalSession;

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), ResetColor, Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

struct Ui {
    cursor: usize,
    playing: usize,
    seed: u32,
    volume: f32,
    paused: bool,
    generation: u64,
    loading: Option<usize>,
    pending_load: Option<(u64, usize, u32, Option<alien::Discovery>)>,
    building: bool,
    snapshot: Snapshot,
    events: EventView,
    visual: Visualization,
    discovery: Option<alien::Discovery>,
    condition: usize,
    notice: Option<String>,
}

impl Ui {
    fn request_world(&mut self, selection: usize) {
        self.generation += 1;
        self.loading = Some(selection);
        self.pending_load = Some((
            self.generation,
            selection,
            self.seed,
            self.discovery.clone(),
        ));
        self.paused = false;
        self.notice = None;
    }

    fn name(&self) -> &'static str {
        if self.discovery.is_some() {
            "alien discovery"
        } else {
            presets::PRESETS[self.playing].name
        }
    }

    fn discovery_lines(
        &self,
        device: &str,
        sr: u32,
        width: u16,
        height: u16,
    ) -> Vec<(Color, String)> {
        let discovery = self.discovery.as_ref().unwrap();
        let mut lines = vec![
            (
                Color::Cyan,
                " ripple / alien discovery | v visualize | l log".into(),
            ),
            (Color::DarkGrey, format!(" {device} | {sr} Hz")),
            (
                Color::White,
                format!(
                    " Seed {} | Generation {}",
                    self.seed,
                    discovery.generation()
                ),
            ),
            (
                Color::White,
                format!(
                    " {} | {:.1}s | volume {:.0}%",
                    if self.loading.is_some() {
                        "Discovering; previous audio continues"
                    } else if self.paused {
                        "Paused"
                    } else {
                        "Playing"
                    },
                    self.snapshot.seconds,
                    self.volume * 100.0
                ),
            ),
            (
                Color::Cyan,
                format!(
                    " {} A: Parent      {} B: Discovery",
                    if self.condition == 0 { "[x]" } else { "[ ]" },
                    if self.condition == 1 { "[x]" } else { "[ ]" }
                ),
            ),
            (
                Color::DarkGrey,
                format!(" {:23} {:>11} {:>11}", "", "A", "B"),
            ),
        ];
        if let Some(metrics) = self.snapshot.metrics {
            for (label, values, unit) in metrics {
                let number = |value: f64| {
                    if unit == "count" {
                        format!("{value:.0}")
                    } else {
                        format!("{value:.4}")
                    }
                };
                lines.push((
                    Color::White,
                    format!(
                        " {:23} {:>11} {:>11}",
                        label,
                        number(values[0]),
                        number(values[1])
                    ),
                ));
            }
        }
        lines.push((
            Color::Cyan,
            format!(
                " {:6.1} dBFS | peak {:.3}",
                20.0 * self.snapshot.rms.max(1e-9).log10(),
                self.snapshot.peak
            ),
        ));
        if self.snapshot.invalid_samples > 0 {
            lines.push((
                Color::Red,
                " Invalid audio; paused. Press r to restart.".into(),
            ));
        } else if let Some(notice) = &self.notice {
            lines.push((Color::Yellow, format!(" {notice}")));
        }
        let rows = (height as usize).saturating_sub(lines.len() + 3).min(5);
        lines.extend(self.events.activity_lines(
            self.snapshot.seconds,
            Some(self.condition),
            width,
            rows,
        ));
        lines.push((
            Color::DarkGrey,
            " a/b compare | e keep and evolve | s save".into(),
        ));
        lines.push((
            Color::DarkGrey,
            " Space pause | +/- volume | r replay | n new seed".into(),
        ));
        lines.push((
            Color::DarkGrey,
            " Saves go in discoveries/ | q / Esc quit".into(),
        ));
        lines
    }

    fn lines(&self, device: &str, sr: u32, width: u16, height: u16) -> Vec<(Color, String)> {
        if self.discovery.is_some() {
            return self.discovery_lines(device, sr, width, height);
        }
        let mut lines = vec![
            (
                Color::Cyan,
                " ripple / a world you can hear | v visualize | l log".into(),
            ),
            (Color::DarkGrey, format!(" {device} | {sr} Hz")),
        ];
        for (i, preset) in presets::PRESETS.iter().enumerate() {
            lines.push((
                if self.cursor == i {
                    Color::Cyan
                } else {
                    Color::White
                },
                format!(
                    " {} {:13} {} {}",
                    if self.cursor == i { ">" } else { " " },
                    preset.name,
                    if self.playing == i { "*" } else { " " },
                    preset.description
                ),
            ));
        }
        if height >= 24 {
            lines.push((Color::White, String::new()));
        }
        if let Some(i) = self.loading {
            lines.push((
                Color::Yellow,
                format!(
                    " Preparing {}... current audio continues.",
                    presets::PRESETS[i].name
                ),
            ));
        } else {
            lines.push((
                Color::White,
                format!(
                    " {}: {} | {:.1}s | volume {:.0}%",
                    if self.paused { "Paused " } else { "Playing" },
                    self.name(),
                    self.snapshot.seconds,
                    self.volume * 100.0
                ),
            ));
        }
        lines.push((
            Color::DarkGrey,
            format!(
                " Seed {} | r repeats this world; n grows a new one",
                self.seed
            ),
        ));
        let db = 20.0 * self.snapshot.rms.max(1e-9).log10();
        let bars = (((db + 60.0) / 60.0).clamp(0.0, 1.0) * 24.0).round() as usize;
        lines.push((
            Color::Cyan,
            format!(
                " [{}{}] {:6.1} dBFS | peak {:.3}",
                "|".repeat(bars),
                " ".repeat(24 - bars),
                db,
                self.snapshot.peak
            ),
        ));
        if self.snapshot.invalid_samples > 0 {
            lines.push((
                Color::Red,
                " Non-finite audio detected; playback stopped. Press r to restart.".into(),
            ));
        } else if self.snapshot.clipped_samples > 0 {
            lines.push((
                Color::Yellow,
                " Output reached its limit; lower the volume with -.".into(),
            ));
        }
        if height >= 24 {
            lines.push((Color::White, String::new()));
        }
        let rows = (height as usize)
            .saturating_sub(lines.len() + 2 + usize::from(height >= 24))
            .min(5);
        lines.extend(
            self.events
                .activity_lines(self.snapshot.seconds, None, width, rows),
        );
        if height >= 24 {
            lines.push((Color::White, String::new()));
        }
        lines.push((
            Color::DarkGrey,
            " Up/Down or j/k select | Enter play | Space pause".into(),
        ));
        lines.push((
            Color::DarkGrey,
            " +/- volume | r restart | n new seed | q / Esc quit".into(),
        ));
        lines
    }

    fn draw(&self, device: &str, sr: u32) -> io::Result<()> {
        if self.visual.visible {
            return self.visual.draw(
                self.name(),
                self.paused,
                self.loading.is_some(),
                &self.events,
                &self.snapshot,
            );
        }
        if self.events.visible {
            let title = if self.discovery.is_some() {
                if self.condition == 0 {
                    "A parent audible; A+B simulated"
                } else {
                    "B discovery audible; A+B simulated"
                }
            } else {
                self.name()
            };
            return self.events.draw(title, self.snapshot.seconds, self.paused);
        }
        let (width, height) = terminal::size()?;
        let mut stdout = io::stdout().lock();
        let lines = if (width < 54 || height < 20) && self.discovery.is_some() {
            let mut lines = vec![
                (
                    Color::Cyan,
                    " ripple / discovery | v visualize | l log".into(),
                ),
                (
                    Color::White,
                    format!(
                        " {} | {:.1}s",
                        if self.paused { "paused" } else { "playing" },
                        self.snapshot.seconds
                    ),
                ),
            ];
            lines.extend(self.events.activity_lines(
                self.snapshot.seconds,
                Some(self.condition),
                width,
                (height as usize).saturating_sub(5).min(2),
            ));
            lines.extend([
                (Color::DarkGrey, " a/b compare | e evolve | s save".into()),
                (Color::DarkGrey, " Space pause | +/- | r replay".into()),
                (Color::DarkGrey, " n new seed | q quit".into()),
            ]);
            lines
        } else if width < 54 || height < 20 {
            let mut lines = vec![
                (Color::Cyan, " ripple / worlds | v visualize | l log".into()),
                (
                    Color::White,
                    format!(
                        " {} | {} | {:.1}s",
                        presets::PRESETS[self.playing].name,
                        if self.paused { "paused" } else { "playing" },
                        self.snapshot.seconds
                    ),
                ),
            ];
            lines.extend(self.events.activity_lines(
                self.snapshot.seconds,
                None,
                width,
                (height as usize).saturating_sub(5).min(2),
            ));
            lines.extend([
                (Color::DarkGrey, " j/k select | Enter play".into()),
                (Color::DarkGrey, " +/- | r restart | n seed".into()),
                (Color::DarkGrey, " Space pause | q quit".into()),
            ]);
            lines
        } else {
            self.lines(device, sr, width, height)
        };
        for (row, (color, line)) in lines.into_iter().take(height as usize).enumerate() {
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

pub fn run(name: &str, seed: u32) -> Result<()> {
    play(name, seed, None)
}

pub fn discover(discovery: alien::Discovery) -> Result<()> {
    play(presets::DEFAULT, discovery.seed, Some(discovery))
}

fn play(name: &str, seed: u32, discovery: Option<alien::Discovery>) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("the world player needs an interactive terminal; run ripple in your terminal or use `ripple render`".into());
    }
    let start = presets::PRESETS
        .iter()
        .position(|p| p.name == name)
        .ok_or_else(|| format!("unknown world: {name}"))?;
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no default audio output device found")?;
    let supported = device.default_output_config()?;
    let config: cpal::StreamConfig = supported.clone().into();
    let sr = config.sample_rate.0;
    let device_name = device.name().unwrap_or_else(|_| "default output".into());
    if discovery.is_some() && sr < 8000 {
        return Err("alien discovery needs an audio sample rate of at least 8000 Hz".into());
    }
    println!(
        "Preparing {}...",
        if discovery.is_some() {
            "alien discovery"
        } else {
            name
        }
    );
    let world = Source::build(start, sr as f32, seed, discovery.as_ref());
    let metrics = world.metrics();
    let (commands_tx, commands_rx) = mpsc::sync_channel(32);
    let (snapshots_tx, snapshots_rx) = mpsc::sync_channel(4);
    let (events_tx, events_rx) = mpsc::sync_channel(16);
    let (retired_tx, retired_rx) = mpsc::sync_channel(4);
    let (errors_tx, errors_rx) = mpsc::sync_channel(1);
    let (loaded_tx, loaded_rx) = mpsc::channel::<(u64, usize, Source)>();
    let visual = Visualization::default();
    let player = Player::new(world, sr as f32, commands_rx, snapshots_tx, retired_tx)
        .observing(events_tx)
        .visualizing(visual.shared.clone());
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => make_stream::<f32>(&device, &config, player, errors_tx)?,
        cpal::SampleFormat::I16 => make_stream::<i16>(&device, &config, player, errors_tx)?,
        cpal::SampleFormat::U16 => make_stream::<u16>(&device, &config, player, errors_tx)?,
        other => return Err(format!("unsupported audio sample format: {other}").into()),
    };
    let _terminal = TerminalSession::enter()?;
    let mut ui = Ui {
        cursor: start,
        playing: start,
        seed,
        volume: 0.7,
        paused: false,
        generation: 0,
        loading: None,
        pending_load: None,
        building: false,
        snapshot: Snapshot {
            metrics,
            ..Snapshot::default()
        },
        events: EventView::default(),
        visual,
        discovery,
        condition: 1,
        notice: None,
    };
    stream.play()?;
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    loop {
        for retired in retired_rx.try_iter() {
            drop(retired);
        }
        if let Ok(error) = errors_rx.try_recv() {
            return Err(format!("audio output failed: {error}").into());
        }
        for (generation, selection, mut world) in loaded_rx.try_iter() {
            ui.building = false;
            if generation == ui.generation {
                world.select(ui.condition);
                let metrics = world.metrics();
                commands_tx.send(Command::Replace {
                    generation,
                    world,
                    paused: ui.paused,
                })?;
                ui.playing = selection;
                ui.loading = None;
                ui.snapshot = Snapshot {
                    generation,
                    metrics,
                    ..Snapshot::default()
                };
            }
        }
        // At most one build runs. Rapid selection changes overwrite one pending
        // request rather than spawning competing terrain simulations.
        if !ui.building {
            if let Some((generation, selection, seed, discovery)) = ui.pending_load.take() {
                ui.building = true;
                let sender = loaded_tx.clone();
                std::thread::spawn(move || {
                    let world = Source::build(selection, sr as f32, seed, discovery.as_ref());
                    let _ = sender.send((generation, selection, world));
                });
            }
        }
        for snapshot in snapshots_rx.try_iter() {
            if snapshot.generation == ui.snapshot.generation {
                if snapshot.invalid_samples > 0 && ui.loading.is_none() {
                    ui.paused = true;
                }
                ui.snapshot = snapshot;
            }
        }
        ui.events.set_generation(ui.snapshot.generation);
        for batch in events_rx.try_iter() {
            ui.events.receive(batch, ui.snapshot.generation);
        }
        if last_draw.elapsed() >= Duration::from_millis(100) {
            ui.visual.refresh(ui.snapshot.generation);
            ui.draw(&device_name, sr)?;
            last_draw = Instant::now();
        }
        if !event::poll(Duration::from_millis(30))? {
            continue;
        }
        match event::read()? {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) if kind != KeyEventKind::Release => {
                if code == KeyCode::Esc
                    || code == KeyCode::Char('q')
                    || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }
                if ui.visual.handle_key(code, &mut ui.events.visible) || ui.events.handle_key(code)
                {
                    last_draw = Instant::now() - Duration::from_secs(1);
                    continue;
                }
                match code {
                    KeyCode::Up | KeyCode::Char('k') if ui.discovery.is_none() => {
                        ui.cursor =
                            (ui.cursor + presets::PRESETS.len() - 1) % presets::PRESETS.len()
                    }
                    KeyCode::Down | KeyCode::Char('j') if ui.discovery.is_none() => {
                        ui.cursor = (ui.cursor + 1) % presets::PRESETS.len()
                    }
                    KeyCode::Enter if ui.discovery.is_none() => {
                        ui.request_world(ui.cursor);
                        ui.visual.visible = true;
                    }
                    KeyCode::Char('a' | 'A' | 'b' | 'B') | KeyCode::Left | KeyCode::Right
                        if ui.discovery.is_some() && ui.loading.is_none() =>
                    {
                        ui.condition =
                            usize::from(matches!(code, KeyCode::Char('b' | 'B') | KeyCode::Right));
                        commands_tx.send(Command::Condition(ui.condition))?;
                    }
                    KeyCode::Char('e') if ui.discovery.is_some() && ui.loading.is_none() => {
                        match ui.discovery.as_mut().unwrap().keep(ui.condition) {
                            Ok(()) => {
                                ui.condition = 1;
                                ui.request_world(ui.playing);
                            }
                            Err(error) => ui.notice = Some(error.to_string()),
                        }
                    }
                    KeyCode::Char('s') if ui.discovery.is_some() && ui.loading.is_none() => {
                        ui.notice = Some(
                            match ui
                                .discovery
                                .as_ref()
                                .unwrap()
                                .save(std::path::Path::new("discoveries"))
                            {
                                Ok(path) => format!(
                                    "Saved: {}",
                                    path.file_name().unwrap().to_string_lossy()
                                ),
                                Err(error) => format!("Save failed: {error}"),
                            },
                        );
                    }
                    KeyCode::Char(' ') => {
                        ui.paused = !ui.paused;
                        commands_tx.send(Command::Pause(ui.paused))?;
                    }
                    KeyCode::Char('+' | '=') => {
                        ui.volume = (ui.volume + 0.1).min(2.0);
                        commands_tx.send(Command::Volume(ui.volume))?;
                    }
                    KeyCode::Char('-' | '_') => {
                        ui.volume = (ui.volume - 0.1).max(0.0);
                        commands_tx.send(Command::Volume(ui.volume))?;
                    }
                    KeyCode::Char('r') | KeyCode::Char('n') => {
                        if code == KeyCode::Char('n') {
                            ui.seed = crate::fresh_seed();
                            if ui.discovery.is_some() {
                                ui.discovery = Some(alien::Discovery::new(ui.seed));
                                ui.condition = 1;
                            }
                        }
                        ui.request_world(ui.playing);
                    }
                    _ => {}
                }
                last_draw = Instant::now() - Duration::from_secs(1);
            }
            Event::Resize(_, _) => last_draw = Instant::now() - Duration::from_secs(1),
            _ => {}
        }
    }
    drop(stream);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visualization_is_passive_even_when_the_ui_is_contended_and_pause_freezes_state() {
        let (base, commands, _, retired) = player();
        let (mut reference, _, _, _) = player();
        let visual = Visualization::default();
        let mut observed = base.visualizing(visual.shared.clone());
        let guard = visual.shared.lock().unwrap();
        for _ in 0..9600 {
            assert_eq!(observed.next(), reference.next());
        }
        assert_eq!(guard.seconds, 0.0); // busy UI: observations skipped, audio continued
        drop(guard);
        observed.publish_visual();
        commands.send(Command::Pause(true)).unwrap();
        observed.begin_buffer();
        let frozen = format!("{:?}", visual.shared.lock().unwrap());
        for _ in 0..9600 {
            assert_eq!(observed.next(), (0.0, 0.0));
        }
        assert_eq!(format!("{:?}", visual.shared.lock().unwrap()), frozen);
        commands
            .send(Command::Replace {
                generation: 4,
                paused: false,
                world: presets::build("hearth", 48_000.0, 17).unwrap().into(),
            })
            .unwrap();
        observed.begin_buffer();
        drop(retired.try_recv().unwrap());
        for _ in 0..4800 {
            observed.next();
        }
        let frame = visual.shared.lock().unwrap();
        assert_eq!(frame.generation, 4);
        assert_eq!(frame.seed, 17);
        assert_eq!(frame.seconds, 0.1);
        assert!(frame.bodies.values.is_empty());
        assert!(frame.fire.is_some());
    }

    fn player() -> (
        Player,
        SyncSender<Command>,
        Receiver<Snapshot>,
        Receiver<Source>,
    ) {
        let (tx, rx) = mpsc::sync_channel(32);
        let (snapshot_tx, snapshot_rx) = mpsc::sync_channel(4);
        let (retired_tx, retired_rx) = mpsc::sync_channel(4);
        let player = Player::new(
            presets::build("mountain", 48_000.0, 12345).unwrap(),
            48_000.0,
            rx,
            snapshot_tx,
            retired_tx,
        );
        (player, tx, snapshot_rx, retired_rx)
    }

    #[test]
    fn pause_does_not_advance_the_world() {
        let (mut paused, commands, _, _) = player();
        let (mut uninterrupted, _, _, _) = player();
        for _ in 0..4800 {
            assert_eq!(paused.next(), uninterrupted.next());
        }
        commands.send(Command::Pause(true)).unwrap();
        paused.begin_buffer();
        for _ in 0..4800 {
            assert_eq!(paused.next(), (0.0, 0.0));
        }
        assert_eq!(paused.elapsed_samples, 4800);
        commands.send(Command::Pause(false)).unwrap();
        paused.begin_buffer();
        for _ in 0..4800 {
            assert_eq!(paused.next(), uninterrupted.next());
        }
    }

    #[test]
    fn a_full_event_channel_preserves_audio_and_reports_loss_then_restarts_cleanly() {
        let (base, commands, _, retired) = player();
        let (mut reference, _, _, _) = player();
        let (sender, receiver) = mpsc::sync_channel(1);
        let mut observed = base.observing(sender);
        for frame in 0..48_000 * 2 {
            assert_eq!(observed.next(), reference.next());
            if frame % 512 == 511 {
                observed.publish_events();
            }
        }
        assert!(observed.events_lost > 0);
        let first = receiver.try_recv().unwrap();
        assert_eq!(first.records()[0].sample, 0);
        observed.publish_events();
        assert!(receiver.try_recv().unwrap().lost > 0);
        commands.send(Command::Pause(true)).unwrap();
        observed.begin_buffer();
        let elapsed = observed.elapsed_samples;
        for _ in 0..512 {
            assert_eq!(observed.next(), (0.0, 0.0));
        }
        observed.publish_events();
        assert!(receiver.try_recv().is_err());
        assert_eq!(observed.elapsed_samples, elapsed);
        commands
            .send(Command::Replace {
                generation: 9,
                paused: false,
                world: presets::build("mountain", 48_000.0, 12345).unwrap().into(),
            })
            .unwrap();
        observed.begin_buffer();
        drop(retired.try_recv().unwrap());
        observed.next();
        observed.publish_events();
        let restarted = receiver.try_recv().unwrap();
        assert_eq!(restarted.generation, 9);
        assert_eq!(restarted.lost, 0);
        assert_eq!(restarted.records()[0].sample, 0);
        assert!(matches!(
            restarted.records()[0].event,
            crate::events::Kind::Started { seed: 12345 }
        ));
    }

    #[test]
    fn restart_replays_the_seed_and_resumes_atomically() {
        let (mut player, commands, _, retired) = player();
        let beginning: Vec<_> = (0..4800).map(|_| player.next()).collect();
        commands.send(Command::Pause(true)).unwrap();
        player.begin_buffer();
        commands
            .send(Command::Replace {
                generation: 1,
                paused: false,
                world: presets::build("mountain", 48_000.0, 12345).unwrap().into(),
            })
            .unwrap();
        player.begin_buffer();
        drop(retired.try_recv().unwrap());
        let replay: Vec<_> = (0..4800).map(|_| player.next()).collect();
        assert_eq!(beginning, replay);
        assert_eq!(player.generation, 1);
    }
}
