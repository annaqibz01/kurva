//! Oklab perceptual gradient estimation and flat-interior vs. transition classification.

/// Classification of a pixel's local neighborhood in Oklab space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelKind {
    /// Pixel lies inside a uniform or smoothly consistent region.
    FlatInterior,
    /// Pixel lies on an anti-aliased edge or color transition ramp.
    Transition,
}

/// Per-pixel interior/transition classification, gradient magnitudes, unit normals, and thin-stroke flags.
pub struct GradientMap {
    pub kinds: Vec<PixelKind>,
    pub magnitudes: Vec<f32>,
    pub normals: Vec<[f32; 2]>,
    pub thin_strokes: Vec<bool>,
}

/// Base Oklab gradient magnitude threshold separating flat interiors from edges.
const BASE_GRAD_THRESHOLD: f32 = 0.055;

/// Minimum squared Oklab contrast against both opposing neighbors to qualify as a thin stroke.
const MIN_RIDGE_CONTRAST_SQ: f32 = 0.018;

/// Minimum fraction (percentage) of pixels required to be `FlatInterior`.
const MIN_INTERIOR_PERCENT: usize = 15;

/// Fallback percentile used when flat-interior coverage is below the minimum.
const FALLBACK_PERCENTILE: usize = 25;

#[inline]
fn at_clamped(lab: &[[f32; 3]], x: isize, y: isize, width: usize, height: usize) -> [f32; 3] {
    let cx = x.clamp(0, width as isize - 1) as usize;
    let cy = y.clamp(0, height as isize - 1) as usize;
    lab[cy * width + cx]
}

#[inline]
fn opposing_pairs(
    lab: &[[f32; 3]],
    x: isize,
    y: isize,
    width: usize,
    height: usize,
) -> [([f32; 3], [f32; 3]); 4] {
    [
        (at_clamped(lab, x - 1, y, width, height), at_clamped(lab, x + 1, y, width, height)),
        (at_clamped(lab, x, y - 1, width, height), at_clamped(lab, x, y + 1, width, height)),
        (at_clamped(lab, x - 1, y - 1, width, height), at_clamped(lab, x + 1, y + 1, width, height)),
        (at_clamped(lab, x + 1, y - 1, width, height), at_clamped(lab, x - 1, y + 1, width, height)),
    ]
}

/// Computes normalized 3x3 Sobel gradient magnitude and unit edge normal in Oklab space.
fn sobel(lab: &[[f32; 3]], x: isize, y: isize, width: usize, height: usize) -> (f32, [f32; 2]) {
    let tl = at_clamped(lab, x - 1, y - 1, width, height);
    let tc = at_clamped(lab, x, y - 1, width, height);
    let tr = at_clamped(lab, x + 1, y - 1, width, height);
    let ml = at_clamped(lab, x - 1, y, width, height);
    let mr = at_clamped(lab, x + 1, y, width, height);
    let bl = at_clamped(lab, x - 1, y + 1, width, height);
    let bc = at_clamped(lab, x, y + 1, width, height);
    let br = at_clamped(lab, x + 1, y + 1, width, height);

    let mut gx = [0.0_f32; 3];
    let mut gy = [0.0_f32; 3];
    let mut grad_sq = 0.0_f32;
    let mut best_c = 0_usize;
    let mut best_c_sq = -1.0_f32;

    for c in 0..3 {
        let cx = (tr[c] + 2.0 * mr[c] + br[c] - tl[c] - 2.0 * ml[c] - bl[c]) * 0.25;
        let cy = (bl[c] + 2.0 * bc[c] + br[c] - tl[c] - 2.0 * tc[c] - tr[c]) * 0.25;
        gx[c] = cx;
        gy[c] = cy;
        let c_sq = cx * cx + cy * cy;
        grad_sq += c_sq;
        if c_sq > best_c_sq {
            best_c_sq = c_sq;
            best_c = c;
        }
    }

    let (ref_x, ref_y) = (gx[best_c], gy[best_c]);
    let (mut sx, mut sy) = (0.0_f32, 0.0_f32);
    for c in 0..3 {
        let sign = if gx[c] * ref_x + gy[c] * ref_y >= 0.0 { 1.0 } else { -1.0 };
        sx += sign * gx[c];
        sy += sign * gy[c];
    }
    let norm = (sx * sx + sy * sy).sqrt();
    let normal = if norm > 1e-6 { [sx / norm, sy / norm] } else { [0.0, 0.0] };
    (grad_sq.sqrt(), normal)
}

