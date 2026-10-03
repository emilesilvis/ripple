//! Small, passive clouds of acoustically coupled bubbles.
//!
//! A bubble's gas compression supplies stiffness; surrounding water supplies
//! inertia. With volume perturbation V_i = sqrt(R_i) y_i, their equations are
//! M y'' + C y' + K y = F, where M_ii = 1,
//! M_ij = sqrt(R_i R_j) / distance(i,j), and K_ii = (2 pi f_i)^2.
//! Here f_i follows Minnaert's law. Off-diagonal *inertia*, rather than an
//! extra bass oscillator, creates the collective low mode.
//!
//! This uses the spherical monopole model of Xue et al., SIGGRAPH 2023,
//! sections 3.1, 3.2 and 4.1: https://graphics.stanford.edu/papers/coupledbubbles/
//! It is a small fixed-geometry approximation, not their fluid simulation:
//! eight separated spherical bubbles, no advection, retardation, surface
//! image sources, merging or dissolution. Modal damping uses a fixed loss
//! ratio; the paper's full thermal/viscous damping and airborne FDTD are
//! omitted. Sound is proportional to coherent total volume acceleration.

use crate::dsp::pan;
use std::f64::consts::TAU;

const COUNT: usize = 8;
const LOSS_RATIO: f64 = 0.009;
const PRESSURE_PA: f64 = 101_325.0;
const WATER_DENSITY: f64 = 1000.0;
const HEAT_CAPACITY_RATIO: f64 = 1.4;
// Convert normalized volume acceleration to the engine's output level.
const OUTPUT_GAIN: f64 = 0.5;
type Matrix = [[f64; COUNT]; COUNT];

fn minnaert_hz(radius_m: f64) -> f64 {
    (3.0 * HEAT_CAPACITY_RATIO * PRESSURE_PA / WATER_DENSITY).sqrt() / (TAU * radius_m)
}

#[derive(Clone, Copy, Default)]
struct ModeShape {
    /// Frequency relative to Minnaert's frequency at the total-volume radius.
    frequency_ratio: f64,
    /// Projection of a spatially uniform pressure impulse / coherent pickup.
    projection: f64,
}

/// Fixed-size Jacobi diagonalization of a symmetric matrix. Only construction
/// uses dense algebra; neither spawning nor audio processing allocates.
fn eigensystem(mut matrix: Matrix, count: usize) -> ([f64; COUNT], Matrix) {
    let mut vectors = [[0.0; COUNT]; COUNT];
    for (i, row) in vectors.iter_mut().enumerate().take(count) {
        row[i] = 1.0;
    }
    for _ in 0..64 * COUNT * COUNT {
        let (mut p, mut q, mut off) = (0, 0, 0.0f64);
        for (i, row) in matrix.iter().enumerate().take(count) {
            for (j, value) in row.iter().enumerate().take(count).skip(i + 1) {
                if value.abs() > off {
                    (p, q, off) = (i, j, value.abs());
                }
            }
        }
        if off < 1e-12 {
            let mut eigenvalues = [0.0; COUNT];
            for i in 0..count {
                eigenvalues[i] = matrix[i][i];
            }
            return (eigenvalues, vectors);
        }
        let angle = 0.5 * (2.0 * matrix[p][q]).atan2(matrix[q][q] - matrix[p][p]);
        let (s, c) = angle.sin_cos();
        let (pp, qq, pq) = (matrix[p][p], matrix[q][q], matrix[p][q]);
        matrix[p][p] = c * c * pp - 2.0 * s * c * pq + s * s * qq;
        matrix[q][q] = s * s * pp + 2.0 * s * c * pq + c * c * qq;
        matrix[p][q] = 0.0;
        matrix[q][p] = 0.0;
        for i in 0..count {
            if i != p && i != q {
                let (ip, iq) = (matrix[i][p], matrix[i][q]);
                matrix[i][p] = c * ip - s * iq;
                matrix[p][i] = matrix[i][p];
                matrix[i][q] = s * ip + c * iq;
                matrix[q][i] = matrix[i][q];
            }
            let (vp, vq) = (vectors[i][p], vectors[i][q]);
            vectors[i][p] = c * vp - s * vq;
            vectors[i][q] = s * vp + c * vq;
        }
    }
    panic!("fixed bubble eigensystem did not converge");
}

