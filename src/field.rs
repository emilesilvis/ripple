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
#[derive(Clone, Copy, PartialEq)]
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
#[derive(Clone, Copy)]
pub enum Event {
    /// Air entrained in fast water — a bubble of some radius, in metres.
    Bubble { pan: f32, radius_m: f32, energy: f32 },
    /// A wave shoaling and tipping over — a wash of foam.
    Break { pan: f32, energy: f32 },
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

    /// Deposit water at a normalised position — a raindrop soaking in.
    pub fn add_water(&mut self, nx: f32, ny: f32, amount: f32) {
        let x = ((nx * self.w as f32) as usize).min(self.w - 1);
        let y = ((ny * self.h as f32) as usize).min(self.h - 1);
        let i = self.idx(x, y);
        self.depth[i] += amount;
    }

    // --- the one law -------------------------------------------------------

    pub fn step(&mut self, dt: f32) {
        self.time += dt;
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
                    self.depth[c] *= 1.0 - (self.drain[c] * dt).min(1.0);
                }
            }
        }

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
                        self.events.push(Event::Bubble { pan, radius_m, energy });
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
