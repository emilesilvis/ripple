//! One automatic composition, drawn only from the playing simulation.
use super::*;
use crate::{
    events::Kind,
    field::Surface,
    visual::{Frame, Shared},
};
use std::sync::{Arc, Mutex};

pub(super) struct Visualization {
    pub visible: bool,
    pub shared: Shared,
    frame: Option<Frame>,
}

impl Default for Visualization {
    fn default() -> Self {
        Self {
            visible: true,
            shared: Arc::new(Mutex::new(Frame::default())),
            frame: None,
        }
    }
}

impl Visualization {
    pub fn refresh(&mut self, generation: u64) {
        if self
            .frame
            .as_ref()
            .is_some_and(|f| f.generation != generation)
        {
            self.frame = None;
        }
        if let Ok(frame) = self.shared.try_lock() {
            if frame.generation == generation {
                self.frame = Some(frame.clone());
            }
        }
    }

    pub fn handle_key(&mut self, code: KeyCode, log: &mut bool) -> bool {
        match code {
            KeyCode::Char('v') => {
                self.visible = !self.visible;
                *log = false;
                true
            }
            KeyCode::Char('l') => {
                *log = !*log;
                self.visible = !*log;
                true
            }
            // Browsing is always visible; never change a hidden selection.
            KeyCode::Up | KeyCode::Down | KeyCode::Char('j' | 'k') if self.visible => {
                self.visible = false;
                false
            }
            // Tab belongs to the library when browsing, not to the picture.
            KeyCode::Tab | KeyCode::BackTab if self.visible => true,
            _ => false,
        }
    }

    pub fn draw(
        &self,
        title: &str,
        paused: bool,
        loading: bool,
        events: &EventView,
        audio: &Snapshot,
    ) -> io::Result<()> {
        let (w, h) = terminal::size()?;
        let mut canvas = self.render(title, paused, loading, events, w, h);
        let warning = if audio.invalid_samples > 0 {
            Some("Audio stopped. Press r to restart.")
        } else if audio.clipped_samples > 0 {
            Some("Volume is too high. Press - to lower it.")
        } else {
            None
        };
        if let Some(warning) = warning {
            let width = canvas.w;
            canvas.text(
                0,
                canvas.h.saturating_sub(2),
                Color::Red,
                &format!("{warning:<width$}"),
            );
        }
        let mut out = io::stdout().lock();
        for y in 0..canvas.h {
            queue!(
                out,
                MoveTo(0, y as u16),
                ResetColor,
                Clear(ClearType::CurrentLine)
            )?;
            let mut color = Color::Reset;
            for x in 0..canvas.w {
                let pixel = canvas.cells[y * canvas.w + x];
                if pixel.1 != color {
                    color = pixel.1;
                    queue!(out, SetForegroundColor(color))?;
                }
                queue!(out, Print(pixel.0))?;
            }
        }
        queue!(out, ResetColor, Clear(ClearType::FromCursorDown))?;
        out.flush()
    }

