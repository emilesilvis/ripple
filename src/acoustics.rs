//! Geometric sound paths shared by animal hearing and the human listener.
//!
//! This intentionally small model uses finite travel time, softened inverse
//! distance spreading, a straight barrier and a linear noise threshold. It
//! does not model diffraction, frequency-dependent hearing or reflections.

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Barrier {
    pub x: f32,
    pub y_min: f32,
    pub y_max: f32,
    /// Transmitted amplitude, not energy (0 = opaque; 1 = open).
    pub transmission: f32,
}

impl Barrier {
    fn crosses(&self, from: Point, to: Point) -> bool {
        let a = from.x - self.x;
        let b = to.x - self.x;
        if a * b >= 0.0 {
            return false;
        }
        let t = (self.x - from.x) / (to.x - from.x);
        let y = from.y + t * (to.y - from.y);
        y >= self.y_min && y <= self.y_max
    }
}

#[derive(Clone)]
pub struct HearingScene {
    pub positions: Vec<Point>,
    pub listener: Point,
    pub barrier: Option<Barrier>,
    pub sound_speed: f32,
    pub hearing_threshold: f32,
    /// Estimated diffuse background level at the animals' ears.
    pub masking_level: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct AcousticPath {
    pub delay_samples: usize,
    pub gain: f32,
    pub pan: f32,
}

impl HearingScene {
    pub fn meadow(count: usize) -> Self {
        let positions: Vec<_> = (0..count)
            .map(|i| {
                let angle = i as f32 * 2.399_963_1;
                let radius = 2.0 + 0.8 * (i as f32).sqrt();
                Point {
                    x: radius * angle.cos(),
                    y: 4.0 + radius * angle.sin(),
                }
            })
            .collect();
        Self {
            positions,
            listener: Point { x: 0.0, y: -2.0 },
            barrier: None,
            sound_speed: 343.0,
            hearing_threshold: 0.006,
            masking_level: 0.001,
        }
    }

    /// Two clusters of six crickets. Both runs retain the same positions,
    /// clocks and voice random streams; only barrier transmission changes.
    #[cfg(test)]
    pub(crate) fn two_groups(barrier: bool) -> Self {
        let mut scene = Self::meadow(12);
        scene.positions = (0..12)
            .map(|i| Point {
                x: if i < 6 {
                    -6.0 - (i % 2) as f32 * 2.0
                } else {
                    6.0 + (i % 2) as f32 * 2.0
                },
                y: (i % 6 / 2) as f32 * 2.5,
            })
            .collect();
        scene.listener = Point { x: -3.0, y: -4.0 };
        scene.barrier = barrier.then_some(Barrier {
            x: 0.0,
            y_min: -20.0,
            y_max: 20.0,
            transmission: 0.04,
        });
        scene
    }

    pub fn validate(&self, sr: f32) -> Result<(), String> {
        if !sr.is_finite() || !(100.0..=384_000.0).contains(&sr) {
            return Err("hearing sample rate must be between 100 and 384000 Hz".into());
        }
        if self.positions.len() > 128 {
            return Err("hearing supports at most 128 animals".into());
        }
        if !self.sound_speed.is_finite() || !(10.0..=1_000.0).contains(&self.sound_speed) {
            return Err("sound speed must be between 10 and 1000 metres per second".into());
        }
        if !self.hearing_threshold.is_finite()
            || self.hearing_threshold < 0.0
            || !self.masking_level.is_finite()
            || self.masking_level < 0.0
        {
            return Err(
                "hearing threshold and masking level must be finite and nonnegative".into(),
            );
        }
        for point in self.positions.iter().chain(std::iter::once(&self.listener)) {
            if !point.x.is_finite()
                || !point.y.is_finite()
                || point.x.abs() > 1_000.0
                || point.y.abs() > 1_000.0
            {
                return Err("hearing coordinates must be finite and within 1000 metres".into());
            }
        }
        if let Some(b) = self.barrier {
            if !b.x.is_finite()
                || !b.y_min.is_finite()
                || !b.y_max.is_finite()
                || b.y_min > b.y_max
                || !b.transmission.is_finite()
                || !(0.0..=1.0).contains(&b.transmission)
            {
                return Err("barrier geometry and transmission must be finite and valid".into());
            }
        }
        Ok(())
    }

