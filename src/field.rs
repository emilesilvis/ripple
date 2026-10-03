//! The matter of the world: a shallow body of water over terrain.
//!
//! One law governs it — water accelerates down the gradient of its own
//! surface and carries its momentum (the shallow-water equations, integrated
//! with the "virtual pipes" scheme). From that single law, everything follows:
//! rain pools and swells a brook; a brook runs steadily down a rocky channel;
//! an imposed ocean swell travels up a beach, shoals, and breaks. The field
//! doesn't know those words. It only knows depth, slope, and momentum, and it
//! hands the rest of the world a few honest numbers: how fast the water is
//! moving, and where it churns.

use crate::dsp::Noise;

/// What a falling drop meets where it lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surface {
    /// Open ground or water: rain soaks in and raises the water here.
    Open,
    /// A roof: rain drums on it and runs off; the water below is unchanged.
    Roof,
    /// A rocky bed: rough, so moving water over it churns loudly.
    Rock,
}

/// A discrete thing the water does that the ear can hear. The field reports
/// only physical facts — a bubble's *size*, a wave's energy; what they sound
/// like is decided by law where the world listens (Minnaert's resonance turns
/// a radius into a pitch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Air entrained in fast water — a bubble of some radius, in metres.
    Bubble {
        pan: f32,
        radius_m: f32,
        energy: f32,
    },
    /// A wave shoaling and tipping over — a wash of foam.
    Break { pan: f32, energy: f32 },
}

/// A deliberately small catchment model: rain is retained in a linear soil
/// reservoir; a mobile bed exchanges and advects solid volume with the water.
/// Erosion runs faster than natural geological time.
struct History {
    retained: Vec<f64>, // water-equivalent depth, m
    bed: Vec<f64>,
    bedrock: Vec<f64>,
    sediment: Vec<f64>, // suspended solid volume / cell area, m
    delta: Vec<f64>,
    capacity_m: f64,
    release_seconds: f64,
    erosion_rate: f64,
    exported_solid_m3: f64,
    rain_m3: f64,
    drained_water_m3: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct HistoryStats {
    pub surface_water_m3: f64,
    pub retained_water_m3: f64,
    pub suspended_solid_m3: f64,
    pub bed_solid_m3: f64,
    pub exported_solid_m3: f64,
    pub rain_m3: f64,
    pub drained_water_m3: f64,
}

pub struct Field {
    w: usize,
    h: usize,
    cell: f32, // metres per cell
    terrain: Vec<f32>,
    depth: Vec<f32>,
    fx: Vec<f32>, // flux across each cell's right face (+ = rightward)
    fy: Vec<f32>, // flux across each cell's bottom face (+ = downward)
    rough: Vec<f32>,
    surface: Vec<Surface>,
    source: Vec<f32>, // steady inflow per cell (a spring)
    drain: Vec<f32>,  // fraction of depth removed per second (an outlet)
    // Optional ocean swell forced along the left edge.
    swell_amp: f32,
    swell_period: f32,
    swell_base: f32,
    time: f32,
    rng: Noise,
    // Per-cell memory for detecting a wave that just broke.
    prev_speed: Vec<f32>,
    events: Vec<Event>,
    history: Option<History>,
}

impl Field {
    pub fn new(w: usize, h: usize, cell: f32, seed: u32) -> Self {
        let n = w * h;
        Self {
            w,
            h,
            cell,
            terrain: vec![0.0; n],
            depth: vec![0.0; n],
            fx: vec![0.0; n],
            fy: vec![0.0; n],
            rough: vec![0.0; n],
            surface: vec![Surface::Open; n],
            source: vec![0.0; n],
            drain: vec![0.0; n],
            swell_amp: 0.0,
            swell_period: 8.0,
            swell_base: 0.0,
            time: 0.0,
            rng: Noise::new(seed ^ 0xf1e1_d000),
            prev_speed: vec![0.0; n],
            events: Vec::with_capacity(64),
            history: None,
        }
    }

    #[inline]
    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }

    pub fn width(&self) -> usize {
        self.w
    }
    pub fn height(&self) -> usize {
        self.h
    }

    // --- terrain / setup helpers (called by presets) -----------------------