    fn render(
        &self,
        title: &str,
        paused: bool,
        loading: bool,
        events: &EventView,
        w: u16,
        h: u16,
    ) -> Canvas {
        let mut c = Canvas::new(w.saturating_sub(1).min(240) as usize, h.min(100) as usize);
        c.text(
            1,
            0,
            Color::Cyan,
            &format!("ripple / {title}{}", if paused { " / paused" } else { "" }),
        );
        let footer = c.h.saturating_sub(1);
        c.text(
            1,
            footer,
            Color::DarkGrey,
            if c.w >= 54 {
                "Space pause   +/- volume   v browse   l log   q quit"
            } else {
                "Space pause  v browse  q quit"
            },
        );
        // Small terminals still get a picture, without a configuration screen.
        if c.w < 8 || c.h < 5 {
            return c;
        }
        let Some(f) = self.frame.as_ref().filter(|f| f.ready) else {
            c.text(1, 2, Color::DarkGrey, "Preparing your soundscape...");
            return c;
        };
        let wind_rows = usize::from(f.air_sound && f.candidate.is_none()) * 2;
        let area = Rect {
            x: 1,
            y: 2,
            w: c.w.saturating_sub(2),
            h: c.h.saturating_sub(5 + wind_rows).max(1),
        };
        if f.candidate.is_some() {
            resonators(&mut c, area, f);
        } else {
            let parts = parts(f);
            for (part, rect) in parts.iter().zip(layout(area, parts.len())) {
                let label = match part {
                    Part::Water => {
                        if f.rain_sound {
                            "Water & rain"
                        } else {
                            "Water"
                        }
                    }
                    Part::Fire => "Fire",
                    Part::Chimes => "Chimes",
                    Part::Calls => "Calls",
                };
                c.text(rect.x, rect.y, Color::DarkGrey, label);
                let body = Rect {
                    y: rect.y + 1,
                    h: rect.h.saturating_sub(1),
                    ..rect
                };
                if body.h == 0 || body.w == 0 {
                    continue;
                }
                match part {
                    Part::Water => water(&mut c, body, f, events),
                    Part::Fire => fire(&mut c, body, f, events),
                    Part::Chimes => chimes(&mut c, body, f),
                    Part::Calls => calls(&mut c, body, f, events),
                }
            }
            if f.air_sound {
                let y = c.h.saturating_sub(4);
                wind(
                    &mut c,
                    Rect {
                        x: 1,
                        y,
                        w: area.w,
                        h: 1,
                    },
                    f.air,
                );
            } else if parts.is_empty() {
                c.text(1, 3, Color::DarkGrey, "Quiet");
            }
        }
        let note = if loading {
            "Preparing the next soundscape..."
        } else if f.omitted() > 0 {
            "Some activity cannot fit in this view."
        } else if events.lost() > 0 {
            "Some brief events were missed."
        } else {
            ""
        };
        c.text(1, c.h.saturating_sub(2), Color::DarkGrey, note);
        c
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Water,
    Fire,
    Chimes,
    Calls,
}
fn parts(f: &Frame) -> Vec<Part> {
    let mut parts = Vec::new();
    // Choose from enabled sound sources, not preset names or transient peaks.
    if f.water_sound || f.rain_sound {
        parts.push(Part::Water);
    }
    if f.fire.is_some() {
        parts.push(Part::Fire);
    }
    if !f.bodies.values.is_empty() {
        parts.push(Part::Chimes);
    }
    if !f.populations.values.is_empty() {
        parts.push(Part::Calls);
    }
    parts
}

#[derive(Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}
fn layout(r: Rect, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![r];
    }
    if r.w < 44 {
        return (0..count)
            .map(|i| Rect {
                y: r.y + i * r.h / count,
                h: ((i + 1) * r.h / count).saturating_sub(i * r.h / count),
                ..r
            })
            .collect();
    }
    let left = (r.w - 2) / 2;
    let right = Rect {
        x: r.x + left + 2,
        w: r.w - left - 2,
        ..r
    };
    let mut out = vec![Rect { w: left, ..r }];
    out.extend((0..count - 1).map(|i| Rect {
        y: r.y + i * r.h / (count - 1),
        h: ((i + 1) * r.h / (count - 1)).saturating_sub(i * r.h / (count - 1)),
        ..right
    }));
    out
}

