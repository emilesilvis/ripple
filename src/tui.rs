//! Live world selection. The audio callback owns the world; construction and
//! disposal happen on the UI side so changing scenes cannot block playback.

use crate::{presets, world::World};
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

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

enum Command {
    Replace {
        generation: u64,
        world: World,
        paused: bool,
    },
    Volume(f32),
    Pause(bool),
}

#[derive(Clone, Copy, Default)]
struct Snapshot {
    generation: u64,
    seconds: f64,
    peak: f32,
    rms: f64,
    invalid_samples: u64,
    clipped_samples: u64,
}

struct Player {
    world: World,
    sr: f32,
    commands: Receiver<Command>,
    snapshots: SyncSender<Snapshot>,
    retired: SyncSender<World>,
    deferred_retirement: Option<World>,
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
        world: World,
        sr: f32,
        commands: Receiver<Command>,
        snapshots: SyncSender<Snapshot>,
        retired: SyncSender<World>,
    ) -> Self {
        Self {
            world,
            sr,
            commands,
            snapshots,
            retired,
            deferred_retirement: None,
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
                    world,
                    paused,
                } => {
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
            let _ = self.snapshots.try_send(Snapshot {
                generation: self.generation,
                seconds: self.elapsed_samples as f64 / self.sr as f64,
                peak: self.peak,
                rms: (self.squares / self.meter_samples as f64).sqrt(),
                invalid_samples: self.invalid_samples,
                clipped_samples: self.clipped_samples,
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
    pending_load: Option<(u64, usize, u32)>,
    building: bool,
    snapshot: Snapshot,
}

impl Ui {
    fn request_world(&mut self, selection: usize) {
        self.generation += 1;
        self.loading = Some(selection);
        self.pending_load = Some((self.generation, selection, self.seed));
        self.paused = false;
    }

    fn lines(&self, device: &str, sr: u32) -> Vec<(Color, String)> {
        let mut lines = vec![
            (Color::Cyan, " ripple / a world you can hear".into()),
            (Color::DarkGrey, format!(" {device} | {sr} Hz")),
            (Color::White, String::new()),
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
        lines.push((Color::White, String::new()));
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
                    presets::PRESETS[self.playing].name,
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
        lines.push((Color::White, String::new()));
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
        let (width, height) = terminal::size()?;
        let mut stdout = io::stdout().lock();
        let lines = if width < 54 || height < 20 {
            vec![
                (Color::Cyan, " ripple / a world you can hear".into()),
                (
                    Color::White,
                    format!(
                        " {} | {} | {:.1}s",
                        presets::PRESETS[self.playing].name,
                        if self.paused { "paused" } else { "playing" },
                        self.snapshot.seconds
                    ),
                ),
                (
                    Color::White,
                    " Resize to at least 54 columns x 20 rows for details.".into(),
                ),
                (
                    Color::DarkGrey,
                    " Arrows select | Enter play | Space pause".into(),
                ),
                (
                    Color::DarkGrey,
                    " +/- volume | r restart | n seed | q quit".into(),
                ),
            ]
        } else {
            self.lines(device, sr)
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
    println!("Preparing {name}...");
    let world = presets::build(name, sr as f32, seed).unwrap();
    let (commands_tx, commands_rx) = mpsc::sync_channel(32);
    let (snapshots_tx, snapshots_rx) = mpsc::sync_channel(4);
    let (retired_tx, retired_rx) = mpsc::sync_channel(4);
    let (errors_tx, errors_rx) = mpsc::sync_channel(1);
    let (loaded_tx, loaded_rx) = mpsc::channel();
    let player = Player::new(world, sr as f32, commands_rx, snapshots_tx, retired_tx);
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
        snapshot: Snapshot::default(),
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
        for (generation, selection, world) in loaded_rx.try_iter() {
            ui.building = false;
            if generation == ui.generation {
                commands_tx.send(Command::Replace {
                    generation,
                    world,
                    paused: ui.paused,
                })?;
                ui.playing = selection;
                ui.loading = None;
                ui.snapshot = Snapshot {
                    generation,
                    ..Snapshot::default()
                };
            }
        }
        // At most one build runs. Rapid selection changes overwrite one pending
        // request rather than spawning competing terrain simulations.
        if !ui.building {
            if let Some((generation, selection, seed)) = ui.pending_load.take() {
                ui.building = true;
                let sender = loaded_tx.clone();
                std::thread::spawn(move || {
                    let world =
                        presets::build(presets::PRESETS[selection].name, sr as f32, seed).unwrap();
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
        if last_draw.elapsed() >= Duration::from_millis(100) {
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
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        ui.cursor =
                            (ui.cursor + presets::PRESETS.len() - 1) % presets::PRESETS.len()
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        ui.cursor = (ui.cursor + 1) % presets::PRESETS.len()
                    }
                    KeyCode::Enter => ui.request_world(ui.cursor),
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

    fn player() -> (
        Player,
        SyncSender<Command>,
        Receiver<Snapshot>,
        Receiver<World>,
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
    fn restart_replays_the_seed_and_resumes_atomically() {
        let (mut player, commands, _, retired) = player();
        let beginning: Vec<_> = (0..4800).map(|_| player.next()).collect();
        commands.send(Command::Pause(true)).unwrap();
        player.begin_buffer();
        commands
            .send(Command::Replace {
                generation: 1,
                paused: false,
                world: presets::build("mountain", 48_000.0, 12345).unwrap(),
            })
            .unwrap();
        player.begin_buffer();
        drop(retired.try_recv().unwrap());
        let replay: Vec<_> = (0..4800).map(|_| player.next()).collect();
        assert_eq!(beginning, replay);
        assert_eq!(player.generation, 1);
    }
}