    pub fn set_terrain(&mut self, x: usize, y: usize, height: f32) {
        let i = self.idx(x, y);
        self.terrain[i] = height;
        if let Some(history) = &mut self.history {
            history.bed[i] = height as f64;
        }
    }
    pub fn terrain_at(&self, x: usize, y: usize) -> f32 {
        self.terrain[self.idx(x, y)]
    }
    pub fn set_surface(&mut self, x: usize, y: usize, s: Surface) {
        let i = self.idx(x, y);
        self.surface[i] = s;
    }
    pub fn set_source(&mut self, x: usize, y: usize, rate: f32) {
        let i = self.idx(x, y);
        self.source[i] = rate;
    }
    pub fn set_drain(&mut self, x: usize, y: usize, rate: f32) {
        let i = self.idx(x, y);
        self.drain[i] = rate;
    }
    pub fn set_depth(&mut self, x: usize, y: usize, d: f32) {
        let i = self.idx(x, y);
        self.depth[i] = d;
    }
    pub fn set_swell(&mut self, amp: f32, period: f32, base: f32) {
        self.swell_amp = amp;
        self.swell_period = period;
        self.swell_base = base;
    }

    /// The surface at a normalised position (0..1, 0..1).
    pub fn surface_at(&self, nx: f32, ny: f32) -> Surface {
        let x = ((nx * self.w as f32) as usize).min(self.w - 1);
        let y = ((ny * self.h as f32) as usize).min(self.h - 1);
        self.surface[self.idx(x, y)]
    }

    /// Enable history after the initial geological epoch. Bedrock limits
    /// erosion; retained rain later returns as seepage with a 40-second time
    /// constant. Roofs are impermeable and keep their surface classification.
    pub fn enable_history(&mut self) {
        if self.history.is_some() {
            return;
        }
        let n = self.w * self.h;
        let bed: Vec<f64> = self.terrain.iter().map(|&h| h as f64).collect();
        let bedrock = bed
            .iter()
            .zip(&self.surface)
            .map(|(&h, surface)| h - if *surface == Surface::Rock { 0.0 } else { 0.06 })
            .collect();
        self.history = Some(History {
            retained: vec![0.0; n],
            bed,
            bedrock,
            sediment: vec![0.0; n],
            delta: vec![0.0; n],
            capacity_m: 0.08,
            release_seconds: 40.0,
            erosion_rate: 0.006,
            exported_solid_m3: 0.0,
            rain_m3: 0.0,
            drained_water_m3: 0.0,
        });
    }

    /// Rain is a water volume input, split between retention and runoff.
    /// Existing flowing water is not repeatedly reclassified as new rainfall.
    pub fn rain_on(&mut self, nx: f32, ny: f32, depth_m: f32) {
        let x = ((nx * self.w as f32) as usize).min(self.w - 1);
        let y = ((ny * self.h as f32) as usize).min(self.h - 1);
        let i = self.idx(x, y);
        let rain = depth_m.max(0.0) as f64;
        let retained = if let Some(history) = &mut self.history {
            history.rain_m3 += rain * (self.cell as f64).powi(2);
            let uptake = if self.surface[i] == Surface::Open {
                (rain * 0.8).min((history.capacity_m - history.retained[i]).max(0.0))
            } else {
                0.0
            };
            history.retained[i] += uptake;
            uptake
        } else {
            0.0
        };
        self.depth[i] += (rain - retained) as f32;
    }

    #[cfg(test)]
    fn rainfall(&mut self, rate_m_s: f32, dt: f32) {
        for y in 0..self.h {
            for x in 0..self.w {
                self.rain_on(
                    (x as f32 + 0.5) / self.w as f32,
                    (y as f32 + 0.5) / self.h as f32,
                    rate_m_s * dt,
                );
            }
        }
    }

    pub fn history_stats(&self) -> HistoryStats {
        let area = (self.cell as f64).powi(2);
        let mut stats = HistoryStats {
            surface_water_m3: self.depth.iter().map(|&d| d as f64).sum::<f64>() * area,
            retained_water_m3: 0.0,
            suspended_solid_m3: 0.0,
            bed_solid_m3: 0.0,
            exported_solid_m3: 0.0,
            rain_m3: 0.0,
            drained_water_m3: 0.0,
        };
        if let Some(history) = &self.history {
            stats.retained_water_m3 = history.retained.iter().sum::<f64>() * area;
            stats.suspended_solid_m3 = history.sediment.iter().sum::<f64>() * area;
            stats.bed_solid_m3 = history
                .bed
                .iter()
                .zip(&history.bedrock)
                .map(|(bed, rock)| bed - rock)
                .sum::<f64>()
                * area;
            stats.exported_solid_m3 = history.exported_solid_m3;
            stats.rain_m3 = history.rain_m3;
            stats.drained_water_m3 = history.drained_water_m3;
        }
        stats
    }

