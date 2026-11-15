//! Oklab perceptual gradient estimation and flat-interior vs. transition classification.

/// Classification of a pixel's local neighborhood in Oklab space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelKind {
    /// Pixel lies inside a uniform or smoothly consistent region.
    FlatInterior,
    /// Pixel lies on an anti-aliased edge or color transition ramp.
    Transition,
}

/// Per-pixel interior/transition classification.
pub struct GradientMap {
    pub kinds: Vec<PixelKind>,
}

/// Base Oklab gradient magnitude threshold separating flat interiors from edges.
const BASE_GRAD_THRESHOLD: f32 = 0.055;

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

/// Computes normalized 3x3 Sobel gradient magnitude in Oklab space.
fn sobel_magnitude(lab: &[[f32; 3]], x: isize, y: isize, width: usize, height: usize) -> f32 {
    let tl = at_clamped(lab, x - 1, y - 1, width, height);
    let tc = at_clamped(lab, x, y - 1, width, height);
    let tr = at_clamped(lab, x + 1, y - 1, width, height);
    let ml = at_clamped(lab, x - 1, y, width, height);
    let mr = at_clamped(lab, x + 1, y, width, height);
    let bl = at_clamped(lab, x - 1, y + 1, width, height);
    let bc = at_clamped(lab, x, y + 1, width, height);
    let br = at_clamped(lab, x + 1, y + 1, width, height);

    let mut grad_sq = 0.0_f32;
    for c in 0..3 {
        let gx = (tr[c] + 2.0 * mr[c] + br[c] - tl[c] - 2.0 * ml[c] - bl[c]) * 0.25;
        let gy = (bl[c] + 2.0 * bc[c] + br[c] - tl[c] - 2.0 * tc[c] - tr[c]) * 0.25;
        grad_sq += gx * gx + gy * gy;
    }
    grad_sq.sqrt()
}

/// Returns `true` if `center` lies strictly between at least one pair of opposite neighbors.
///
/// This distinguishes true anti-aliased transition pixels from crisp 1-2 px dots or corners,
/// which are local extrema in Oklab space and must remain `FlatInterior` seeds.
fn is_intermediate(lab: &[[f32; 3]], x: isize, y: isize, width: usize, height: usize) -> bool {
    let c = at_clamped(lab, x, y, width, height);
    let pairs = [
        (at_clamped(lab, x - 1, y, width, height), at_clamped(lab, x + 1, y, width, height)),
        (at_clamped(lab, x, y - 1, width, height), at_clamped(lab, x, y + 1, width, height)),
        (at_clamped(lab, x - 1, y - 1, width, height), at_clamped(lab, x + 1, y + 1, width, height)),
        (at_clamped(lab, x + 1, y - 1, width, height), at_clamped(lab, x - 1, y + 1, width, height)),
    ];
    for (p1, p2) in pairs {
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
    let mut magnitudes = Vec::with_capacity(total);
    let mut kinds = Vec::with_capacity(total);
    let mut flat_count = 0_usize;

    for y in 0..height {
        for x in 0..width {
            let xi = x as isize;
            let yi = y as isize;
            let mag = sobel_magnitude(lab, xi, yi, width, height);
            magnitudes.push(mag);
            let kind = if mag > BASE_GRAD_THRESHOLD && is_intermediate(lab, xi, yi, width, height) {
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
        for (kind, &mag) in kinds.iter_mut().zip(magnitudes.iter()) {
            *kind = if mag <= adaptive_thresh {
                PixelKind::FlatInterior
            } else {
                PixelKind::Transition
            };
        }
    }

    GradientMap { kinds }
}