fn water(c: &mut Canvas, r: Rect, f: &Frame, events: &EventView) {
    if f.width == 0 || f.height == 0 || f.cells.values.len() != f.width * f.height {
        return;
    }
    let low = f
        .cells
        .values
        .iter()
        .map(|v| v.bed_m)
        .fold(f32::INFINITY, f32::min);
    let high = f
        .cells
        .values
        .iter()
        .map(|v| v.bed_m)
        .fold(f32::NEG_INFINITY, f32::max);
    let plot = Plot::in_rect(
        r,
        [
            0.0,
            f.width as f64 * f.cell_m as f64,
            0.0,
            f.height as f64 * f.cell_m as f64,
        ],
    );
    for y in 0..plot.h {
        for x in 0..plot.w {
            let (x0, x1) = sample_range(x, plot.w, f.width);
            let (y0, y1) = sample_range(y, plot.h, f.height);
            let (mut depth, mut bed, mut flux, mut roofs) = (0.0, 0.0, [0.0; 2], 0);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let cell = &f.cells.values[sy * f.width + sx];
                    depth += cell.depth_m;
                    bed += cell.bed_m;
                    flux[0] += cell.flux[0];
                    flux[1] += cell.flux[1];
                    roofs += usize::from(cell.surface == Surface::Roof);
                }
            }
            let n = ((x1 - x0) * (y1 - y0)) as f32;
            let pixel = if roofs as f32 > n * 0.5 {
                ('─', Color::DarkGrey)
            } else if depth / n < 1e-4 {
                let shade = ((bed / n - low) / (high - low).max(1e-6)).clamp(0.0, 1.0);
                ('·', Color::AnsiValue(236 + (shade * 7.0) as u8))
            } else {
                let mut pixel = depth_pixel(depth / n);
                if x % 3 == 0 && y % 2 == 0 && flux[0].hypot(flux[1]) / n > 1e-7 {
                    pixel.0 = if flux[0].abs() > flux[1].abs() {
                        if flux[0] > 0.0 {
                            '›'
                        } else {
                            '‹'
                        }
                    } else if flux[1] > 0.0 {
                        '↓'
                    } else {
                        '↑'
                    };
                }
                pixel
            };
            c.put(plot.x + x, plot.y + y, pixel);
        }
    }
    for record in events.recent(f.generation, f.seconds) {
        if let Kind::RainImpact { x, y, .. } = record.event {
            plot.mark(
                c,
                [
                    x as f64 * f.width as f64 * f.cell_m as f64,
                    y as f64 * f.height as f64 * f.cell_m as f64,
                ],
                ('+', Color::White),
            );
        }
    }
}

fn chimes(c: &mut Canvas, r: Rect, f: &Frame) {
    let plot = Plot::in_rect(r, [-0.16, 0.16, -0.16, 0.16]);
    for (i, b) in f.bodies.values.iter().enumerate() {
        let color = if b.touching {
            Color::Yellow
        } else if i == 0 {
            Color::White
        } else {
            Color::Cyan
        };
        plot.mark(c, b.origin_m, ('·', Color::DarkGrey));
        for y in 0..plot.h {
            for x in 0..plot.w {
                let p = plot.world(x, y);
                if (p[0] - b.position_m[0]).hypot(p[1] - b.position_m[1]) <= b.radius_m {
                    c.put(plot.x + x, plot.y + y, ('●', color));
                }
            }
        }
        plot.mark(c, b.position_m, (if i == 0 { '●' } else { 'o' }, color));
    }
}