    fn release_retained_water(&mut self, dt: f32) {
        if let Some(history) = &mut self.history {
            let fraction = -(-(dt as f64) / history.release_seconds).exp_m1();
            for (depth, retained) in self.depth.iter_mut().zip(&mut history.retained) {
                let released = *retained * fraction;
                *retained -= released;
                *depth += released as f32;
            }
        }
    }

    /// Upwind transport uses the already-limited water fluxes. Every solid
    /// transfer is subtracted from one cell and added to its neighbour.
    fn transport_sediment(&mut self, dt: f32) {
        let Some(history) = &mut self.history else {
            return;
        };
        history.delta.fill(0.0);
        let area = (self.cell as f64).powi(2);
        let mut transfer = |a: usize, b: usize, flux: f32| {
            let (from, to) = if flux >= 0.0 { (a, b) } else { (b, a) };
            let water = self.depth[from] as f64 * area;
            if water > 1e-15 {
                let fraction = ((flux.abs() as f64 * dt as f64) / water).min(1.0);
                let solid = history.sediment[from] * fraction;
                history.delta[from] -= solid;
                history.delta[to] += solid;
            }
        };
        for y in 0..self.h {
            for x in 0..self.w {
                let i = y * self.w + x;
                if x + 1 < self.w {
                    transfer(i, i + 1, self.fx[i]);
                }
                if y + 1 < self.h {
                    transfer(i, i + self.w, self.fy[i]);
                }
            }
        }
        for (solid, delta) in history.sediment.iter_mut().zip(&history.delta) {
            *solid = (*solid + delta).max(0.0);
        }
    }

    fn exchange_with_bed(&mut self, dt: f32) {
        if self.history.is_none() {
            return;
        }
        for y in 0..self.h {
            for x in 0..self.w {
                let i = self.idx(x, y);
                if self.surface[i] == Surface::Roof {
                    continue;
                }
                let speed = self.cell_speed(x, y, i) as f64;
                let history = self.history.as_mut().unwrap();
                let capacity = self.depth[i] as f64 * (speed / 0.12).min(1.0) * 0.04;
                let difference = capacity - history.sediment[i];
                if difference > 0.0 {
                    let erosion = (history.erosion_rate * speed * dt as f64)
                        .min(difference)
                        .min((history.bed[i] - history.bedrock[i]).max(0.0));
                    history.bed[i] -= erosion;
                    history.sediment[i] += erosion;
                } else {
                    let fraction = if self.depth[i] < 1e-5 {
                        1.0
                    } else {
                        -(-(dt as f64) * 1.5).exp_m1()
                    };
                    let deposit = -difference * fraction;
                    history.bed[i] += deposit;
                    history.sediment[i] -= deposit;
                }
                self.terrain[i] = history.bed[i] as f32;
                let cover = (history.bed[i] - history.bedrock[i]).max(0.0);
                self.rough[i] = (0.12 + 0.88 * (1.0 - cover / 0.06).clamp(0.0, 1.0)) as f32;
                self.surface[i] = if cover < 0.004 {
                    Surface::Rock
                } else {
                    Surface::Open
                };
            }
        }
    }

    // --- the one law -------------------------------------------------------