/// Generalized modes K phi = lambda M phi, normalized phi^T M phi = 1.
/// Radii and positions are relative to the packet's equivalent gas radius.
fn mode_shapes(radii: &[f64], positions: &[[f64; 3]], coupling: f64) -> [ModeShape; COUNT] {
    let count = radii.len();
    assert!(count > 0 && count <= COUNT && positions.len() == count);
    let mut mass = [[0.0; COUNT]; COUNT];
    for i in 0..count {
        mass[i][i] = 1.0;
        for j in 0..i {
            let distance = positions[i]
                .iter()
                .zip(positions[j])
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(
                distance >= radii[i] + radii[j],
                "bubble spheres must not overlap"
            );
            mass[i][j] = coupling * (radii[i] * radii[j]).sqrt() / distance;
            mass[j][i] = mass[i][j];
        }
    }
    let mut lower = [[0.0; COUNT]; COUNT];
    for i in 0..count {
        for j in 0..=i {
            let remainder = mass[i][j] - (0..j).map(|k| lower[i][k] * lower[j][k]).sum::<f64>();
            if i == j {
                assert!(remainder > 0.0, "fluid inertia must be positive definite");
                lower[i][j] = remainder.sqrt();
            } else {
                lower[i][j] = remainder / lower[j][j];
            }
        }
    }
    let mut inverse = [[0.0; COUNT]; COUNT];
    for col in 0..count {
        for row in col..count {
            let rhs = if row == col { 1.0 } else { 0.0 };
            inverse[row][col] = (rhs
                - (col..row)
                    .map(|k| lower[row][k] * inverse[k][col])
                    .sum::<f64>())
                / lower[row][row];
        }
    }
    let mut symmetric = [[0.0; COUNT]; COUNT];
    for i in 0..count {
        for j in 0..count {
            symmetric[i][j] = (0..count)
                .map(|k| inverse[i][k] * inverse[j][k] / radii[k].powi(2))
                .sum();
        }
    }
    let (eigenvalues, vectors) = eigensystem(symmetric, count);
    let mut result = [ModeShape::default(); COUNT];
    // ||F|| = 1, so eight bubbles do not receive eight full-strength impulses.
    let force_norm = radii.iter().sum::<f64>().sqrt();
    for mode in 0..count {
        let mut projection = 0.0;
        for i in 0..count {
            let phi = (i..count)
                .map(|k| inverse[k][i] * vectors[k][mode])
                .sum::<f64>();
            projection += phi * radii[i].sqrt() / force_norm;
        }
        assert!(eigenvalues[mode] > 0.0);
        result[mode] = ModeShape {
            frequency_ratio: eigenvalues[mode].sqrt(),
            projection,
        };
    }
    result[..count].sort_by(|a, b| a.frequency_ratio.total_cmp(&b.frequency_ratio));
    result
}

fn geometry() -> ([f64; COUNT], [[f64; 3]; COUNT]) {
    let mut radii: [f64; COUNT] = [0.89, 1.04, 0.96, 1.12, 1.01, 0.93, 1.08, 0.98];
    let volume_radius = radii.iter().map(|r| r.powi(3)).sum::<f64>().cbrt();
    for radius in &mut radii {
        *radius /= volume_radius;
    }
    // Centre separation exceeds every pair's summed radii. A moderately
    // compact packet deliberately makes collective inertia audible.
    let spacing = 3.0 * radii.iter().copied().fold(0.0, f64::max);
    let positions = std::array::from_fn(|i| {
        [
            if i & 1 == 0 { 0.0 } else { spacing },
            if i & 2 == 0 { 0.0 } else { spacing },
            if i & 4 == 0 { 0.0 } else { spacing },
        ]
    });
    (radii, positions)
}

#[derive(Clone, Copy, Default)]
struct Mode {
    // z = omega * displacement; mechanical energy = (z^2 + v^2) / 2.
    z: f64,
    v: f64,
    zz: f64,
    zv: f64,
    vz: f64,
    vv: f64,
    omega: f64,
    beta: f64,
    pickup: f64,
}

impl Mode {
    #[inline]
    fn advance(&mut self) -> f64 {
        let acceleration = (-self.omega * self.z - 2.0 * self.beta * self.v) * self.pickup;
        (self.z, self.v) = (
            self.zz * self.z + self.zv * self.v,
            self.vz * self.z + self.vv * self.v,
        );
        acceleration
    }