fn calls(c: &mut Canvas, r: Rect, f: &Frame, events: &EventView) {
    // Each population keeps its own local space. All appear together, divided
    // into small vignettes; selecting a population is never necessary.
    let count = f.populations.values.len();
    if count == 0 {
        return;
    }
    let cols = count.min((r.w / 12).max(1));
    let rows = count.div_ceil(cols);
    for (group, p) in f.populations.values.iter().enumerate() {
        if p.gain <= 0.0 {
            continue;
        }
        let col = group % cols;
        let row = group / cols;
        let x0 = col * r.w / cols;
        let x1 = (col + 1) * r.w / cols;
        let y0 = row * r.h / rows;
        let y1 = (row + 1) * r.h / rows;
        if x1 <= x0 + 1 || y1 <= y0 {
            continue;
        }
        let tile = Rect {
            x: r.x + x0,
            y: r.y + y0,
            w: x1 - x0 - 1,
            h: y1 - y0,
        };
        if col > 0 {
            for y in tile.y..tile.y + tile.h {
                c.put(tile.x - 1, y, ('│', Color::DarkGrey));
            }
        }
        if row > 0 {
            for x in tile.x..tile.x + tile.w {
                c.put(x, tile.y.saturating_sub(1), ('─', Color::DarkGrey));
            }
        }
        let animals: Vec<_> = f
            .animals
            .values
            .iter()
            .filter(|a| a.population == group)
            .collect();
        let mut bounds = [
            p.listener.x as f64,
            p.listener.x as f64,
            p.listener.y as f64,
            p.listener.y as f64,
        ];
        for a in &animals {
            bounds[0] = bounds[0].min(a.position.x as f64);
            bounds[1] = bounds[1].max(a.position.x as f64);
            bounds[2] = bounds[2].min(a.position.y as f64);
            bounds[3] = bounds[3].max(a.position.y as f64);
        }
        bounds[0] -= 1.0;
        bounds[1] += 1.0;
        bounds[2] -= 1.0;
        bounds[3] += 1.0;
        let plot = Plot::in_rect(tile, bounds);
        if let Some(b) = p.barrier {
            plot.line(
                c,
                [b.x as f64, b.y_min as f64],
                [b.x as f64, b.y_max as f64],
                ('│', Color::DarkGrey),
            );
        }
        for record in events.recent(f.generation, f.seconds) {
            if let Kind::CallHeard {
                population,
                caller,
                receiver,
                ..
            } = record.event
            {
                if population != group {
                    continue;
                }
                if let (Some(a), Some(b)) = (
                    animals.iter().find(|a| a.index == caller),
                    animals.iter().find(|a| a.index == receiver),
                ) {
                    plot.line(
                        c,
                        [a.position.x as f64, a.position.y as f64],
                        [b.position.x as f64, b.position.y as f64],
                        ('·', Color::Green),
                    );
                }
            }
        }
        for a in animals {
            plot.mark(
                c,
                [a.position.x as f64, a.position.y as f64],
                if a.calling {
                    ('*', Color::Yellow)
                } else {
                    ('o', Color::Cyan)
                },
            );
        }
    }
}

fn fire(c: &mut Canvas, r: Rect, f: &Frame, events: &EventView) {
    let activity = f.fire.unwrap_or(0.0).clamp(0.0, 1.0) as f64;
    let pop = events
        .recent(f.generation, f.seconds)
        .any(|e| matches!(e.event, Kind::FirePop { .. }));
    // A glow icon encodes the lumped heat driver. It is not an invented flame
    // field: no wind particles, random motion, or independent animation clock.
    let size = (r.w as f64 / 2.0).min(r.h as f64).max(1.0);
    for y in 0..r.h {
        for x in 0..r.w {
            let dx = (x as f64 + 0.5 - r.w as f64 / 2.0) / (size * 0.9);
            let dy = (y as f64 + 0.5 - r.h as f64 / 2.0) / size;
            let glow = (1.0 - (dx * dx + dy * dy * 2.0).sqrt()).max(0.0) * activity;
            let pixel = if glow > 0.6 {
                ('●', if pop { Color::Yellow } else { Color::Red })
            } else if glow > 0.3 {
                ('o', if pop { Color::Yellow } else { Color::Red })
            } else if glow > 0.1 {
                ('·', Color::DarkRed)
            } else {
                continue;
            };
            c.put(r.x + x, r.y + y, pixel);
        }
    }
}

fn wind(c: &mut Canvas, r: Rect, air: f32) {
    c.text(r.x, r.y, Color::DarkGrey, "Wind");
    let n = r.w.saturating_sub(6);
    let filled = (air.clamp(0.0, 1.0) * n as f32).round() as usize;
    for i in 0..n {
        c.put(
            r.x + 6 + i,
            r.y,
            if i < filled {
                ('━', Color::Cyan)
            } else {
                ('─', Color::DarkGrey)
            },
        );
    }
}

