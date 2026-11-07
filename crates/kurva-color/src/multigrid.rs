//! Full multigrid hierarchy and the FMG V-cycle schedule of the relaxation.

use crate::phase::PhaseField;

/// Deepest pyramid supported: level three is `W/8 x H/8`.
pub const MAX_LEVELS: usize = 4;
/// Smoothing steps per level, finest first.
pub const SMOOTH_STEPS: [usize; MAX_LEVELS] = [3, 5, 15, 40];
/// Interface width multiplier per level, finest first.
pub const EPSILON_SCALE: [f32; MAX_LEVELS] = [1.0, 1.0, 2.0, 4.0];
/// Pixels this close to a decision boundary stay in the active set.
pub const ACTIVE_ETA: f32 = 0.05;

/// One resolution of the Oklab pyramid.
#[derive(Clone, Debug)]
pub struct Level {
    pub w: usize,
    pub h: usize,
    pub data: Vec<[f32; 3]>,
}

/// Coarsened Oklab rasters, index zero being the input resolution.
#[derive(Clone, Debug)]
pub struct Pyramid {
    pub levels: Vec<Level>,
}

impl Pyramid {
    /// Builds the hierarchy by `2 x 2` block averaging.
    ///
    /// A level is only added while it still holds at least `min_pixels` pixels,
    /// so every level can carry all palette phases without degenerating.
    pub fn build(data: Vec<[f32; 3]>, w: usize, h: usize, min_pixels: usize) -> Self {
        let mut levels = vec![Level { w, h, data }];
        while levels.len() < MAX_LEVELS {
            let last = levels.len() - 1;
            let (lw, lh) = (levels[last].w, levels[last].h);
            if lw < 2 || lh < 2 || (lw / 2) * (lh / 2) < min_pixels {
                break;
            }
            let data = downsample(&levels[last].data, lw, lh);
            levels.push(Level {
                w: lw / 2,
                h: lh / 2,
                data,
            });
        }
        Self { levels }
    }

    /// Index of the coarsest available level, `Omega_3` for a full pyramid.
    pub fn coarsest(&self) -> usize {
        self.levels.len() - 1
    }
}

/// `2 x 2` block average, the restriction operator of the Oklab pyramid.
fn downsample(data: &[[f32; 3]], w: usize, h: usize) -> Vec<[f32; 3]> {
    let (dw, dh) = (w / 2, h / 2);
    let mut out = Vec::with_capacity(dw * dh);
    for y in 0..dh {
        for x in 0..dw {
            let mut acc = [0.0f32; 3];
            for dy in 0..2 {
                for dx in 0..2 {
                    let pixel = (2 * y + dy) * w + 2 * x + dx;
                    for (slot, value) in acc.iter_mut().zip(data[pixel].iter()) {
                        *slot += 0.25 * value;
                    }
                }
            }
            out.push(acc);
        }
    }
    out
}

/// `2 x 2` block restriction of the phase densities, preserving partition of unity.
pub fn restrict(field: &PhaseField) -> PhaseField {
    let (w, h) = (field.w / 2, field.h / 2);
    if w == 0 || h == 0 {
        return field.clone();
    }
    let k = field.k;
    let mut phi = vec![0.0f32; w * h * k];
    for y in 0..h {
        for x in 0..w {
            let target = (y * w + x) * k;
            for dy in 0..2 {
                for dx in 0..2 {
                    let source = ((2 * y + dy) * field.w + 2 * x + dx) * k;
                    for index in 0..k {
                        phi[target + index] += 0.25 * field.phi[source + index];
                    }
                }
            }
        }
    }
    PhaseField { w, h, k, phi }
}

/// Bilinear prolongation preserving nonnegativity and the simplex sum.
pub fn prolongate(coarse: &PhaseField, w: usize, h: usize) -> PhaseField {
    let k = coarse.k;
    let (cw, ch) = (coarse.w, coarse.h);
    let mut phi = vec![0.0f32; w * h * k];
    for y in 0..h {
        let v = ((y as f32 + 0.5) * 0.5 - 0.5).clamp(0.0, (ch - 1) as f32);
        let y0 = v.floor() as usize;
        let y1 = (y0 + 1).min(ch - 1);
        let ty = v - y0 as f32;
        for x in 0..w {
            let u = ((x as f32 + 0.5) * 0.5 - 0.5).clamp(0.0, (cw - 1) as f32);
            let x0 = u.floor() as usize;
            let x1 = (x0 + 1).min(cw - 1);
            let tx = u - x0 as f32;
            let base = (y * w + x) * k;
            let corners = [
                (y0 * cw + x0) * k,
                (y0 * cw + x1) * k,
                (y1 * cw + x0) * k,
                (y1 * cw + x1) * k,
            ];
            let mut total = 0.0;
            for index in 0..k {
                let top = coarse.phi[corners[0] + index] * (1.0 - tx)
                    + coarse.phi[corners[1] + index] * tx;
                let bottom = coarse.phi[corners[2] + index] * (1.0 - tx)
                    + coarse.phi[corners[3] + index] * tx;
                let value = top * (1.0 - ty) + bottom * ty;
                phi[base + index] = value;
                total += value;
            }
            if total > 0.0 {
                for index in 0..k {
                    phi[base + index] /= total;
                }
            } else {
                phi[base] = 1.0;
            }
        }
    }
    PhaseField { w, h, k, phi }
}

/// Pixels where no phase has reached the confident plateau `1 - eta`.
pub fn active_mask(field: &PhaseField, eta: f32) -> Vec<bool> {
    (0..field.w * field.h)
        .map(|pixel| {
            let base = pixel * field.k;
            let peak = field.phi[base..base + field.k]
                .iter()
                .copied()
                .fold(0.0f32, f32::max);
            peak <= 1.0 - eta
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smeared(w: usize, h: usize, k: usize) -> PhaseField {
        let mut phi = vec![0.0f32; w * h * k];
        for pixel in 0..w * h {
            for index in 0..k {
                phi[pixel * k + index] = 0.5 + 0.1 * index as f32;
            }
            let total: f32 = phi[pixel * k..pixel * k + k].iter().sum();
            for index in 0..k {
                phi[pixel * k + index] /= total;
            }
        }
        PhaseField { w, h, k, phi }
    }

    fn assert_simplex(field: &PhaseField) {
        for chunk in field.phi.chunks_exact(field.k) {
            for value in chunk {
                assert!(*value >= 0.0);
            }
            let total: f32 = chunk.iter().sum();
            assert!((total - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn multigrid_simplex_preservation() {
        let fine = smeared(8, 6, 3);
        let coarse = restrict(&fine);
        assert_eq!((coarse.w, coarse.h, coarse.k), (4, 3, 3));
        assert_simplex(&coarse);

        let restored = prolongate(&coarse, 8, 6);
        assert_simplex(&restored);
        let odd = prolongate(&coarse, 7, 5);
        assert_simplex(&odd);
        assert_eq!(odd.phi.len(), 7 * 5 * 3);
    }

    #[test]
    fn active_set_marks_uncertain_pixels_only() {
        let field = smeared(4, 4, 2);
        assert_eq!(active_mask(&field, ACTIVE_ETA).len(), 16);
        let mut sharp = field.clone();
        for (index, value) in sharp.phi.iter_mut().enumerate() {
            *value = if index % 2 == 0 { 1.0 } else { 0.0 };
        }
        assert!(active_mask(&sharp, ACTIVE_ETA).iter().all(|flag| !flag));
    }
}