/// Returns `true` when `(x, y)` is a 1D ridge or valley exceeding `min_contrast_sq` on both sides.
fn is_ridge_or_valley(
    lab: &[[f32; 3]],
    x: isize,
    y: isize,
    width: usize,
    height: usize,
    min_contrast_sq: f32,
) -> bool {
    let c = at_clamped(lab, x, y, width, height);
    for (p_minus, p_plus) in opposing_pairs(lab, x, y, width, height) {
        let dm = [c[0] - p_minus[0], c[1] - p_minus[1], c[2] - p_minus[2]];
        let dp = [c[0] - p_plus[0], c[1] - p_plus[1], c[2] - p_plus[2]];
        let dot = dm[0] * dp[0] + dm[1] * dp[1] + dm[2] * dp[2];
        if dot <= 0.0 {
            continue;
        }
        let dm_sq = dm[0] * dm[0] + dm[1] * dm[1] + dm[2] * dm[2];
        let dp_sq = dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2];
        let span = [p_minus[0] - p_plus[0], p_minus[1] - p_plus[1], p_minus[2] - p_plus[2]];
        let span_sq = span[0] * span[0] + span[1] * span[1] + span[2] * span[2];
        if dm_sq >= min_contrast_sq
            && dp_sq >= min_contrast_sq
            && span_sq < dm_sq
            && span_sq < dp_sq
        {
            return true;
        }
    }
    false
}

/// Returns `true` if `center` lies strictly between at least one pair of opposite neighbors.
fn is_intermediate(lab: &[[f32; 3]], x: isize, y: isize, width: usize, height: usize) -> bool {
    let c = at_clamped(lab, x, y, width, height);
    for (p1, p2) in opposing_pairs(lab, x, y, width, height) {
        let dot = (p1[0] - c[0]) * (p2[0] - c[0])
            + (p1[1] - c[1]) * (p2[1] - c[1])
            + (p1[2] - c[2]) * (p2[2] - c[2]);
        if dot < -1e-5 {
            return true;
        }
    }
    false
}

/// Classifies every pixel into `FlatInterior` or `Transition` with adaptive fallback.
pub fn classify(lab: &[[f32; 3]], width: usize, height: usize) -> GradientMap {
    let total = width * height;
    let edge_sq = BASE_GRAD_THRESHOLD * BASE_GRAD_THRESHOLD;
    let mut magnitudes = Vec::with_capacity(total);
    let mut normals = Vec::with_capacity(total);
    let mut kinds = Vec::with_capacity(total);
    let mut thin_strokes = Vec::with_capacity(total);
    let mut flat_count = 0_usize;

    for y in 0..height {
        for x in 0..width {
            let xi = x as isize;
            let yi = y as isize;
            let (mag, normal) = sobel(lab, xi, yi, width, height);
            magnitudes.push(mag);
            normals.push(normal);
            let strong_ridge = is_ridge_or_valley(lab, xi, yi, width, height, MIN_RIDGE_CONTRAST_SQ);
            let weak_ripple =
                !strong_ridge && is_ridge_or_valley(lab, xi, yi, width, height, edge_sq);
            thin_strokes.push(strong_ridge);
            let kind = if !strong_ridge
                && (weak_ripple
                    || (mag > BASE_GRAD_THRESHOLD && is_intermediate(lab, xi, yi, width, height)))
            {
                PixelKind::Transition
            } else {
                flat_count += 1;
                PixelKind::FlatInterior
            };
            kinds.push(kind);
        }
    }

    if total > 0 && flat_count * 100 < total * MIN_INTERIOR_PERCENT {
        let mut sorted = magnitudes.clone();
        sorted.sort_by(f32::total_cmp);
        let pct_idx = (total * FALLBACK_PERCENTILE / 100).min(total - 1);
        let adaptive_thresh = sorted[pct_idx].max(BASE_GRAD_THRESHOLD);
        for ((kind, &mag), &ridge) in kinds.iter_mut().zip(magnitudes.iter()).zip(thin_strokes.iter()) {
            *kind = if ridge || mag <= adaptive_thresh {
                PixelKind::FlatInterior
            } else {
                PixelKind::Transition
            };
        }
    }

    GradientMap {
        kinds,
        magnitudes,
        normals,
        thin_strokes,
    }
}
