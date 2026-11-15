//! Linear sRGB anti-aliasing unmixing for `Transition` pixels.

use kurva_types::ColorRgb;

use crate::gradient::{GradientMap, PixelKind};
use crate::palette::{nearest_centroid, PaletteModel};

/// Maximum perpendicular residual in sRGB [0..255] space for a valid AA unmix match.
const AA_RESIDUAL_TOL_SQ: f32 = 28.0 * 28.0;

#[inline]
fn to_f32_rgb(c: ColorRgb) -> [f32; 3] {
    [f32::from(c.r), f32::from(c.g), f32::from(c.b)]
}

/// Assigns palette indices to `FlatInterior` pixels via Oklab nearest-neighbor,
/// and unmixes `Transition` pixels along the gradient normal in linear sRGB space.
pub fn assign_indices(
    lab: &[[f32; 3]],
    rgb_bytes: &[u8],
    grad_map: &GradientMap,
    palette: &PaletteModel,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let kinds = &grad_map.kinds;
    let total = width * height;
    let mut indices = vec![0_u8; total];

    for i in 0..total {
        if kinds[i] == PixelKind::FlatInterior {
            indices[i] = nearest_centroid(lab[i], &palette.lab) as u8;
        }
    }

    let mut candidates = Vec::with_capacity(9);
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if kinds[idx] != PixelKind::Transition {
                continue;
            }

            let base = idx * 3;
            let c = [
                f32::from(rgb_bytes[base]),
                f32::from(rgb_bytes[base + 1]),
                f32::from(rgb_bytes[base + 2]),
            ];

            if grad_map.magnitudes[idx] > 1e-6 {
                if let Some((la, lb)) = find_opposing_labels(
                    (x, y),
                    (width, height),
                    kinds,
                    &indices,
                    grad_map.normals[idx],
                ) {
                    if let Some(snapped) = unmix_srgb_pair(c, &[la, lb], &palette.rgb) {
                        indices[idx] = snapped;
                        continue;
                    }
                }
            }

            collect_interior_candidates((x, y), (width, height), kinds, &indices, &mut candidates);
            if candidates.len() >= 2 {
                if let Some(snapped) = unmix_srgb_pair(c, &candidates, &palette.rgb) {
                    indices[idx] = snapped;
                    continue;
                }
            }
            indices[idx] = nearest_centroid(lab[idx], &palette.lab) as u8;
        }
    }

    indices
}

fn step_along_normal(
    center: (usize, usize),
    dims: (usize, usize),
    kinds: &[PixelKind],
    indices: &[u8],
    dir: [f32; 2],
) -> Option<u8> {
    let (cx, cy) = (center.0 as f32, center.1 as f32);
    let (w, h) = (dims.0 as isize, dims.1 as isize);
    for step in [1.0_f32, 2.0_f32] {
        let px = (cx + step * dir[0]).round() as isize;
        let py = (cy + step * dir[1]).round() as isize;
        if px < 0 || py < 0 || px >= w || py >= h {
            continue;
        }
        let n_idx = py as usize * dims.0 + px as usize;
        if kinds[n_idx] == PixelKind::FlatInterior {
            return Some(indices[n_idx]);
        }
    }
    None
}

fn find_opposing_labels(
    center: (usize, usize),
    dims: (usize, usize),
    kinds: &[PixelKind],
    indices: &[u8],
    normal: [f32; 2],
) -> Option<(u8, u8)> {
    if normal[0] * normal[0] + normal[1] * normal[1] < 0.25 {
        return None;
    }
    let pos = step_along_normal(center, dims, kinds, indices, normal)?;
    let neg = step_along_normal(center, dims, kinds, indices, [-normal[0], -normal[1]])?;
    (pos != neg).then_some((pos, neg))
}

fn collect_interior_candidates(
    center: (usize, usize),
    dims: (usize, usize),
    kinds: &[PixelKind],
    indices: &[u8],
    out: &mut Vec<u8>,
) {
    let (cx, cy) = center;
    let (width, height) = dims;
    out.clear();
    let x0 = cx.saturating_sub(1);
    let x1 = (cx + 1).min(width - 1);
    let y0 = cy.saturating_sub(1);
    let y1 = (cy + 1).min(height - 1);

    for ny in y0..=y1 {
        for nx in x0..=x1 {
            let n_idx = ny * width + nx;
            if kinds[n_idx] == PixelKind::FlatInterior {
                let label = indices[n_idx];
                if !out.contains(&label) {
                    out.push(label);
                }
            }
        }
    }
}

/// Projects `c` onto every candidate segment `AB` in sRGB space and snaps to the
/// closer endpoint if the perpendicular residual is within `AA_RESIDUAL_TOL_SQ`.
fn unmix_srgb_pair(c: [f32; 3], candidates: &[u8], palette_rgb: &[ColorRgb]) -> Option<u8> {
    let mut best_res_sq = f32::INFINITY;
    let mut best_label = 0_u8;

    for i in 0..candidates.len() {
        let la = candidates[i];
        let a = to_f32_rgb(palette_rgb[usize::from(la)]);
        for &lb in &candidates[(i + 1)..] {
            let b = to_f32_rgb(palette_rgb[usize::from(lb)]);
            let ab = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
            let norm_sq = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
            if norm_sq < 1e-6 {
                continue;
            }

            let cb = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
            let dot = cb[0] * ab[0] + cb[1] * ab[1] + cb[2] * ab[2];
            let alpha = (dot / norm_sq).clamp(0.0, 1.0);

            let res = [
                c[0] - (b[0] + alpha * ab[0]),
                c[1] - (b[1] + alpha * ab[1]),
                c[2] - (b[2] + alpha * ab[2]),
            ];
            let res_sq = res[0] * res[0] + res[1] * res[1] + res[2] * res[2];
            if res_sq < best_res_sq {
                best_res_sq = res_sq;
                best_label = if alpha >= 0.5 { la } else { lb };
            }
        }
    }

    (best_res_sq <= AA_RESIDUAL_TOL_SQ).then_some(best_label)
}