fn resonators(c: &mut Canvas, r: Rect, f: &Frame) {
    c.text(
        r.x,
        r.y,
        Color::DarkGrey,
        if f.candidate == Some(0) {
            "Resonance / A"
        } else {
            "Resonance / B"
        },
    );
    if r.h < 2 {
        return;
    }
    // Display the actual A/B blend. Energy contributions use squared mix
    // weights; mode energy is not claimed to be the summed audio waveform.
    let low = f
        .resonances
        .values
        .iter()
        .map(|m| m.hz)
        .fold(f64::INFINITY, f64::min);
    let high = f
        .resonances
        .values
        .iter()
        .map(|m| m.hz)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut bins = vec![0.0; r.w];
    for mode in &f.resonances.values {
        let x = (((mode.hz - low) / (high - low).max(1.0)) * (r.w - 1) as f64) as usize;
        let weight = if mode.candidate == 0 {
            1.0 - f.mix
        } else {
            f.mix
        };
        bins[x.min(r.w - 1)] += mode.energy * weight * weight;
    }
    for (x, energy) in bins.into_iter().enumerate() {
        let height = ((energy / 0.025).clamp(0.0, 1.0).sqrt() * (r.h - 1) as f64).round() as usize;
        for y in 0..height {
            c.put(r.x + x, r.y + r.h - 1 - y, ('│', Color::Cyan));
        }
        if height == 0 {
            c.put(r.x + x, r.y + r.h - 1, ('·', Color::DarkGrey));
        }
    }
}

fn sample_range(pixel: usize, pixels: usize, cells: usize) -> (usize, usize) {
    let start = pixel * cells / pixels;
    (
        start,
        ((pixel + 1) * cells / pixels).max(start + 1).min(cells),
    )
}
fn depth_pixel(depth: f32) -> (char, Color) {
    let i = [1e-4, 0.001, 0.01, 0.05, 0.2, 0.5, 1.0]
        .iter()
        .take_while(|&&limit| depth >= limit)
        .count();
    (
        ['·', ',', ':', '~', '≈', '≋', '▓', '█'][i],
        if i == 0 {
            Color::DarkGrey
        } else {
            Color::AnsiValue([0, 24, 31, 37, 38, 39, 45, 51][i])
        },
    )
}
struct Canvas {
    w: usize,
    h: usize,
    cells: Vec<(char, Color)>,
}
impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            cells: vec![(' ', Color::Reset); w * h],
        }
    }
    fn put(&mut self, x: usize, y: usize, pixel: (char, Color)) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x] = pixel;
        }
    }
    fn text(&mut self, x: usize, y: usize, color: Color, text: &str) {
        for (i, ch) in text.chars().take(self.w.saturating_sub(x)).enumerate() {
            self.put(x + i, y, (ch, color));
        }
    }
}