    #[cfg(test)]
    fn energy(&self) -> f64 {
        0.5 * (self.z * self.z + self.v * self.v)
    }
}

/// One bounded eight-bubble packet. Construct once, then reuse with `spawn`.
/// All eigensolves occur at construction; audio performs eight exact 2x2
/// damped oscillator updates per sample, with no allocation or time substeps.
#[derive(Clone)]
pub struct BubbleCloud {
    shapes: [ModeShape; COUNT],
    modes: [Mode; COUNT],
    sr: f64,
    remaining: usize,
    pan: f32,
    output_scale: f64,
}

impl BubbleCloud {
    pub fn new(sr: f32) -> Self {
        Self::with_coupling(sr, 1.0)
    }

    fn with_coupling(sr: f32, coupling: f64) -> Self {
        assert!(sr.is_finite() && sr > 0.0);
        let (radii, positions) = geometry();
        Self {
            shapes: mode_shapes(&radii, &positions, coupling),
            modes: [Mode::default(); COUNT],
            sr: sr as f64,
            remaining: 0,
            pan: 0.5,
            output_scale: 0.0,
        }
    }

    /// Excite one packet with a spatially uniform pressure impulse.
    /// `radius_m` is the *total gas-volume equivalent* radius: sum R_i^3 = R^3.
    /// `energy` is the world's existing linear event-strength convention, not
    /// joules. Its normalized pressure-impulse vector has length `energy`.
    /// Above-band modes retain their derived frequencies but are not excited.
    pub fn spawn(&mut self, radius_m: f32, energy: f32, pan_position: f32) {
        self.remaining = 0;
        self.modes = [Mode::default(); COUNT];
        if !radius_m.is_finite()
            || radius_m <= 0.0
            || !energy.is_finite()
            || !pan_position.is_finite()
        {
            return;
        }
        let omega_ref = TAU * minnaert_hz(radius_m as f64);
        self.pan = pan_position.clamp(0.0, 1.0);
        self.output_scale = OUTPUT_GAIN / omega_ref;
        let mut min_beta = f64::INFINITY;
        for (mode, shape) in self.modes.iter_mut().zip(self.shapes) {
            let omega = shape.frequency_ratio * omega_ref;
            if omega / TAU >= 0.42 * self.sr {
                continue;
            }
            let beta = LOSS_RATIO * omega;
            let damped = (omega * omega - beta * beta).sqrt();
            let decay = (-beta / self.sr).exp();
            let (sin, cos) = (damped / self.sr).sin_cos();
            *mode = Mode {
                z: 0.0,
                v: energy as f64 * shape.projection,
                zz: decay * (cos + beta * sin / damped),
                zv: decay * omega * sin / damped,
                vz: -decay * omega * sin / damped,
                vv: decay * (cos - beta * sin / damped),
                omega,
                beta,
                pickup: shape.projection,
            };
            min_beta = min_beta.min(beta);
        }
        // Retire only after the modal envelope has fallen by >120 dB.
        if min_beta.is_finite() && energy != 0.0 {
            self.remaining = (14.0 * self.sr / min_beta).ceil() as usize;
        }
    }

    pub fn is_active(&self) -> bool {
        self.remaining > 0
    }