    pub fn step(&mut self, dt: f32) {
        self.time += dt;
        self.release_retained_water(dt);
        let g = 9.8;
        let l = self.cell;
        let area = l * l;
        let flux_damp = 0.985; // gentle viscosity so it settles

        // Springs and the imposed ocean swell add water at the edges.
        for i in 0..self.w * self.h {
            if self.source[i] > 0.0 {
                self.depth[i] += self.source[i] * dt;
            }
        }
        if self.swell_amp > 0.0 {
            let phase = std::f32::consts::TAU * self.time / self.swell_period;
            let target = self.swell_base + self.swell_amp * phase.sin();
            for y in 0..self.h {
                let i = self.idx(0, y);
                let head = self.terrain[i] + self.depth[i];
                let want = target;
                if want > self.terrain[i] {
                    self.depth[i] = want - self.terrain[i];
                } else {
                    self.depth[i] = 0.0;
                }
                let _ = head;
            }
        }

        // 1. Update fluxes from the surface-height gradient (momentum).
        for y in 0..self.h {
            for x in 0..self.w - 1 {
                let a = self.idx(x, y);
                let b = a + 1;
                let dh = (self.terrain[a] + self.depth[a]) - (self.terrain[b] + self.depth[b]);
                self.fx[a] = (self.fx[a] + dt * g * dh / l) * flux_damp;
            }
        }
        for y in 0..self.h - 1 {
            for x in 0..self.w {
                let a = self.idx(x, y);
                let b = a + self.w;
                let dh = (self.terrain[a] + self.depth[a]) - (self.terrain[b] + self.depth[b]);
                self.fy[a] = (self.fy[a] + dt * g * dh / l) * flux_damp;
            }
        }

        // 2. Limit outflow so no cell goes negative (O'Brien flux scaling).
        for y in 0..self.h {
            for x in 0..self.w {
                let c = self.idx(x, y);
                let mut out = 0.0;
                if x < self.w - 1 && self.fx[c] > 0.0 {
                    out += self.fx[c];
                }
                if x > 0 && self.fx[c - 1] < 0.0 {
                    out += -self.fx[c - 1];
                }
                if y < self.h - 1 && self.fy[c] > 0.0 {
                    out += self.fy[c];
                }
                if y > 0 && self.fy[c - self.w] < 0.0 {
                    out += -self.fy[c - self.w];
                }
                let vol = self.depth[c] * area;
                if out * dt > vol && out > 1e-9 {
                    let k = vol / (out * dt);
                    if x < self.w - 1 && self.fx[c] > 0.0 {
                        self.fx[c] *= k;
                    }
                    if x > 0 && self.fx[c - 1] < 0.0 {
                        self.fx[c - 1] *= k;
                    }
                    if y < self.h - 1 && self.fy[c] > 0.0 {
                        self.fy[c] *= k;
                    }
                    if y > 0 && self.fy[c - self.w] < 0.0 {
                        self.fy[c - self.w] *= k;
                    }
                }
            }
        }

        self.transport_sediment(dt);

        // 3. Move the water; measure how fast each cell runs.
        for y in 0..self.h {
            for x in 0..self.w {
                let c = self.idx(x, y);
                let left = if x > 0 { self.fx[c - 1] } else { 0.0 };
                let right = if x < self.w - 1 { self.fx[c] } else { 0.0 };
                let up = if y > 0 { self.fy[c - self.w] } else { 0.0 };
                let down = if y < self.h - 1 { self.fy[c] } else { 0.0 };
                let d_vol = dt * (left - right + up - down);
                self.depth[c] += d_vol / area;
                if self.depth[c] < 0.0 {
                    self.depth[c] = 0.0;
                }
                if self.drain[c] > 0.0 {
                    let fraction = (self.drain[c] * dt).min(1.0);
                    if let Some(history) = &mut self.history {
                        history.drained_water_m3 +=
                            self.depth[c] as f64 * area as f64 * fraction as f64;
                        let solid = history.sediment[c] * fraction as f64;
                        history.sediment[c] -= solid;
                        history.exported_solid_m3 += solid * area as f64;
                    }
                    self.depth[c] *= 1.0 - fraction;
                }
            }
        }

        self.exchange_with_bed(dt);
        self.detect_events(dt);
    }

    /// The speed of the water running through a cell — the magnitude of the
    /// average face velocity. (The fluxes are already velocity-like, so this
    /// stays well behaved even in a thin film.)
    #[inline]
    fn cell_speed(&self, x: usize, y: usize, c: usize) -> f32 {
        let left = if x > 0 { self.fx[c - 1] } else { 0.0 };
        let right = self.fx[c];
        let up = if y > 0 { self.fy[c - self.w] } else { 0.0 };
        let down = self.fy[c];
        let sx = 0.5 * (left.abs() + right.abs());
        let sy = 0.5 * (up.abs() + down.abs());
        (sx * sx + sy * sy).sqrt()
    }

