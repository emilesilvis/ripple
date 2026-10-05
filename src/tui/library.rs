//! Browse persistent worlds while independent bounded workers scout and prepare
//! playback. Neither file I/O nor candidate analysis runs in the audio callback.
use super::*;
use crate::alien::atlas::{Entry, Library, Progress};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone, Copy, Default)]
enum Filter {
    #[default]
    Atlas,
    Favourites,
    All,
}

struct Browser {
    library: Library,
    cursor: usize,
    filter: Filter,
    playing: Option<String>,
    loading: Option<String>,
    generation: u64,
    pending: Option<(u64, String, Entry)>,
    building: bool,
    paused: bool,
    volume: f32,
    snapshot: Snapshot,
    events: EventView,
    visual: Visualization,
    progress: Progress,
    searching: bool,
    local_search: bool,
    local_best: Option<Entry>,
    notice: String,
}

impl Browser {
    fn ids(&self) -> Vec<String> {
        match self.filter {
            Filter::Atlas => self.library.champions(),
            Filter::Favourites => self.library.favourites.iter().cloned().collect(),
            Filter::All => self.library.entries.keys().cloned().collect(),
        }
    }
    fn selected(&self) -> Option<String> {
        self.ids().get(self.cursor).cloned()
    }
    fn request(&mut self, id: String) {
        if let Some(entry) = self.library.entries.get(&id) {
            self.generation += 1;
            self.pending = Some((self.generation, id.clone(), entry.clone()));
            self.loading = Some(id);
        }
    }
    fn receive(&mut self, entry: Entry) {
        let selected = self.selected();
        if self.local_search
            && self
                .local_best
                .as_ref()
                .is_none_or(|b| entry.character.score() < b.character.score())
        {
            self.local_best = Some(entry.clone());
        }
        let id = entry.id();
        self.library.insert(entry);
        if let Some(index) = self
            .ids()
            .iter()
            .position(|id| Some(id) == selected.as_ref())
        {
            self.cursor = index;
        }
        self.cursor = self.cursor.min(self.ids().len().saturating_sub(1));
        if self.playing.is_none() && self.loading.is_none() {
            self.request(id);
        }
    }
    fn draw(&self) -> io::Result<()> {
        if self.visual.visible {
            return self.visual.draw(
                self.playing.as_deref().unwrap_or("preparing world"),
                self.paused,
                self.loading.is_some(),
                &self.events,
                &self.snapshot,
            );
        }
        if self.events.visible {
            return self.events.draw(
                self.playing.as_deref().unwrap_or("preparing world"),
                self.snapshot.seconds,
                self.paused,
            );
        }
        let (width, height) = terminal::size()?;
        let mut stdout = io::stdout().lock();
        for (row, (color, line)) in self
            .lines(width, height)
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

    fn lines(&self, width: u16, height: u16) -> Vec<(Color, String)> {
        let ids = self.ids();
        let filter = match self.filter {
            Filter::Atlas => "Atlas",
            Filter::Favourites => "Favourites",
            Filter::All => "All saved",
        };
        let mut lines = vec![
            (
                Color::Cyan,
                " ripple / alien world library | v visualize | l log".to_owned(),
            ),
            (
                Color::White,
                format!(
                    " {filter} | {} regions | {} saved | {} favourites",
                    self.library.occupied(),
                    self.library.entries.len(),
                    self.library.favourites.len()
                ),
            ),
            (
                Color::White,
                format!(
                    " {} {} | {:.1}s | volume {:.0}%",
                    if self.paused { "Paused" } else { "Playing" },
                    self.playing.as_deref().unwrap_or("preparing first world"),
                    self.snapshot.seconds,
                    self.volume * 100.0
                ),
            ),
            (
                Color::Yellow,
                if self.searching {
                    format!(
                        " Exploring {}/{} | {} kept | {} rejected | x cancel",
                        self.progress.done,
                        self.progress.total,
                        self.progress.accepted,
                        self.progress.rejected
                    )
                } else if let Some(id) = &self.loading {
                    format!(" Preparing {id}... current audio continues")
                } else {
                    format!(" {}", self.notice)
                },
            ),
        ];
        let activity = self
            .events
            .activity_lines(self.snapshot.seconds, None, width, 5);
        let rows = (height as usize).saturating_sub(10 + activity.len()).max(1);
        lines.extend(activity);
        let start = self
            .cursor
            .saturating_sub(rows / 2)
            .min(ids.len().saturating_sub(rows));
        if ids.is_empty() {
            lines.push((
                Color::White,
                match self.filter {
                    Filter::Favourites => {
                        " No favourites yet. Tab returns to the atlas; f stars a world."
                    }
                    _ => " No worlds yet. Press g to explore.",
                }
                .into(),
            ));
        }
        for (i, id) in ids.iter().enumerate().skip(start).take(rows) {
            let entry = &self.library.entries[id];
            lines.push((
                if i == self.cursor {
                    Color::Cyan
                } else {
                    Color::White
                },
                format!(
                    " {} {} {} {}",
                    if i == self.cursor { ">" } else { " " },
                    if self.library.favourites.contains(id) {
                        "*"
                    } else {
                        " "
                    },
                    &id[..8],
                    entry.character.label()
                ),
            ));
        }
        if let Some(id) = self.selected() {
            let entry = &self.library.entries[&id];
            let c = &entry.character;
            let (bodies, inhabitants) = entry.spec.counts();
            lines.push((
                Color::DarkGrey,
                format!(
                    " Seed {} | {bodies} bodies, {inhabitants} inhabitants | parent {}",
                    entry.spec.seed,
                    entry.parent.as_deref().map(|s| &s[..8]).unwrap_or("none")
                ),
            ));
            lines.push((
                Color::DarkGrey,
                format!(
                    " {:.0} Hz | texture {:.2} | motion {:.2} | width {:.2}",
                    c.brightness, c.texture, c.motion, c.width
                ),
            ));
        }
        lines.push((
            Color::DarkGrey,
            " Arrows/j/k browse | Enter listen | f favourite | Tab filter".into(),
        ));
        lines.push((
            Color::DarkGrey,
            " g explore | e nearby selected | p parent | x stop search".into(),
        ));
        lines.push((
            Color::DarkGrey,
            " Space pause | +/- volume | r replay | q quit".into(),
        ));
        lines.push((
            Color::DarkGrey,
            " Accepted worlds save automatically. Favourites are kept.".into(),
        ));
        // Keep controls available in narrow terminals; no hidden modal dialog.
        if width < 60 || height < 16 {
            lines = vec![
                (
                    Color::Cyan,
                    " ripple / alien library | v visualize | l log".into(),
                ),
                (
                    Color::White,
                    if self.searching {
                        format!(
                            " Exploring {}/{} | x stop",
                            self.progress.done, self.progress.total
                        )
                    } else {
                        format!(
                            " {filter}: {} | {} saved",
                            ids.len(),
                            self.library.entries.len()
                        )
                    },
                ),
                (
                    Color::White,
                    format!(
                        " {} {:.1}s",
                        if self.paused { "paused" } else { "playing" },
                        self.snapshot.seconds
                    ),
                ),
                self.events
                    .activity_lines(self.snapshot.seconds, None, width, 1)
                    .into_iter()
                    .next()
                    .unwrap(),
                (
                    Color::White,
                    self.selected()
                        .map(|id| format!(" > {} | r replay", &id[..8]))
                        .unwrap_or_else(|| " Empty list".into()),
                ),
                (Color::DarkGrey, " j/k | Enter play | f star".into()),
                (Color::DarkGrey, " Tab view | g more | e nearby".into()),
                (Color::DarkGrey, " Space pause | +/- | q quit".into()),
            ];
        }
        lines
    }
}

enum SearchMessage {
    Update(Progress, Option<Entry>),
    Done(std::result::Result<Progress, String>),
}

fn start_search(
    browser: &mut Browser,
    seed: u32,
    parent: Option<Entry>,
    cancel: &Arc<AtomicBool>,
    sender: &mpsc::Sender<SearchMessage>,
) {
    let mut library = browser.library.clone();
    let cancel = Arc::clone(cancel);
    cancel.store(false, Ordering::Relaxed);
    let sender = sender.clone();
    let count = if parent.is_some() { 12 } else { 24 };
    browser.searching = true;
    browser.local_search = parent.is_some();
    browser.local_best = None;
    browser.progress = Progress {
        total: count,
        ..Progress::default()
    };
    std::thread::spawn(move || {
        let result = library
            .scout(seed, count, parent, &cancel, |progress, entry| {
                sender.send(SearchMessage::Update(progress, entry)).is_ok()
            })
            .map_err(|e| e.to_string());
        let _ = sender.send(SearchMessage::Done(result));
    });
}

pub fn run(directory: &Path, seed: u32) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "the library needs an interactive terminal; use `ripple scout` for offline discovery"
                .into(),
        );
    }
    let library = Library::open(directory)?;
    let warning = if library.warnings.is_empty() {
        "Listen, explore, and star your favourites.".into()
    } else {
        format!(
            "Skipped {} unreadable world files; `scout` reports details.",
            library.warnings.len()
        )
    };
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no default audio output device found")?;
    let supported = device.default_output_config()?;
    let config: cpal::StreamConfig = supported.clone().into();
    let sr = config.sample_rate.0;
    if !(16_000..=192_000).contains(&sr) {
        return Err("alien worlds need a sample rate from 16000 to 192000 Hz".into());
    }
    let (commands_tx, commands_rx) = mpsc::sync_channel(32);
    let (snapshots_tx, snapshots_rx) = mpsc::sync_channel(4);
    let (events_tx, events_rx) = mpsc::sync_channel(16);
    let (retired_tx, retired_rx) = mpsc::sync_channel(4);
    let (errors_tx, errors_rx) = mpsc::sync_channel(1);
    let visual = Visualization::default();
    let player = Player::new(
        Source::Silence,
        sr as f32,
        commands_rx,
        snapshots_tx,
        retired_tx,
    )
    .observing(events_tx)
    .visualizing(visual.shared.clone());
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => make_stream::<f32>(&device, &config, player, errors_tx)?,
        cpal::SampleFormat::I16 => make_stream::<i16>(&device, &config, player, errors_tx)?,
        cpal::SampleFormat::U16 => make_stream::<u16>(&device, &config, player, errors_tx)?,
        other => return Err(format!("unsupported audio sample format: {other}").into()),
    };
    let _terminal = TerminalSession::enter()?;
    let mut browser = Browser {
        library,
        cursor: 0,
        filter: Filter::Atlas,
        playing: None,
        loading: None,
        generation: 0,
        pending: None,
        building: false,
        paused: false,
        volume: 0.7,
        snapshot: Snapshot::default(),
        events: EventView::default(),
        visual,
        progress: Progress::default(),
        searching: false,
        local_search: false,
        local_best: None,
        notice: warning,
    };
    let (search_tx, search_rx) = mpsc::channel();
    let (loaded_tx, loaded_rx) =
        mpsc::channel::<(u64, String, std::result::Result<Source, String>)>();
    let cancel = Arc::new(AtomicBool::new(false));
    // Cancels background work on errors as well as on an ordinary quit.
    struct CancelOnDrop(Arc<AtomicBool>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let _cancel_on_drop = CancelOnDrop(Arc::clone(&cancel));
    let mut search_seed = seed;
    if let Some(id) = browser.selected() {
        browser.request(id);
    } else {
        start_search(&mut browser, search_seed, None, &cancel, &search_tx);
    }
    stream.play()?;
    let mut last_draw = Instant::now() - Duration::from_secs(1);
    loop {
        for old in retired_rx.try_iter() {
            drop(old);
        }
        if let Ok(error) = errors_rx.try_recv() {
            return Err(format!("audio output failed: {error}").into());
        }
        for message in search_rx.try_iter() {
            match message {
                SearchMessage::Update(progress, entry) => {
                    browser.progress = progress;
                    if let Some(e) = entry {
                        browser.receive(e);
                    }
                }
                SearchMessage::Done(result) => {
                    browser.searching = false;
                    browser.notice = match result {
                        Ok(progress) => {
                            let text = format!(
                                "Explored {} worlds; {} kept across {} regions.{}",
                                progress.done,
                                progress.accepted,
                                progress.occupied,
                                progress
                                    .last_error
                                    .as_ref()
                                    .map(|e| format!(" Last rejection: {e}"))
                                    .unwrap_or_default()
                            );
                            browser.progress = progress;
                            text
                        }
                        Err(error) => format!("Search stopped: {error}"),
                    };
                    if let Some(entry) = browser.local_best.take() {
                        browser.filter = Filter::All;
                        let id = entry.id();
                        browser.cursor = browser.ids().iter().position(|v| *v == id).unwrap_or(0);
                        browser.request(id);
                    }
                }
            }
        }
        for (generation, id, result) in loaded_rx.try_iter() {
            browser.building = false;
            if generation != browser.generation {
                continue;
            }
            browser.loading = None;
            match result {
                Ok(world) => {
                    commands_tx.send(Command::Replace {
                        generation,
                        world,
                        paused: browser.paused,
                    })?;
                    browser.playing = Some(id);
                    browser.snapshot = Snapshot {
                        generation,
                        ..Snapshot::default()
                    };
                }
                Err(error) => browser.notice = format!("Could not load world: {error}"),
            }
        }
        if !browser.building {
            if let Some((generation, id, entry)) = browser.pending.take() {
                browser.building = true;
                let sender = loaded_tx.clone();
                std::thread::spawn(move || {
                    let result = entry
                        .play(sr as f32)
                        .map(|world| Source::Discovered(Box::new(world)));
                    let _ = sender.send((generation, id, result));
                });
            }
        }
        for snapshot in snapshots_rx.try_iter() {
            if snapshot.generation == browser.snapshot.generation {
                if snapshot.invalid_samples > 0 {
                    browser.paused = true;
                    browser.notice = "Invalid audio; playback paused.".into();
                }
                browser.snapshot = snapshot;
            }
        }
        browser.events.set_generation(browser.snapshot.generation);
        for batch in events_rx.try_iter() {
            browser.events.receive(batch, browser.snapshot.generation);
        }
        if last_draw.elapsed() >= Duration::from_millis(100) {
            browser.visual.refresh(browser.snapshot.generation);
            browser.draw()?;
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
                if matches!(code, KeyCode::Esc | KeyCode::Char('q'))
                    || (code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }
                if browser.visual.handle_key(code, &mut browser.events.visible)
                    || browser.events.handle_key(code)
                {
                    last_draw = Instant::now() - Duration::from_secs(1);
                    continue;
                }
                let len = browser.ids().len().max(1);
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        browser.cursor = (browser.cursor + len - 1) % len
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        browser.cursor = (browser.cursor + 1) % len
                    }
                    KeyCode::Enter => {
                        if let Some(id) = browser.selected() {
                            browser.request(id);
                            browser.visual.visible = true;
                        }
                    }
                    KeyCode::Tab => {
                        browser.filter = match browser.filter {
                            Filter::Atlas => Filter::Favourites,
                            Filter::Favourites => Filter::All,
                            Filter::All => Filter::Atlas,
                        };
                        browser.cursor = 0;
                    }
                    KeyCode::Char('f') => {
                        if let Some(id) = browser.selected() {
                            browser.notice = match browser.library.toggle_favourite(&id) {
                                Ok(true) => "Favourite saved.".into(),
                                Ok(false) => "Favourite removed; the world is still saved.".into(),
                                Err(e) => format!("Favourite failed: {e}"),
                            };
                            browser.cursor =
                                browser.cursor.min(browser.ids().len().saturating_sub(1));
                        }
                    }
                    KeyCode::Char('g' | 'e') if !browser.searching => {
                        let parent = if code == KeyCode::Char('e') {
                            browser
                                .selected()
                                .and_then(|id| browser.library.entries.get(&id).cloned())
                        } else {
                            None
                        };
                        if code == KeyCode::Char('e') && parent.is_none() {
                            browser.notice =
                                "Highlight a world before exploring its neighbours.".into();
                        } else {
                            search_seed = search_seed.wrapping_add(0x9e3779b9);
                            start_search(&mut browser, search_seed, parent, &cancel, &search_tx);
                        }
                    }
                    KeyCode::Char('x') => {
                        cancel.store(true, Ordering::Relaxed);
                        browser.notice = "Stopping search; accepted worlds are saved.".into();
                    }
                    KeyCode::Char('p') => {
                        if let Some(parent) = browser
                            .selected()
                            .and_then(|id| browser.library.entries[&id].parent.clone())
                        {
                            if browser.library.entries.contains_key(&parent) {
                                browser.filter = Filter::All;
                                browser.cursor = browser
                                    .ids()
                                    .iter()
                                    .position(|id| *id == parent)
                                    .unwrap_or(0);
                                browser.request(parent);
                            } else {
                                browser.notice = "Parent is not in this library.".into();
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        if let Some(id) = browser.playing.clone() {
                            browser.request(id);
                        }
                    }
                    KeyCode::Char(' ') => {
                        browser.paused = !browser.paused;
                        commands_tx.send(Command::Pause(browser.paused))?;
                    }
                    KeyCode::Char('+' | '=') => {
                        browser.volume = (browser.volume + 0.1).min(2.0);
                        commands_tx.send(Command::Volume(browser.volume))?;
                    }
                    KeyCode::Char('-' | '_') => {
                        browser.volume = (browser.volume - 0.1).max(0.0);
                        commands_tx.send(Command::Volume(browser.volume))?;
                    }
                    _ => {}
                }
                last_draw = Instant::now() - Duration::from_secs(1);
            }
            Event::Resize(_, _) => last_draw = Instant::now() - Duration::from_secs(1),
            _ => {}
        }
    }
    cancel.store(true, Ordering::Relaxed);
    drop(stream);
    Ok(())
}
