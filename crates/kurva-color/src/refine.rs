//! Contrast-sensitive discrete Iterated Conditional Modes (ICM) cleanup.

use crate::region::dist_sq_oklab;

/// Number of deterministic ICM sweeps.
const ICM_PASSES: usize = 2;

/// Potts smoothness weight balancing data fidelity against speckle removal.
const LAMBDA: f32 = 0.004;

/// Squared Oklab distance below which an assigned pixel is never overwritten by ICM.
const PROTECT_DATA_SQ: f32 = 0.0025;

/// Contrast sensitivity denominator `2 * sigma^2` in Oklab space.
const TWO_SIGMA_SQ: f32 = 2.0 * 0.04 * 0.04;

/// Runs deterministic 8-connected contrast-sensitive ICM over the index map.
///
/// Suppresses isolated compression speckles inside flat fills while preserving
/// sharp corners and 1-2 px thin strokes.
pub fn icm_cleanup(
    lab: &[[f32; 3]],
    palette_lab: &[[f32; 3]],
    thin_strokes: &[bool],
    indices: &mut [u8],
    width: usize,
    height: usize,
) {
    if palette_lab.len() <= 1 || width == 0 || height == 0 {
        return;
    }

    let mut neighbors = Vec::with_capacity(8);
    let mut candidates = Vec::with_capacity(9);

    for _ in 0..ICM_PASSES {
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let curr_label = indices[idx];
                if thin_strokes.get(idx).copied().unwrap_or(false)
                    || dist_sq_oklab(lab[idx], palette_lab[usize::from(curr_label)])
                        < PROTECT_DATA_SQ
                {
                    continue;
                }

                neighbors.clear();
                candidates.clear();
                candidates.push(curr_label);

                let x0 = x.saturating_sub(1);
                let x1 = (x + 1).min(width - 1);
                let y0 = y.saturating_sub(1);
                let y1 = (y + 1).min(height - 1);

                for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        if nx == x && ny == y {
                            continue;
                        }
                        let n_idx = ny * width + nx;
                        let n_label = indices[n_idx];
                        let w = (-dist_sq_oklab(lab[idx], lab[n_idx]) / TWO_SIGMA_SQ).exp();
                        neighbors.push((n_label, w));
                        if !candidates.contains(&n_label) {
                            candidates.push(n_label);
                        }
                    }
                }

                if candidates.len() <= 1 {
                    continue;
                }

                let mut best_label = curr_label;
                let mut best_energy = f32::INFINITY;

                for &cand in &candidates {
                    let data = dist_sq_oklab(lab[idx], palette_lab[usize::from(cand)]);
                    let mut potts = 0.0_f32;
                    for &(n_label, w) in &neighbors {
                        if cand != n_label {
                            potts += w;
                        }
                    }
                    let energy = data + LAMBDA * potts;
                    if energy < best_energy {
                        best_energy = energy;
                        best_label = cand;
                    }
                }

                indices[idx] = best_label;
            }
        }
    }
}