    pub fn path(&self, from: Point, to: Point, sr: f32) -> AcousticPath {
        let dx = from.x - to.x;
        let dy = from.y - to.y;
        let distance = dx.hypot(dy);
        let transmission = self
            .barrier
            .filter(|b| b.crosses(from, to))
            .map_or(1.0, |b| b.transmission);
        AcousticPath {
            // A minimum one-sample delay makes even coincident positions
            // causal, with identical timing for hearing and listener paths.
            delay_samples: ((distance / self.sound_speed * sr).ceil() as usize).max(1),
            gain: transmission / (1.0 + (distance / 4.0).powi(2)).sqrt(),
            pan: 0.5 + 0.45 * dx / (dx.abs() + dy.abs() + 2.0),
        }
    }
}

/// A fixed delay with no allocations while rendering.
pub struct SignalDelay {
    samples: Vec<f32>,
    cursor: usize,
}

impl SignalDelay {
    pub fn new(samples: usize) -> Self {
        Self {
            samples: vec![0.0; samples.max(1)],
            cursor: 0,
        }
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let out = self.samples[self.cursor];
        self.samples[self.cursor] = sample;
        self.cursor += 1;
        if self.cursor == self.samples.len() {
            self.cursor = 0;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_barrier_change_gain_without_changing_causal_delay() {
        let mut scene = HearingScene::meadow(0);
        let from = Point { x: -4.0, y: 0.0 };
        let to = Point { x: 4.0, y: 0.0 };
        let open = scene.path(from, to, 48_000.0);
        let near = scene.path(from, Point { x: -3.0, y: 0.0 }, 48_000.0);
        assert!(near.gain > open.gain);
        assert!(near.delay_samples < open.delay_samples);
        scene.barrier = Some(Barrier {
            x: 0.0,
            y_min: -1.0,
            y_max: 1.0,
            transmission: 0.04,
        });
        let closed = scene.path(from, to, 48_000.0);
        assert_eq!(closed.delay_samples, open.delay_samples);
        assert!((closed.gain - open.gain * 0.04).abs() < 1e-7);
        assert_eq!(
            scene.path(from, Point { x: -3.0, y: 0.0 }, 48_000.0).gain,
            near.gain
        );
        let above = scene.path(
            Point { x: -4.0, y: 2.0 },
            Point { x: 4.0, y: 2.0 },
            48_000.0,
        );
        assert_eq!(above.gain, open.gain);
    }

    #[test]
    fn listener_signal_obeys_the_path_delay_exactly() {
        let scene = HearingScene::meadow(0);
        let path = scene.path(Point { x: 0.0, y: 0.0 }, Point { x: 34.3, y: 0.0 }, 1_000.0);
        let mut delay = SignalDelay::new(path.delay_samples);
        assert_eq!(delay.process(1.0), 0.0);
        for _ in 1..path.delay_samples {
            assert_eq!(delay.process(0.0), 0.0);
        }
        assert_eq!(delay.process(0.0) * path.gain, path.gain);
        assert_eq!(delay.process(0.0), 0.0);
    }

    #[test]
    fn invalid_scene_is_rejected_before_allocating_delay_buffers() {
        let mut scene = HearingScene::meadow(2);
        scene.positions[0].x = f32::NAN;
        assert!(scene.validate(48_000.0).is_err());
        scene.positions[0].x = 0.0;
        scene.sound_speed = 0.0;
        assert!(scene.validate(48_000.0).is_err());
    }
}
