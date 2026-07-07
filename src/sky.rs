//! The slow state of the world above the ground: wind, rainfall, and the turn
//! of day. Nothing here makes sound directly. It sets the conditions — how hard
//! the air is moving, whether it is raining, whether it is night — and lets the
//! ground below respond. Left alone, it wanders, so the soundscape drifts of
//! its own accord.

use crate::dsp::{Noise, OnePole};

/// The starting weather and how freely it is allowed to wander.
#[derive(Clone)]
pub struct Climate {
    pub wind_base: f32, // 0..1 typical wind strength
    pub wind_var: f32,  // how much the gusts vary
    pub rain_base: f32, // 0..1 typical rainfall (0 = dry)
    pub rain_var: f32,
    /// Length of a full day in seconds (0 = time stands still at `day_start`).
    pub day_len: f32,
    pub day_start: f32, // 0 = midnight, 0.5 = noon
}

impl Climate {
    pub fn calm_day() -> Self {
        Self { wind_base: 0.25, wind_var: 0.3, rain_base: 0.0, rain_var: 0.0, day_len: 0.0, day_start: 0.3 }
    }
    pub fn still_night() -> Self {
        Self { wind_base: 0.12, wind_var: 0.15, rain_base: 0.0, rain_var: 0.0, day_len: 0.0, day_start: 0.92 }
    }
    pub fn windy() -> Self {
        Self { wind_base: 0.6, wind_var: 0.5, rain_base: 0.0, rain_var: 0.0, day_len: 0.0, day_start: 0.4 }
    }
    pub fn rainy() -> Self {
        Self { wind_base: 0.35, wind_var: 0.4, rain_base: 0.7, rain_var: 0.4, day_len: 0.0, day_start: 0.35 }
    }
}

pub struct Sky {
    rng: Noise,
    wind: f32,
    wind_target: f32,
    wind_timer: f32,
    wind_smooth: OnePole,
    gust: f32, // fast flutter riding on the slow wind
    gust_target: f32,
    gust_timer: f32,
    gust_smooth: OnePole,
    rain: f32,
    rain_target: f32,
    rain_timer: f32,
    day_phase: f32,
    day_rate: f32,
    climate: Climate,
}

impl Sky {
    pub fn new(climate: Climate, sr: f32, seed: u32) -> Self {
        Self {
            rng: Noise::new(seed ^ 0x5c1_2abc),
            wind: climate.wind_base,
            wind_target: climate.wind_base,
            wind_timer: 0.0,
            wind_smooth: OnePole::new(0.4, sr),
            gust: 0.4,
            gust_target: 0.4,
            gust_timer: 0.0,
            gust_smooth: OnePole::new(3.0, sr),
            rain: climate.rain_base,
            rain_target: climate.rain_base,
            rain_timer: 0.0,
            day_phase: climate.day_start,
            day_rate: if climate.day_len > 0.0 { 1.0 / climate.day_len } else { 0.0 },
            climate,
        }
    }

    pub fn step(&mut self, dt: f32) {
        // Slow wind: pick a new strength every several seconds, ease toward it.
        self.wind_timer -= dt;
        if self.wind_timer <= 0.0 {
            let c = &self.climate;
            self.wind_target =
                (c.wind_base + self.rng.range(-c.wind_var, c.wind_var)).clamp(0.0, 1.0);
            self.wind_timer = self.rng.range(4.0, 12.0);
        }
        self.wind = self.wind_smooth.process(self.wind_target);

        // Faster gusts riding on top.
        self.gust_timer -= dt;
        if self.gust_timer <= 0.0 {
            self.gust_target = self.rng.range(0.2, 1.0);
            self.gust_timer = self.rng.range(0.6, 2.5);
        }
        self.gust = self.gust_smooth.process(self.gust_target);

        // Rainfall wanders (passing showers) if this climate rains at all.
        if self.climate.rain_base > 0.0 || self.climate.rain_var > 0.0 {
            self.rain_timer -= dt;
            if self.rain_timer <= 0.0 {
                let c = &self.climate;
                self.rain_target =
                    (c.rain_base + self.rng.range(-c.rain_var, c.rain_var)).clamp(0.0, 1.0);
                self.rain_timer = self.rng.range(8.0, 20.0);
            }
            self.rain += (self.rain_target - self.rain) * (dt / 3.0).min(1.0);
        }

        // The turn of day.
        if self.day_rate > 0.0 {
            self.day_phase = (self.day_phase + self.day_rate * dt) % 1.0;
        }
    }

    /// Combined wind × gust, the actual air speed right now (0..~1).
    pub fn air_speed(&self) -> f32 {
        (0.3 + 0.7 * self.gust) * self.wind
    }
    /// The current gust flutter (0..1), for whistles and chime strikes.
    pub fn gust(&self) -> f32 {
        self.gust
    }
    /// Rainfall rate (0..1).
    pub fn rain(&self) -> f32 {
        self.rain
    }
    /// 0 = deep night, 1 = full day (a smooth day/night weighting).
    pub fn daylight(&self) -> f32 {
        // day_phase: 0 = midnight, 0.5 = noon.
        (0.5 - 0.5 * (std::f32::consts::TAU * self.day_phase).cos()).clamp(0.0, 1.0)
    }
    /// 1 = deep night, 0 = full day.
    pub fn night(&self) -> f32 {
        1.0 - self.daylight()
    }
}