    /// Mechanical energy in consistent normalized units, before listening gain.
    #[cfg(test)]
    fn mechanical_energy(&self) -> f64 {
        if !self.is_active() {
            return 0.0;
        }
        self.modes.iter().map(Mode::energy).sum()
    }

    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        if self.remaining == 0 {
            return (0.0, 0.0);
        }
        self.remaining -= 1;
        let acceleration = self.modes.iter_mut().map(Mode::advance).sum::<f64>();
        pan((acceleration * self.output_scale) as f32, self.pan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Radix-two FFT of the actual rendered onset, with rectangular window. The
    /// onset already decays well before the buffer ends; a Hann window would
    /// suppress that transient and bias comparison toward its tail.
    fn power_spectrum(samples: &[f64]) -> Vec<f64> {
        let n = samples.len().next_power_of_two();
        let mut re = vec![0.0; n];
        let mut im = vec![0.0; n];
        re[..samples.len()].copy_from_slice(samples);
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                re.swap(i, j);
            }
        }
        let mut width = 2;
        while width <= n {
            let (step_im, step_re) = (-TAU / width as f64).sin_cos();
            for start in (0..n).step_by(width) {
                let (mut wr, mut wi) = (1.0, 0.0);
                for i in start..start + width / 2 {
                    let k = i + width / 2;
                    let (tr, ti) = (wr * re[k] - wi * im[k], wr * im[k] + wi * re[k]);
                    (re[k], im[k]) = (re[i] - tr, im[i] - ti);
                    (re[i], im[i]) = (re[i] + tr, im[i] + ti);
                    (wr, wi) = (wr * step_re - wi * step_im, wr * step_im + wi * step_re);
                }
            }
            width *= 2;
        }
        (0..=n / 2).map(|i| re[i] * re[i] + im[i] * im[i]).collect()
    }

    #[test]
    fn single_bubble_recovers_minnaert_and_pair_has_collective_low_mode() {
        let single = mode_shapes(&[1.0], &[[0.0, 0.0, 0.0]], 1.0);
        assert!((single[0].frequency_ratio - 1.0).abs() < 1e-12);
        assert!((minnaert_hz(0.001) - 3283.2434).abs() < 0.01);
        let pair = mode_shapes(&[1.0, 1.0], &[[0.0, 0.0, 0.0], [3.0, 0.0, 0.0]], 1.0);
        assert!((pair[0].frequency_ratio - (1.0f64 + 1.0 / 3.0).sqrt().recip()).abs() < 1e-12);
        assert!((pair[1].frequency_ratio - (1.0f64 - 1.0 / 3.0).sqrt().recip()).abs() < 1e-12);
        // A uniform pressure impulse excites the symmetric mode only.
        assert!(pair[0].projection.abs() > 0.8);
        assert!(pair[1].projection.abs() < 1e-12);
    }

    #[test]
    fn packet_preserves_gas_volume_and_splits_a_normalized_impulse() {
        let (radii, positions) = geometry();
        assert!((radii.iter().map(|r| r.powi(3)).sum::<f64>() - 1.0).abs() < 1e-12);
        let modes = mode_shapes(&radii, &positions, 0.0);
        assert!((modes.iter().map(|m| m.projection.powi(2)).sum::<f64>() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn exact_modal_update_is_passive_and_finite_across_radii_and_sample_rates() {
        for sr in [22_050.0, 44_100.0, 48_000.0, 96_000.0] {
            for radius in [0.00001, 0.0005, 0.003, 0.01, 0.05] {
                let mut cloud = BubbleCloud::new(sr);
                cloud.spawn(radius, 1.0, 0.5);
                let start = cloud.mechanical_energy();
                let mut previous = start;
                for _ in 0..(sr * 2.0) as usize {
                    let (l, r) = cloud.process();
                    let energy = cloud.mechanical_energy();
                    assert!(l.is_finite() && r.is_finite() && energy.is_finite());
                    assert!(energy <= previous + 1e-13, "unforced energy increased");
                    previous = energy;
                }
                assert!(previous < start * 0.01 + 1e-20);
            }
        }
    }

    #[test]
    fn rendered_collective_peak_lies_below_every_isolated_member() {
        let mut uncoupled = BubbleCloud::with_coupling(48_000.0, 0.0);
        let mut coupled = BubbleCloud::new(48_000.0);
        uncoupled.spawn(0.006, 1.0, 0.5);
        coupled.spawn(0.006, 1.0, 0.5);
        let mut signals = [Vec::new(), Vec::new()];
        for _ in 0..16_384 {
            let a = uncoupled.process().0;
            let b = coupled.process().0;
            assert!(a.abs() < 1.0 && b.abs() < 1.0, "comparison must not clip");
            signals[0].push(a as f64);
            signals[1].push(b as f64);
        }
        let peaks = signals.map(|s| {
            let p = power_spectrum(&s);
            let bin = (1..p.len()).max_by(|a, b| p[*a].total_cmp(&p[*b])).unwrap();
            bin as f64 * 48_000.0 / 16_384.0
        });
        let lowest_isolated = uncoupled.shapes[0].frequency_ratio * minnaert_hz(0.006);
        assert!(peaks[1] < lowest_isolated * 0.8);
        assert!(peaks[1] < peaks[0] * 0.8);
    }
}
