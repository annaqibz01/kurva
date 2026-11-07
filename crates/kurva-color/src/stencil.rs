//! Adaptive triangulation and nine-point M-matrix stiffness assembly.

use crate::tensor::DTensor;

/// Slot of the eastern neighbour.
pub const E: usize = 0;
/// Slot of the western neighbour.
pub const W: usize = 1;
/// Slot of the northern neighbour.
pub const N: usize = 2;
/// Slot of the southern neighbour.
pub const S: usize = 3;
/// Slot of the north-eastern neighbour.
pub const NE: usize = 4;
/// Slot of the north-western neighbour.
pub const NW: usize = 5;
/// Slot of the south-eastern neighbour.
pub const SE: usize = 6;
/// Slot of the south-western neighbour.
pub const SW: usize = 7;

/// Pixel offset of every stencil slot, in the same order as the slot constants.
pub const OFFSETS: [(isize, isize); 8] = [
    (1, 0),
    (-1, 0),
    (0, -1),
    (0, 1),
    (1, -1),
    (-1, -1),
    (1, 1),
    (-1, 1),
];

/// Conductance row of the assembled stiffness operator for one pixel.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stencil {
    /// Conductance towards the eight neighbouring pixels.
    pub w: [f32; 8],
    /// Diagonal entry, always the negated sum of the off-diagonal weights.
    pub center: f32,
}

/// Assembled stiffness operator over a `w x h` grid.
#[derive(Clone, Debug)]
pub struct StencilField {
    pub w: usize,
    pub h: usize,
    pub rows: Vec<Stencil>,
}

/// Flat index of the neighbour of `(x, y)` in `slot`, or `None` outside the grid.
pub fn neighbour(w: usize, h: usize, x: usize, y: usize, slot: usize) -> Option<usize> {
    let (dx, dy) = OFFSETS[slot];
    let nx = x as isize + dx;
    let ny = y as isize + dy;
    if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
        return None;
    }
    Some(ny as usize * w + nx as usize)
}

impl StencilField {
    /// Applies the stiffness operator to one scalar field sampled per pixel.
    pub fn apply(&self, values: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0f32; values.len()];
        for y in 0..self.h {
            for x in 0..self.w {
                let pixel = y * self.w + x;
                let row = self.rows[pixel];
                let mut acc = row.center * values[pixel];
                for (slot, weight) in row.w.iter().enumerate() {
                    if let Some(other) = neighbour(self.w, self.h, x, y, slot) {
                        acc += weight * values[other];
                    }
                }
                out[pixel] = acc;
            }
        }
        out
    }
}

/// Adds one symmetric edge to both of its endpoints.
fn link(rows: &mut [Stencil], a: usize, a_slot: usize, b: usize, b_slot: usize, weight: f32) {
    if weight <= 0.0 {
        return;
    }
    rows[a].w[a_slot] += weight;
    rows[b].w[b_slot] += weight;
}

/// Conductance of a diagonal edge whose direction is `(sign, sign) / sqrt(2)`.
fn diagonal_weight(first: DTensor, second: DTensor, sign: f32) -> f32 {
    let project = |tensor: DTensor| 0.5 * (tensor.dxx + tensor.dyy + 2.0 * sign * tensor.dxy);
    (0.5 * (project(first) + project(second))).max(0.0)
}

/// Assembles the adaptive nine-point stencil field of a diffusion tensor field.
///
/// Every unit cell contributes its two axis edges and one diagonal edge. The
/// diagonal chosen for a cell follows the sign of the cell-averaged `D_xy`, which
/// keeps the triangulation aligned with the dominant edge orientation.
pub fn assemble(field: &[DTensor], w: usize, h: usize) -> StencilField {
    let mut rows = vec![Stencil::default(); w * h];
    for y in 0..h {
        for x in 0..w {
            let pixel = y * w + x;
            if x + 1 < w {
                let east = pixel + 1;
                let weight = 0.5 * (field[pixel].dxx + field[east].dxx);
                link(&mut rows, pixel, E, east, W, weight);
            }
            if y + 1 < h {
                let south = pixel + w;
                let weight = 0.5 * (field[pixel].dyy + field[south].dyy);
                link(&mut rows, pixel, S, south, N, weight);
            }
        }
    }
    for y in 0..h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let pixel = y * w + x;
            let cell = 0.25
                * (field[pixel].dxy
                    + field[pixel + 1].dxy
                    + field[pixel + w].dxy
                    + field[pixel + w + 1].dxy);
            if cell >= 0.0 {
                let opposite = pixel + w + 1;
                let weight = diagonal_weight(field[pixel], field[opposite], 1.0);
                link(&mut rows, pixel, SE, opposite, NW, weight);
            } else {
                let corner = pixel + 1;
                let opposite = pixel + w;
                let weight = diagonal_weight(field[corner], field[opposite], -1.0);
                link(&mut rows, corner, SW, opposite, NE, weight);
            }
        }
    }
    for row in rows.iter_mut() {
        row.center = -row.w.iter().sum::<f32>();
    }
    StencilField { w, h, rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_field(w: usize, h: usize) -> Vec<DTensor> {
        (0..w * h)
            .map(|pixel| {
                let dxy = ((pixel * 7 + 3) % 5) as f32 * 0.02 - 0.04;
                DTensor {
                    dxx: 1.0,
                    dxy,
                    dyy: 1.0,
                }
            })
            .collect()
    }

    #[test]
    fn stencil_m_matrix() {
        let (w, h) = (9, 7);
        let stencil = assemble(&sample_field(w, h), w, h);
        for row in stencil.rows.iter() {
            assert!(row.center <= 0.0);
            for weight in row.w.iter() {
                assert!(*weight >= 0.0);
            }
            let sum = row.center + row.w.iter().sum::<f32>();
            assert!(sum.abs() < 1e-5);
        }
    }

    #[test]
    fn stiffness_annihilates_constant_fields() {
        let (w, h) = (6, 5);
        let stencil = assemble(&sample_field(w, h), w, h);
        for value in stencil.apply(&vec![1.0f32; w * h]) {
            assert!(value.abs() < 1e-5);
        }
    }
}
