//! Phase-field relaxation with tangent-space projected gradient flow.
//!
//! The chemical potential is projected onto the simplex tangent space before the
//! reaction substep, so the per-pixel mass is conserved exactly.

use crate::oklab::delta_e_ok;
use crate::stencil::{neighbour, StencilField};

/// Diffuse interface width at the finest resolution.
pub const EPSILON: f32 = 0.02;
/// Fidelity weight pulling every phase towards its palette centroid.
pub const LAMBDA: f32 = 40.0;
/// Explicit reaction step size of the Strang splitting.
pub const TIME_STEP: f32 = 0.004;
/// Gauss-Seidel sweeps inside the implicit diffusion substep.
pub const SWEEPS: usize = 2;
/// Temperature of the softmax seeding the simplex from a palette.
pub const INIT_TEMPERATURE: f32 = 0.02;

/// Projects `mu` onto the tangent space of the simplex: `sum_i mu_i = 0`.
pub fn project_tangent(mu: &mut [f64]) {
    if mu.is_empty() {
        return;
    }
    let mean = mu.iter().sum::<f64>() / mu.len() as f64;
    for value in mu.iter_mut() {
        *value -= mean;
    }
}

/// Density fields `phi_i` of `k` phases over a `w x h` pixel grid.
#[derive(Clone, Debug)]
pub struct PhaseField {
    pub w: usize,
    pub h: usize,
    pub k: usize,
    pub phi: Vec<f32>,
}

impl PhaseField {
    /// Total density carried by every phase.
    pub fn mass(&self) -> Vec<f32> {
        let mut mass = vec![0.0f32; self.k];
        for chunk in self.phi.chunks_exact(self.k) {
            for (slot, value) in mass.iter_mut().zip(chunk.iter()) {
                *slot += *value;
            }
        }
        mass
    }

    /// Renormalises every pixel onto the unit simplex.
    pub fn normalise(&mut self) {
        for chunk in self.phi.chunks_exact_mut(self.k) {
            let total: f32 = chunk.iter().sum();
            if total > 0.0 {
                for value in chunk.iter_mut() {
                    *value /= total;
                }
            } else {
                chunk[0] = 1.0;
            }
        }
    }

    /// True when one phase has swallowed more than `threshold` of the mass.
    pub fn collapsed(&self, threshold: f32) -> bool {
        let mass = self.mass();
        let total: f32 = mass.iter().sum();
        total > 0.0 && mass.iter().copied().fold(0.0f32, f32::max) / total > threshold
    }
}

/// Derivative of the double-well potential `W(phi) = phi^2 (1 - phi)^2`.
pub fn double_well_derivative(value: f32) -> f32 {
    2.0 * value * (1.0 - value) * (1.0 - 2.0 * value)
}

/// Euclidean projection of every pixel onto the probability simplex.
pub fn project_simplex(field: &mut PhaseField) {
    let mut scratch = vec![0.0f32; field.k];
    for chunk in field.phi.chunks_exact_mut(field.k) {
        let total: f32 = chunk.iter().sum();
        if chunk.iter().all(|value| *value >= 0.0) && (total - 1.0).abs() <= 1e-6 {
            continue;
        }
        duchi(chunk, &mut scratch);
    }
}

/// Duchi projection of one simplex row, used when clipping pushes a phase below zero.
fn duchi(values: &mut [f32], scratch: &mut [f32]) {
    let count = values.len();
    scratch[..count].copy_from_slice(values);
    scratch[..count].sort_by(|left, right| right.total_cmp(left));
    let (mut cumulative, mut theta) = (0.0f32, 0.0f32);
    for (rank, value) in scratch[..count].iter().enumerate() {
        cumulative += *value;
        let candidate = (cumulative - 1.0) / (rank as f32 + 1.0);
        if *value - candidate > 0.0 {
            theta = candidate;
        }
    }
    for value in values.iter_mut() {
        *value = (*value - theta).max(0.0);
    }
    let total: f32 = values.iter().sum();
    if total > 0.0 {
        for value in values.iter_mut() {
            *value /= total;
        }
    }
}

