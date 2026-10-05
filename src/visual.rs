//! Read-only observations of the state that produces the audio.
//!
//! Buffers are allocated before playback. Writers never grow them; if a future
//! world exceeds the display budget, the frame explicitly reports omissions.
//! There is no renderer clock, random stream, or secondary simulation here.
use crate::{
    acoustics::{Barrier, Point},
    contacts::Diagnostics,
    field::Surface,
};

pub type Shared = std::sync::Arc<std::sync::Mutex<Frame>>;

#[derive(Clone, Debug)]
pub struct Buffer<T> {
    pub values: Vec<T>,
    pub omitted: usize,
}

impl<T> Buffer<T> {
    fn new(capacity: usize) -> Self {
        Self {
            values: Vec::with_capacity(capacity),
            omitted: 0,
        }
    }
    pub fn clear(&mut self) {
        self.values.clear();
        self.omitted = 0;
    }
    pub fn push(&mut self, value: T) {
        if self.values.len() < self.values.capacity() {
            self.values.push(value);
        } else {
            self.omitted += 1;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub bed_m: f32,
    pub depth_m: f32,
    /// Signed mean face fluxes of the virtual-pipe solver. These are not a
    /// calibrated fluid velocity; do not label them m/s or advect particles.
    pub flux: [f32; 2],
    pub surface: Surface,
}

#[derive(Clone, Copy, Debug)]
pub struct SuspendedBody {
    pub origin_m: [f64; 2],
    pub position_m: [f64; 2],
    pub radius_m: f64,
    pub touching: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Animal {
    pub population: usize,
    pub index: usize,
    pub position: Point,
    /// Syllable currently sounding at the source, before listener travel time.
    pub calling: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Population {
    pub listener: Point,
    pub barrier: Option<Barrier>,
    pub gain: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Resonance {
    pub candidate: usize,
    pub hz: f64,
    pub energy: f64,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub generation: u64,
    pub seconds: f64,
    pub ready: bool,
    pub seed: u32,
    pub width: usize,
    pub height: usize,
    pub cell_m: f32,
    pub cells: Buffer<Cell>,
    /// Clapper first, followed by bars, in the rig's local horizontal plane.
    pub bodies: Buffer<SuspendedBody>,
    pub contact: Diagnostics,
    pub animals: Buffer<Animal>,
    pub populations: Buffer<Population>,
    pub resonances: Buffer<Resonance>,
    pub candidate: Option<usize>,
    /// Actual smoothed B mix, including A/B crossfades.
    pub mix: f64,
    /// Dimensionless model drivers, not measured meteorological quantities.
    pub air: f32,
    /// Presence of sound-producing systems, independent of momentary activity.
    /// Keeps automatic composition stable during quiet passages.
    pub water_sound: bool,
    pub air_sound: bool,
    pub rain_sound: bool,
    pub rain: f32,
    pub daylight: f32,
    pub fire: Option<f32>,
    pub bubbles: usize,
    pub clouds: usize,
    pub water_m3: f64,
    pub retained_m3: f64,
    pub sediment_m3: f64,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            generation: 0,
            seconds: 0.0,
            ready: false,
            seed: 0,
            width: 0,
            height: 0,
            cell_m: 0.0,
            cells: Buffer::new(4096),
            bodies: Buffer::new(65),
            animals: Buffer::new(1024),
            populations: Buffer::new(8),
            resonances: Buffer::new(24),
            candidate: None,
            mix: 0.0,
            contact: Diagnostics::default(),
            air: 0.0,
            water_sound: false,
            air_sound: false,
            rain_sound: false,
            rain: 0.0,
            daylight: 0.0,
            fire: None,
            bubbles: 0,
            clouds: 0,
            water_m3: 0.0,
            retained_m3: 0.0,
            sediment_m3: 0.0,
        }
    }
}

impl Frame {
    pub fn clear(&mut self) {
        self.ready = false;
        self.seed = 0;
        self.width = 0;
        self.height = 0;
        self.cell_m = 0.0;
        self.cells.clear();
        self.bodies.clear();
        self.animals.clear();
        self.populations.clear();
        self.resonances.clear();
        self.candidate = None;
        self.mix = 0.0;
        self.contact = Diagnostics::default();
        self.air = 0.0;
        self.water_sound = false;
        self.air_sound = false;
        self.rain_sound = false;
        self.rain = 0.0;
        self.daylight = 0.0;
        self.fire = None;
        self.bubbles = 0;
        self.clouds = 0;
        self.water_m3 = 0.0;
        self.retained_m3 = 0.0;
        self.sediment_m3 = 0.0;
    }
    pub fn omitted(&self) -> usize {
        self.cells.omitted
            + self.bodies.omitted
            + self.animals.omitted
            + self.populations.omitted
            + self.resonances.omitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell as Counter;
    thread_local! {
        static TRACK: Counter<bool> = const { Counter::new(false) };
        static ALLOCATIONS: Counter<usize> = const { Counter::new(0) };
    }
    struct AuditAllocator;
    #[global_allocator]
    static ALLOCATOR: AuditAllocator = AuditAllocator;
    unsafe impl GlobalAlloc for AuditAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if TRACK.try_with(|t| t.get()).unwrap_or(false) {
                let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
            }
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[test]
    fn snapshots_preserve_audio_for_every_world_and_use_no_callback_allocations() {
        let sr = 16_000.0;
        for name in crate::presets::PRESETS
            .iter()
            .map(|p| p.name)
            .chain(["generated"])
        {
            let build = || {
                if name == "generated" {
                    crate::alien::worlds::WorldSpec::random(12345)
                        .build(sr)
                        .unwrap()
                } else {
                    crate::presets::build(name, sr, 12345).unwrap()
                }
            };
            let mut world = build();
            let mut reference = build();
            let mut frame = Frame::default();
            for i in 0..32_000 {
                assert_eq!(
                    world.next_sample(),
                    reference.next_sample(),
                    "{name} at {i}"
                );
                if i % 1600 == 0 {
                    ALLOCATIONS.with(|n| n.set(0));
                    TRACK.with(|t| t.set(true));
                    world.visualize(&mut frame);
                    TRACK.with(|t| t.set(false));
                    assert_eq!(ALLOCATIONS.with(|n| n.get()), 0, "{name}");
                    assert_eq!(frame.omitted(), 0, "{name}");
                    assert_eq!(frame.cells.values.len(), frame.width * frame.height);
                    assert!(frame
                        .cells
                        .values
                        .iter()
                        .all(|c| c.depth_m >= 0.0 && c.depth_m.is_finite()));
                }
            }
        }
    }

    #[test]
    fn grid_snapshot_matches_known_geometry_and_conserves_the_displayed_water() {
        let mut field = crate::field::Field::new(3, 2, 0.25, 5);
        field.set_terrain(2, 1, 0.75);
        field.set_depth(2, 1, 0.125);
        field.set_surface(2, 1, Surface::Roof);
        let mut frame = Frame::default();
        field.visualize(&mut frame);
        assert_eq!((frame.width, frame.height, frame.cell_m), (3, 2, 0.25));
        let cell = &frame.cells.values[5];
        assert_eq!(
            (cell.bed_m, cell.depth_m, cell.surface),
            (0.75, 0.125, Surface::Roof)
        );
        assert_eq!(cell.flux, [0.0; 2]);
        let volume: f64 = frame
            .cells
            .values
            .iter()
            .map(|c| c.depth_m as f64 * 0.25 * 0.25)
            .sum();
        assert_eq!(volume, frame.water_m3);
    }

    #[test]
    fn bounded_storage_reports_omissions_without_growing() {
        let mut buffer = Buffer::new(2);
        let address = buffer.values.as_ptr();
        for i in 0..10 {
            buffer.push(i);
        }
        assert_eq!(buffer.values.as_ptr(), address);
        assert_eq!(buffer.values, [0, 1]);
        assert_eq!(buffer.omitted, 8);
        buffer.clear();
        assert_eq!(buffer.omitted, 0);
        assert_eq!(buffer.values.capacity(), 2);
    }

    #[test]
    fn discovery_views_follow_the_real_candidate_and_saved_world_state() {
        let mut demo = crate::alien::LiveDemo::new(16_000.0, 12345);
        let mut reference = crate::alien::LiveDemo::new(16_000.0, 12345);
        let mut frame = Frame::default();
        for candidate in [1, 0, 1] {
            demo.select(candidate);
            reference.select(candidate);
            for i in 0..3200 {
                assert_eq!(demo.next_sample(), reference.next_sample());
                if i % 160 == 0 {
                    ALLOCATIONS.with(|n| n.set(0));
                    TRACK.with(|t| t.set(true));
                    demo.visualize(&mut frame);
                    TRACK.with(|t| t.set(false));
                    assert_eq!(ALLOCATIONS.with(|n| n.get()), 0);
                    assert_eq!(frame.candidate, Some(candidate));
                    assert!((0.0..=1.0).contains(&frame.mix));
                    for mode in &frame.resonances.values {
                        assert!(mode.hz > 0.0 && mode.hz.is_finite());
                        assert!(mode.energy.is_finite() && (0.0..=0.025).contains(&mode.energy));
                    }
                }
            }
        }
        let spec = crate::alien::worlds::WorldSpec::random(12345);
        let (bodies, animals) = spec.counts();
        let mut saved = crate::alien::worlds::Soundscape::new(&spec, 16_000.0, 0.3).unwrap();
        for _ in 0..1600 {
            saved.next_sample();
        }
        saved.visualize(&mut frame);
        assert_eq!(frame.seed, 12345);
        assert_eq!(frame.candidate, None);
        assert_eq!(frame.resonances.values.len(), 0);
        assert_eq!(frame.bodies.values.len(), bodies + 1);
        assert_eq!(frame.animals.values.len(), animals);
        assert_eq!(frame.omitted(), 0);
    }
}