    /// Turn the water's motion into audible events: bubbles where fast water
    /// churns over rock, foam where a wave shoals and breaks.
    fn detect_events(&mut self, dt: f32) {
        for y in 0..self.h {
            for x in 0..self.w {
                let c = self.idx(x, y);
                let d = self.depth[c];
                if d < 1e-4 {
                    self.prev_speed[c] = 0.0;
                    continue;
                }
                let speed = self.cell_speed(x, y, c);
                let pan = x as f32 / (self.w - 1) as f32;

                // Bubbles: fast water over a rough bed entrains air. Faster
                // churn tears off smaller bubbles; their pitch is not chosen
                // here — size is the physical fact, Minnaert does the rest.
                let churn = speed * self.rough[c];
                if churn > 0.04 {
                    let rate = (churn * 40.0).min(20.0);
                    if self.rng.chance((rate * dt).min(0.9)) {
                        let radius_m = self.rng.range(0.0036, 0.009) / (1.0 + churn);
                        let energy = (churn * 0.5).min(0.1);
                        self.events.push(Event::Bubble {
                            pan,
                            radius_m,
                            energy,
                        });
                    }
                }

                // Breaking: a wave that was moving fast suddenly piles up in
                // shallow water (speed high, depth small, decelerating).
                let shoaling = self.swell_amp > 0.0 && self.terrain[c] > -0.35;
                if shoaling && speed > 0.035 && d < 0.3 && speed < self.prev_speed[c] * 0.99 {
                    let energy = (speed * 8.0).min(0.6);
                    if self.rng.chance((10.0 * dt).min(0.6)) {
                        self.events.push(Event::Break { pan, energy });
                    }
                }
                self.prev_speed[c] = speed;
            }
        }
    }

    /// Overall churn of freely running water (a brook): (loudness, brightness).
    pub fn flow(&self) -> (f32, f32) {
        let mut energy = 0.0;
        let mut speed_sum = 0.0;
        let mut count = 0.0;
        for y in 0..self.h {
            for x in 0..self.w {
                let c = self.idx(x, y);
                if self.depth[c] < 1e-4 {
                    continue;
                }
                let speed = self.cell_speed(x, y, c);
                energy += speed * self.rough[c];
                speed_sum += speed;
                count += 1.0;
            }
        }
        let e = (energy * 0.05).min(0.85);
        let s = if count > 0.0 {
            (speed_sum / count * 5.0).min(1.0)
        } else {
            0.0
        };
        (e, s)
    }

