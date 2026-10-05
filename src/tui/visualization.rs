//! A view of measured model state, with separate coordinate systems and units.
use super::*;
use crate::{
    events::Kind,
    field::Surface,
    visual::{Frame, Shared},
};
use std::sync::{Arc, Mutex};

const LAYERS: [&str; 5] = ["Water", "Bed", "Flow", "Chimes", "Hearing"];

pub(super) struct Visualization {
    pub visible: bool,
    pub shared: Shared,
    frame: Option<Frame>,
    layer: usize,
    population: usize,
}

impl Default for Visualization {
    fn default() -> Self {
        Self {
            visible: false,
            shared: Arc::new(Mutex::new(Frame::default())),
            frame: None,
            layer: 0,
            population: 0,
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
        if code == KeyCode::Char('v') {
            self.visible = !self.visible;
            *log = false;
            return true;
        }
        if !self.visible {
            return false;
        }
        match code {
            KeyCode::Char('l') => {
                self.visible = false;
                *log = true;
            }
            KeyCode::Tab => self.layer = (self.layer + 1) % LAYERS.len(),
            KeyCode::BackTab => self.layer = (self.layer + LAYERS.len() - 1) % LAYERS.len(),
            KeyCode::Char(c @ '1'..='5') => self.layer = c as usize - '1' as usize,
            KeyCode::Char('[' | ']') => {
                let n = self
                    .frame
                    .as_ref()
                    .map_or(0, |f| f.populations.values.len())
                    .max(1);
                self.population =
                    (self.population + if code == KeyCode::Char(']') { 1 } else { n - 1 }) % n;
            }
            _ => return false,
        }
        true
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
            Some("Non-finite audio; playback stopped. Press r to restart.")
        } else if audio.clipped_samples > 0 {
            Some("Audio output reached its limit; lower the volume with -.")
        } else {
            None
        };
        if let Some(warning) = warning {
            let width = canvas.w;
            canvas.text(
                0,
                canvas.h.saturating_sub(4),
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
        c.text(0, 0, Color::Cyan, &format!(" ripple / physics / {title}"));
        if c.w < 79 || c.h < 20 {
            c.text(
                0,
                2,
                Color::White,
                "Enlarge to 80 columns x 20 rows for the maps.",
            );
            c.text(
                0,
                4,
                Color::DarkGrey,
                "v player | l log | Space pause | q quit",
            );
            return c;
        }
        let Some(f) = self.frame.as_ref().filter(|f| f.ready) else {
            c.text(
                0,
                2,
                Color::White,
                "Waiting for a snapshot of the playing world...",
            );
            c.text(0, 4, Color::DarkGrey, "v player | l log | q quit");
            return c;
        };
        c.text(
            0,
            1,
            Color::White,
            &format!(
                "{} | {:.3}s | {} | 10 Hz snapshots",
                if paused { "PAUSED" } else { "LIVE" },
                f.seconds,
                if f.candidate.is_some() {
                    "A+B simulated".into()
                } else {
                    format!("seed {}", f.seed)
                }
            ),
        );
        if f.candidate.is_some() {
            self.resonators(&mut c, f);
        } else {
            let tabs = LAYERS
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    if self.layer == i {
                        format!("[{} {label}]", i + 1)
                    } else {
                        format!("{} {label}", i + 1)
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            c.text(0, 2, Color::Cyan, &tabs);
            match self.layer {
                0..=2 => self.field(&mut c, f, events),
                3 => self.chimes(&mut c, f),
                _ => self.hearing(&mut c, f, events),
            }
        }
        let bottom = c.h - 4;
        let status = if loading {
            "Preparing next world; showing the current audio's state.".into()
        } else if f.omitted() > 0 {
            format!("Display capacity reached: {} items omitted.", f.omitted())
        } else if events.lost() > 0 {
            format!(
                "{} event marks missed; state snapshots remain independent.",
                events.lost()
            )
        } else {
            "Direct model state; no interpolated motion or simulated sound waves.".into()
        };
        c.text(0, bottom, Color::Yellow, &status);
        c.text(
            0,
            bottom + 1,
            Color::DarkGrey,
            if f.candidate.is_some() {
                "v player | l log"
            } else {
                "Tab/1-5 view | [ ] population | v player | l log"
            },
        );
        c.text(
            0,
            bottom + 2,
            Color::DarkGrey,
            "Space pause | +/- volume | r restart | q quit",
        );
        if f.candidate.is_some() {
            c.text(
                0,
                bottom + 3,
                Color::DarkGrey,
                "a/b compare | e evolve | s save | n new seed",
            );
        } else {
            c.text(
                0,
                bottom + 3,
                Color::DarkGrey,
                &format!(
                    "Drivers (0..1): wind {:.2} rain {:.2} day {:.2}{}",
                    f.air,
                    f.rain,
                    f.daylight,
                    f.fire
                        .map_or(String::new(), |fire| format!(" fire {fire:.2}"))
                ),
            );
        }
        c
    }

    fn field(&self, c: &mut Canvas, f: &Frame, events: &EventView) {
        if f.cells.values.len() != f.width * f.height || f.width == 0 || f.height == 0 {
            c.text(
                0,
                4,
                Color::Yellow,
                "Field exceeds display capacity; map unavailable.",
            );
            return;
        }
        let lo = f
            .cells
            .values
            .iter()
            .map(|v| v.bed_m)
            .fold(f32::INFINITY, f32::min);
        let hi = f
            .cells
            .values
            .iter()
            .map(|v| v.bed_m)
            .fold(f32::NEG_INFINITY, f32::max);
        let legend = match self.layer {
            0 => "Depth m: . dry , <.001 : <.01 ~ <.05 = <.2 O <.5 # <1 @ >=1".into(),
            1 => format!("Bed m: .:-=+*#%@ low {lo:.3} .. high {hi:.3} (auto range)"),
            _ => "Arrows: signed mean pipe flux; . stagnant/dry (not m/s)".into(),
        };
        c.text(0, 3, Color::White, &legend);
        c.text(
            0,
            4,
            Color::DarkGrey,
            "Plan view: +x right, +y down | area means when reduced",
        );
        c.text(
            0,
            5,
            Color::DarkGrey,
            &format!(
                "{}x{} cells, {:.2} m/cell | domain {:.2} x {:.2} m",
                f.width,
                f.height,
                f.cell_m,
                f.width as f32 * f.cell_m,
                f.height as f32 * f.cell_m
            ),
        );
        let plot = Plot::fit(
            c.w,
            c.h.saturating_sub(13),
            6,
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
                let (mut depth, mut bed, mut vx, mut vy, mut roofs) = (0.0, 0.0, 0.0, 0.0, 0);
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let v = &f.cells.values[sy * f.width + sx];
                        depth += v.depth_m;
                        bed += v.bed_m;
                        vx += v.flux[0];
                        vy += v.flux[1];
                        roofs += usize::from(v.surface == Surface::Roof);
                    }
                }
                let n = ((x1 - x0) * (y1 - y0)) as f32;
                depth /= n;
                bed /= n;
                vx /= n;
                vy /= n;
                let pixel = match self.layer {
                    0 => depth_pixel(depth),
                    1 => {
                        let t = ((bed - lo) / (hi - lo).max(1e-6)).clamp(0.0, 1.0);
                        (
                            b".:-=+*#%@"[(t * 8.0).round() as usize] as char,
                            Color::AnsiValue(101 + (t * 5.0) as u8),
                        )
                    }
                    _ => (if depth < 1e-4 { '.' } else { arrow(vx, vy) }, Color::Cyan),
                };
                c.put(
                    plot.x + x,
                    plot.y + y,
                    if roofs as f32 > n * 0.5 {
                        ('R', Color::White)
                    } else {
                        pixel
                    },
                );
            }
        }
        for r in events.recent(f.generation, f.seconds) {
            if let Kind::RainImpact { x, y, .. } = r.event {
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
        c.text(
            0,
            c.h - 7,
            Color::White,
            &format!(
                "Water {:.3} m3 | retained {:.3} | sediment {:.6}",
                f.water_m3, f.retained_m3, f.sediment_m3
            ),
        );
        c.text(
            0,
            c.h - 6,
            Color::DarkGrey,
            "R roof | + actual rain impact, held 0.5 simulation seconds",
        );
        c.text(
            0,
            c.h - 5,
            Color::DarkGrey,
            &format!(
                "Active bubbles {} / packets {}: no mapped positions",
                f.bubbles, f.clouds
            ),
        );
    }

    fn chimes(&self, c: &mut Canvas, f: &Frame) {
        c.text(
            0,
            3,
            Color::White,
            "Rig-local horizontal plane in metres | + anchors",
        );
        if f.bodies.values.is_empty() {
            c.text(
                0,
                5,
                Color::DarkGrey,
                "This world has no suspended chime rig.",
            );
            return;
        }
        // Fixed physical extent prevents the camera from disguising displacement.
        // A body beyond it is reported explicitly below rather than auto-zoomed.
        let plot = Plot::fit(c.w, c.h.saturating_sub(12), 5, [-0.16, 0.16, -0.16, 0.16]);
        let mut outside = 0;
        for (i, b) in f.bodies.values.iter().enumerate() {
            plot.mark(c, b.origin_m, ('+', Color::DarkGrey));
            let color = if b.touching {
                Color::Yellow
            } else if i == 0 {
                Color::White
            } else {
                Color::Cyan
            };
            // Rasterize the actual collision-disc radius in the same coordinates.
            for y in 0..plot.h {
                for x in 0..plot.w {
                    let p = plot.world(x, y);
                    let d = (p[0] - b.position_m[0]).hypot(p[1] - b.position_m[1]);
                    if d <= b.radius_m {
                        c.put(plot.x + x, plot.y + y, ('o', color));
                    }
                }
            }
            if !plot.mark(
                c,
                b.position_m,
                (
                    if i == 0 {
                        'C'
                    } else {
                        char::from_digit(((i - 1) % 10) as u32, 10).unwrap()
                    },
                    color,
                ),
            ) {
                outside += 1;
            }
        }
        c.text(
            0,
            4,
            Color::DarkGrey,
            "x/y: -0.16 .. +0.16 m | C clapper, 0-9 bars, yellow contact",
        );
        c.text(
            0,
            c.h - 7,
            Color::White,
            &format!(
                "Energy {:.6} J | wind work {:.6} J | {} impacts",
                f.contact.mechanical_energy_j, f.contact.wind_work_j, f.contact.impacts
            ),
        );
        c.text(
            0,
            c.h - 6,
            Color::DarkGrey,
            &format!(
                "Balance residual {:.2e} J | {} centres outside view",
                f.contact.integration_residual_j, outside
            ),
        );
        c.text(
            0,
            c.h - 5,
            Color::DarkGrey,
            "Small-angle suspension; audio-rate bending is not animated.",
        );
    }

    fn hearing(&self, c: &mut Canvas, f: &Frame, events: &EventView) {
        let count = f.populations.values.len();
        if count == 0 {
            c.text(
                0,
                4,
                Color::DarkGrey,
                "This world has no creature population.",
            );
            return;
        }
        let population = self.population % count;
        let p = &f.populations.values[population];
        let animals: Vec<_> = f
            .animals
            .values
            .iter()
            .filter(|a| a.population == population)
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
        c.text(
            0,
            3,
            Color::White,
            &format!(
                "Population {}/{} | c {:.1} m/s | source gain {:.3}",
                population + 1,
                count,
                p.sound_speed,
                p.gain
            ),
        );
        c.text(
            0,
            4,
            Color::DarkGrey,
            "o resting, * calling at source, L listener; fixed positions",
        );
        c.text(
            0,
            5,
            Color::DarkGrey,
            &format!(
                "Local metres: x {:.1}..{:.1}, y {:.1}..{:.1} (+y down)",
                bounds[0], bounds[1], bounds[2], bounds[3]
            ),
        );
        let plot = Plot::fit(c.w, c.h.saturating_sub(13), 6, bounds);
        if let Some(b) = p.barrier {
            plot.line(
                c,
                [b.x as f64, b.y_min as f64],
                [b.x as f64, b.y_max as f64],
                ('|', Color::White),
            );
        }
        let mut links = 0;
        for r in events.recent(f.generation, f.seconds) {
            if let Kind::CallHeard {
                population: group,
                caller,
                receiver,
                ..
            } = r.event
            {
                if group != population {
                    continue;
                }
                let a = animals.iter().find(|a| a.index == caller);
                let b = animals.iter().find(|a| a.index == receiver);
                if let (Some(a), Some(b)) = (a, b) {
                    plot.line(
                        c,
                        [a.position.x as f64, a.position.y as f64],
                        [b.position.x as f64, b.position.y as f64],
                        (':', Color::Green),
                    );
                    links += 1;
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
        plot.mark(
            c,
            [p.listener.x as f64, p.listener.y as f64],
            ('L', Color::White),
        );
        c.text(
            0,
            c.h - 7,
            Color::Green,
            &format!(": heard arrivals in last 0.5 s: {links} (marks, not waves)"),
        );
        c.text(
            0,
            c.h - 6,
            Color::DarkGrey,
            "Source calls precede listener audio by the modeled path delay.",
        );
        c.text(
            0,
            c.h - 5,
            Color::DarkGrey,
            "Each population has its own scene; not located on the water grid.",
        );
    }

    fn resonators(&self, c: &mut Canvas, f: &Frame) {
        let candidate = f.candidate.unwrap();
        c.text(
            0,
            2,
            Color::Cyan,
            &format!(
                "{} selected | audio mix A {:.1}% / B {:.1}%",
                if candidate == 0 {
                    "A parent"
                } else {
                    "B descendant"
                },
                (1.0 - f.mix) * 100.0,
                f.mix * 100.0
            ),
        );
        c.text(
            0,
            3,
            Color::White,
            "Modal energy = (z^2 + v^2)/2 | full bar 0.025 model units",
        );
        c.text(
            0,
            4,
            Color::DarkGrey,
            "Instantaneous energies; no slow-motion oscillator animation.",
        );
        let modes: Vec<_> = f
            .resonances
            .values
            .iter()
            .filter(|m| m.candidate == candidate)
            .collect();
        let rows = c.h.saturating_sub(11);
        for (row, m) in modes.iter().take(rows).enumerate() {
            let label = format!("{:2} {:7.1} Hz {:9.6} ", m.index, m.hz, m.energy);
            let available = c.w.saturating_sub(label.len());
            let bars = ((m.energy / 0.025).clamp(0.0, 1.0) * available as f64).round() as usize;
            c.text(
                0,
                row + 5,
                Color::Cyan,
                &format!("{label}{}", "|".repeat(bars)),
            );
        }
        c.text(
            0,
            c.h - 6,
            Color::White,
            &format!(
                "Total {:.6} | {}/{} modes shown",
                modes.iter().map(|m| m.energy).sum::<f64>(),
                rows.min(modes.len()),
                modes.len()
            ),
        );
        c.text(
            0,
            c.h - 5,
            Color::DarkGrey,
            "This model specifies bonds and pickups, not a spatial habitat.",
        );
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
        b".,:~=O#@"[i] as char,
        if i == 0 {
            Color::DarkGrey
        } else {
            Color::AnsiValue([0, 17, 18, 19, 25, 31, 38, 51][i])
        },
    )
}

fn arrow(x: f32, y: f32) -> char {
    if x.hypot(y) < 1e-7 {
        return '.';
    }
    if x.abs() > y.abs() * 2.0 {
        if x > 0.0 {
            '>'
        } else {
            '<'
        }
    } else if y.abs() > x.abs() * 2.0 {
        if y > 0.0 {
            'v'
        } else {
            '^'
        }
    } else if x > 0.0 {
        if y > 0.0 {
            '↘'
        } else {
            '↗'
        }
    } else if y > 0.0 {
        '↙'
    } else {
        '↖'
    }
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
    fn views_fit_resizes_and_use_explicit_units_and_unmodeled_geometry() {
        let world = presets::build("glade", 16_000.0, 12345).unwrap();
        let mut view = Visualization::default();
        world.visualize(&mut view.shared.lock().unwrap());
        view.refresh(0);
        let events = EventView::default();
        for (w, h) in [
            (0, 0),
            (1, 1),
            (32, 8),
            (49, 16),
            (80, 24),
            (120, 40),
            (300, 150),
        ] {
            for layer in 0..5 {
                view.layer = layer;
                let c = view.render("glade", false, false, &events, w, h);
                assert_eq!(c.cells.len(), c.w * c.h);
                assert!(c.w < w as usize || w == 0);
                assert!(c.h <= h as usize);
            }
        }
        view.layer = 0;
        let text = plain(&view.render("glade", true, false, &events, 100, 30));
        assert!(text.contains("PAUSED"));
        assert!(text.contains("Depth m:"));
        assert!(text.contains("no mapped positions"));
        view.layer = 4;
        assert!(plain(&view.render("glade", false, false, &events, 100, 30))
            .contains("not located on the water grid"));
    }

    #[test]
    fn new_generations_cannot_display_a_previous_world_and_modes_switch_cleanly() {
        let mut view = Visualization::default();
        view.shared.lock().unwrap().ready = true;
        view.refresh(0);
        assert!(view.frame.is_some());
        view.refresh(1);
        assert!(view.frame.is_none());
        let mut log = true;
        assert!(view.handle_key(KeyCode::Char('v'), &mut log));
        assert!(view.visible && !log);
        assert!(view.handle_key(KeyCode::BackTab, &mut log));
        assert_eq!(view.layer, 4);
        assert!(view.handle_key(KeyCode::Char('l'), &mut log));
        assert!(!view.visible && log);
        assert!(!view.handle_key(KeyCode::Tab, &mut log));
    }

    #[test]
    fn rain_marks_follow_event_positions_and_age_only_in_simulation_time() {
        let mut f = Frame {
            ready: true,
            width: 4,
            height: 4,
            cell_m: 1.0,
            seconds: 2.0,
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
        let view = Visualization {
            frame: Some(f),
            ..Visualization::default()
        };
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
        let frame = view.frame.as_ref().unwrap();
        assert_eq!(events.recent(0, frame.seconds).count(), 1);
        let mut c = Canvas::new(99, 30);
        view.field(&mut c, frame, &events);
        let plot = Plot::fit(99, 17, 6, [0.0, 4.0, 0.0, 4.0]);
        let x = plot.x + plot.w / 2;
        let y = plot.y + plot.h / 2;
        assert_eq!(c.cells[y * c.w + x].0, '+');
        assert_eq!(events.recent(0, 2.3).count(), 1); // only the newer event remains
        assert_eq!(events.recent(1, 2.0).count(), 0);
    }

    #[test]
    fn fixed_depth_scale_and_signed_flow_do_not_auto_normalize() {
        assert_eq!(depth_pixel(0.0).0, '.');
        assert_eq!(depth_pixel(0.1).0, '=');
        assert_eq!(depth_pixel(5.0).0, '@');
        assert_eq!(arrow(1.0, 0.0), '>');
        assert_eq!(arrow(-1.0, 0.0), '<');
        assert_eq!(arrow(0.0, 1.0), 'v');
        assert_eq!(arrow(1.0, -1.0), '↗');
    }
}