struct Plot {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    bounds: [f64; 4],
}
impl Plot {
    fn in_rect(r: Rect, bounds: [f64; 4]) -> Self {
        let mut plot = Self::fit(r.w, r.h, r.y, bounds);
        plot.x += r.x;
        plot.y += r.h.saturating_sub(plot.h) / 2;
        plot
    }
    fn fit(w: usize, h: usize, y: usize, bounds: [f64; 4]) -> Self {
        let aspect = (bounds[1] - bounds[0]) / (bounds[3] - bounds[2]);
        let ph = (h as f64).min(w as f64 / (2.0 * aspect)).floor().max(1.0) as usize;
        let pw = (2.0 * ph as f64 * aspect).round().max(1.0).min(w as f64) as usize;
        Self {
            x: (w - pw) / 2,
            y,
            w: pw,
            h: ph,
            bounds,
        }
    }
    fn world(&self, x: usize, y: usize) -> [f64; 2] {
        [
            self.bounds[0] + (x as f64 + 0.5) / self.w as f64 * (self.bounds[1] - self.bounds[0]),
            self.bounds[2] + (y as f64 + 0.5) / self.h as f64 * (self.bounds[3] - self.bounds[2]),
        ]
    }
    fn mark(&self, c: &mut Canvas, p: [f64; 2], pixel: (char, Color)) -> bool {
        if p[0] < self.bounds[0]
            || p[0] > self.bounds[1]
            || p[1] < self.bounds[2]
            || p[1] > self.bounds[3]
        {
            return false;
        }
        let x =
            ((p[0] - self.bounds[0]) / (self.bounds[1] - self.bounds[0]) * self.w as f64) as usize;
        let y =
            ((p[1] - self.bounds[2]) / (self.bounds[3] - self.bounds[2]) * self.h as f64) as usize;
        c.put(
            self.x + x.min(self.w - 1),
            self.y + y.min(self.h - 1),
            pixel,
        );
        true
    }
    fn line(&self, c: &mut Canvas, a: [f64; 2], b: [f64; 2], pixel: (char, Color)) {
        let steps = self.w.max(self.h) * 2;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            self.mark(
                c,
                [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])],
                pixel,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plain(c: &Canvas) -> String {
        c.cells
            .chunks(c.w.max(1))
            .map(|row| row.iter().map(|p| p.0).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_soundscape_gets_an_automatic_composition_without_view_settings() {
        use Part::*;
        for (name, expected) in [
            ("glade", vec![Water, Chimes, Calls]),
            ("brook", vec![Water]),
            ("cozy-rain", vec![Water, Chimes]),
            ("night-meadow", vec![Calls]),
            ("shore", vec![Water, Calls]),
            ("hearth", vec![Fire, Calls]),
            ("mountain", vec![Calls]),
            ("storm", vec![Water]),
        ] {
            let world = presets::build(name, 16_000.0, 12345).unwrap();
            let mut view = Visualization::default();
            world.visualize(&mut view.shared.lock().unwrap());
            view.refresh(0);
            assert!(view.visible);
            assert_eq!(parts(view.frame.as_ref().unwrap()), expected, "{name}");
            // The source determines the picture even when the title is unrelated.
            for (w, h) in [(0, 0), (1, 1), (10, 5), (32, 12), (80, 24), (120, 40)] {
                let c = view.render("Now playing", false, false, &EventView::default(), w, h);
                assert_eq!(c.cells.len(), c.w * c.h);
                let text = plain(&c);
                for old in [
                    "Tab/",
                    "1 Water",
                    "population",
                    "m/cell",
                    "residual",
                    "snapshot",
                    "Enlarge",
                ] {
                    assert!(!text.contains(old), "{name}: {old}");
                }
            }
        }
        let spec = crate::alien::worlds::WorldSpec::random(12345);
        let world = crate::alien::worlds::Soundscape::new(&spec, 16_000.0, 0.3).unwrap();
        let mut f = Frame::default();
        world.visualize(&mut f);
        assert_eq!(parts(&f), vec![Water, Chimes, Calls]);
        // A quiet instant must not remove or rearrange systems that are present.
        f.water_m3 = 0.0;
        f.air = 0.0;
        f.rain = 0.0;
        for animal in &mut f.animals.values {
            animal.calling = false;
        }
        for population in &mut f.populations.values {
            population.gain = 0.0;
        }
        assert_eq!(parts(&f), vec![Water, Chimes, Calls]);
    }

    #[test]
    fn all_populations_are_visible_together_and_pause_does_not_animate() {
        let world = presets::build("night-meadow", 16_000.0, 12345).unwrap();
        let mut view = Visualization::default();
        world.visualize(&mut view.shared.lock().unwrap());
        view.refresh(0);
        let f = view.frame.as_mut().unwrap();
        assert_eq!(f.populations.values.len(), 3);
        let count = f.animals.values.len();
        // The owl is in the third population, previously hidden behind a selector.
        for animal in &mut f.animals.values {
            animal.calling = animal.population == 2;
        }
        let events = EventView::default();
        let image = view.render("night-meadow", true, false, &events, 120, 40);
        let picture = &image.cells[2 * image.w..(image.h - 5) * image.w];
        let callers = picture.iter().filter(|p| p.0 == '*').count();
        let resting = picture.iter().filter(|p| **p == ('o', Color::Cyan)).count();
        assert_eq!(callers, 1);
        assert_eq!(callers + resting, count);
        assert_eq!(
            image.cells,
            view.render("night-meadow", true, false, &events, 120, 40)
                .cells
        );
    }

    #[test]
    fn browsing_and_logs_return_to_the_automatic_scene_and_old_frames_are_rejected() {
        let mut view = Visualization::default();
        let mut log = false;
        for key in [
            KeyCode::Char('1'),
            KeyCode::Char('5'),
            KeyCode::Char('['),
            KeyCode::Char(']'),
            KeyCode::Tab,
        ] {
            view.handle_key(key, &mut log);
            assert!(view.visible);
        }
        assert!(view.handle_key(KeyCode::Char('v'), &mut log));
        assert!(!view.visible);
        assert!(view.handle_key(KeyCode::Char('v'), &mut log));
        assert!(view.visible);
        assert!(view.handle_key(KeyCode::Char('l'), &mut log));
        assert!(log && !view.visible);
        assert!(view.handle_key(KeyCode::Char('l'), &mut log));
        assert!(!log && view.visible);
        assert!(!view.handle_key(KeyCode::Down, &mut log));
        assert!(!view.visible);
        view.shared.lock().unwrap().ready = true;
        view.refresh(0);
        assert!(view.frame.is_some());
        view.refresh(1);
        assert!(view.frame.is_none());
    }

    #[test]
    fn rain_marks_still_use_actual_positions_and_simulation_time() {
        let mut f = Frame {
            ready: true,
            width: 4,
            height: 4,
            cell_m: 1.0,
            seconds: 2.0,
            water_sound: true,
            ..Frame::default()
        };
        for _ in 0..16 {
            f.cells.push(crate::visual::Cell {
                bed_m: 0.0,
                depth_m: 0.1,
                flux: [0.0; 2],
                surface: Surface::Open,
            });
        }
        let mut events = EventView::default();
        let mut batch = crate::events::Batch::new(0);
        for (time, x) in [(1.0, 0.0), (1.75, 0.5), (2.1, 1.0)] {
            batch.push(crate::events::Record {
                sequence: 0,
                sample: 0,
                time_seconds: time,
                event: Kind::RainImpact {
                    x,
                    y: 0.5,
                    surface: "open ground",
                    strength: 0.1,
                },
            });
        }
        events.receive(batch, 0);
        let r = Rect {
            x: 0,
            y: 0,
            w: 30,
            h: 15,
        };
        let mut c = Canvas::new(30, 15);
        water(&mut c, r, &f, &events);
        assert_eq!(c.cells.iter().filter(|p| p.0 == '+').count(), 1);
        assert_eq!(c.cells[7 * 30 + 15].0, '+');
        assert_eq!(events.recent(1, 2.0).count(), 0);
    }

    #[test]
    fn discovery_picture_uses_the_actual_crossfade_not_just_the_selected_candidate() {
        let mut f = Frame {
            candidate: Some(1),
            mix: 0.0,
            ..Frame::default()
        };
        f.resonances.push(crate::visual::Resonance {
            candidate: 0,
            hz: 200.0,
            energy: 0.025,
        });
        f.resonances.push(crate::visual::Resonance {
            candidate: 1,
            hz: 800.0,
            energy: 0.025,
        });
        let r = Rect {
            x: 0,
            y: 0,
            w: 30,
            h: 12,
        };
        let mut a = Canvas::new(30, 12);
        resonators(&mut a, r, &f);
        assert_eq!(a.cells[30].0, '│');
        assert_eq!(a.cells[59].0, ' ');
        f.mix = 1.0;
        let mut b = Canvas::new(30, 12);
        resonators(&mut b, r, &f);
        assert_eq!(b.cells[30].0, ' ');
        assert_eq!(b.cells[59].0, '│');
    }
}