/// Implicit diffusion substep, Gauss-Seidel sweeps of `(I - eps dt L) phi* = phi`.
fn diffuse(field: &mut PhaseField, stencil: &StencilField, eps: f32, mask: Option<&[bool]>) {
    let (w, h, k) = (field.w, field.h, field.k);
    let factor = eps * TIME_STEP;
    for _ in 0..SWEEPS {
        for y in 0..h {
            for x in 0..w {
                let pixel = y * w + x;
                if !mask.is_none_or(|active| active[pixel]) {
                    continue;
                }
                let row = stencil.rows[pixel];
                let links: [(usize, f32); 8] = std::array::from_fn(|slot| {
                    let other = neighbour(w, h, x, y, slot).unwrap_or(usize::MAX);
                    (other, row.w[slot])
                });
                let total: f32 = links
                    .iter()
                    .filter(|(other, _)| *other != usize::MAX)
                    .map(|(_, weight)| *weight)
                    .sum();
                let denominator = 1.0 + factor * total;
                let base = pixel * k;
                for index in 0..k {
                    let mut acc = field.phi[base + index];
                    for (other, weight) in links.iter() {
                        if *other != usize::MAX && *weight > 0.0 {
                            acc += factor * weight * field.phi[other * k + index];
                        }
                    }
                    field.phi[base + index] = acc / denominator;
                }
            }
        }
    }
}

/// Explicit reaction substep driven by the tangent-space projected potential.
fn react(
    field: &mut PhaseField,
    raster: &[[f32; 3]],
    palette: &[[f32; 3]],
    eps: f32,
    mask: Option<&[bool]>,
) {
    let k = field.k;
    let mut distances = vec![0.0f32; k];
    let mut potential = vec![0.0f64; k];
    for pixel in 0..field.w * field.h {
        if !mask.is_none_or(|active| active[pixel]) {
            continue;
        }
        let colour = raster.get(pixel).copied().unwrap_or([0.0f32; 3]);
        for (slot, entry) in distances.iter_mut().zip(palette.iter()) {
            *slot = delta_e_ok(colour, *entry);
        }
        let base = pixel * k;
        let densities = field.phi[base..base + k].iter();
        for ((slot, value), distance) in potential.iter_mut().zip(densities).zip(distances.iter()) {
            *slot = f64::from(double_well_derivative(*value) / eps) + f64::from(LAMBDA * *distance);
        }
        project_tangent(&mut potential);
        for (value, mu) in field.phi[base..base + k].iter_mut().zip(potential.iter()) {
            *value -= TIME_STEP * *mu as f32;
        }
    }
}

/// One Strang-split step at `eps_scale` times the finest interface width.
pub fn step(
    field: &mut PhaseField,
    stencil: &StencilField,
    raster: &[[f32; 3]],
    palette: &[[f32; 3]],
    eps_scale: f32,
    mask: Option<&[bool]>,
) {
    let eps = EPSILON * eps_scale;
    diffuse(field, stencil, eps, mask);
    react(field, raster, palette, eps, mask);
    project_simplex(field);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stencil::Stencil;

    #[test]
    fn tangent_projection_removes_the_simplex_normal() {
        let mut mu = vec![0.37f64, -1.25, 2.5, 0.0, 0.125];
        project_tangent(&mut mu);
        assert!(mu.iter().sum::<f64>().abs() < 1e-12);
    }

    #[test]
    fn tangent_space_mass_conservation() {
        let palette = [[0.2f32, 0.0, 0.0], [0.6, 0.0, 0.0]];
        let raster = vec![[0.25f32, 0.02, -0.01]; 16];
        let mut field = PhaseField {
            w: 4,
            h: 4,
            k: 2,
            phi: vec![0.5; 32],
        };
        let (w, h) = (4, 4);
        let rows = vec![Stencil::default(); 16];
        let stencil = StencilField { w, h, rows };
        (0..4).for_each(|_| step(&mut field, &stencil, &raster, &palette, 1.0, None));
        for chunk in field.phi.chunks(2) {
            assert!((chunk[0] + chunk[1] - 1.0).abs() < 1e-5);
        }
    }
}