    pub fn drain_events(&mut self) -> std::vec::Drain<'_, Event> {
        self.events.drain(..)
    }

    /// Total water volume in the field — for diagnostics (a rising brook).
    pub fn total_water(&self) -> f32 {
        self.depth.iter().sum::<f32>() * self.cell * self.cell
    }

    /// A geological epoch, run before the audible world starts: let the
    /// water that will live here — the spring, an average rainfall, the
    /// swell — flow over the raw uplifted terrain and *carve* it.
    ///
    /// One law (stream power): running water wears the bed down in
    /// proportion to how fast it runs. Wherever flow concentrates it cuts
    /// deeper, which concentrates the flow further — and a channel is born,
    /// with no valley drawn by hand. The wear itself becomes the bed the
    /// ear will hear: heavily-cut cells are scoured to bare rock and stay
    /// rough, so the brook churns precisely where the water actually dug.
    pub fn geology(&mut self, steps: usize, rainfall: f32) {
        let dt = 0.01; // compressed geological time per step
        let k_erode = 0.4; // stream-power constant
                           // The soft cover is only so deep; below it, bedrock resists. This is
                           // what keeps a valley a valley instead of a bottomless trench.
        let soil = 0.35;
        let n = self.w * self.h;
        let mut wear = vec![0.0f32; n];

        for _ in 0..steps {
            if rainfall > 0.0 {
                for d in self.depth.iter_mut() {
                    *d += rainfall * dt;
                }
            }
            self.step(dt);
            for y in 0..self.h {
                for x in 0..self.w {
                    let c = self.idx(x, y);
                    let d = self.depth[c];
                    if d < 1e-4 {
                        continue;
                    }
                    // Only shallow, fast water works the bed — deep water's
                    // motion never reaches it. (This is also why the sea
                    // wears its shore exactly where the waves break.)
                    let shallow = (1.0 - d / 0.3).max(0.0);
                    let dz = (k_erode * shallow * self.cell_speed(x, y, c) * dt)
                        .min(soil - wear[c])
                        .max(0.0);
                    self.terrain[c] -= dz;
                    wear[c] += dz;
                }
            }
        }

        // The carved bed: roughness follows the cutting, and cells stripped
        // of most of their cover are scoured down to bare rock.
        for i in 0..n {
            let cut = wear[i] / soil;
            self.rough[i] = cut.sqrt().min(1.0);
            if cut > 0.6 {
                self.surface[i] = Surface::Rock;
            }
        }

        // The epoch's water drains away; the audible world starts fresh.
        self.depth.iter_mut().for_each(|d| *d = 0.0);
        self.fx.iter_mut().for_each(|f| *f = 0.0);
        self.fy.iter_mut().for_each(|f| *f = 0.0);
        self.prev_speed.iter_mut().for_each(|s| *s = 0.0);
        self.events.clear();
        self.time = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 32.0 / 48_000.0;

    fn landscape(seed: u32) -> Field {
        let mut field = Field::new(12, 24, 0.2, seed);
        let mut rng = Noise::new(seed);
        let phase = rng.range(0.0, std::f32::consts::TAU);
        for y in 0..field.height() {
            for x in 0..field.width() {
                let height = (field.height() - 1 - y) as f32 * 0.022
                    + 0.013 * (x as f32 * 1.7 + phase).sin() * (y as f32 * 0.9 + phase).cos();
                field.set_terrain(x, y, height);
                if y + 1 == field.height() {
                    field.set_drain(x, y, 3.0);
                }
            }
        }
        field.enable_history();
        field
    }

    fn bed_difference(a: &Field, b: &Field) -> f64 {
        let mut difference = 0.0;
        for y in 0..a.height() {
            for x in 0..a.width() {
                difference += (a.terrain_at(x, y) - b.terrain_at(x, y)).abs() as f64;
            }
        }
        difference / (a.width() * a.height()) as f64
    }

    fn water_error(stats: HistoryStats) -> f64 {
        stats.surface_water_m3 + stats.retained_water_m3 + stats.drained_water_m3 - stats.rain_m3
    }

    #[test]
    fn rain_is_retained_and_released_after_the_sky_clears() {
        let mut field = Field::new(2, 2, 1.0, 7);
        field.enable_history();
        field.rainfall(0.01, 1.0);
        let before = field.history_stats();
        for _ in 0..1500 {
            field.step(DT);
        }
        let after = field.history_stats();
        assert!(after.retained_water_m3 < before.retained_water_m3);
        assert!(after.surface_water_m3 > before.surface_water_m3);
        assert!((after.surface_water_m3 + after.retained_water_m3 - 0.04).abs() < 2e-6);
    }

    #[test]
    fn transported_solid_and_water_are_accounted_for() {
        let mut field = landscape(91);
        let initial = field.history_stats().bed_solid_m3;
        let mut eroded = false;
        for step in 0..(12.0 / DT) as usize {
            if step < (6.0 / DT) as usize {
                field.rainfall(0.01, DT);
            }
            field.step(DT);
            field.drain_events().for_each(drop);
            let stats = field.history_stats();
            eroded |= stats.suspended_solid_m3 > 1e-8;
            let solid = stats.bed_solid_m3 + stats.suspended_solid_m3 + stats.exported_solid_m3;
            assert!(
                (solid - initial).abs() < 1e-7,
                "solid budget drift {}",
                solid - initial
            );
            assert!(
                water_error(stats).abs() < 1e-4,
                "water budget drift {}",
                water_error(stats)
            );
        }
        assert!(eroded, "test must actually mobilise sediment");
        assert!(field.history_stats().exported_solid_m3 > 0.0);
    }

    #[test]
    fn different_histories_remain_distinct_under_identical_present_forcing() {
        let mut dry = landscape(13);
        let mut wet = landscape(13);
        for _ in 0..6000 {
            wet.rainfall(0.01, DT);
            dry.step(DT);
            wet.step(DT);
            dry.drain_events().for_each(drop);
            wet.drain_events().for_each(drop);
        }
        let retained = wet.history_stats().retained_water_m3;
        for _ in 0..6000 {
            dry.step(DT);
            wet.step(DT);
            dry.drain_events().for_each(drop);
            wet.drain_events().for_each(drop);
        }
        assert_eq!(dry.flow().0, 0.0);
        assert!(wet.flow().0 > 0.0);
        assert!(wet.history_stats().retained_water_m3 < retained);
        assert!(bed_difference(&dry, &wet) > 1e-7);
    }
}
