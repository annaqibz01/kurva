//! Pure-region palette clustering in Oklab space.

use kurva_types::ColorRgb;

use crate::oklab;
use crate::region::{dist_sq_oklab, PureRegion};

/// Final quantized palette in both Oklab and sRGB representations.
pub struct PaletteModel {
    pub lab: Vec<[f32; 3]>,
    pub rgb: Vec<ColorRgb>,
}

/// Squared Oklab distance below which two palette entries are merged as identical.
const MERGE_TOL_SQ: f32 = 0.012 * 0.012;

/// Number of weighted Lloyd refinement iterations.
const LLOYD_ITERS: usize = 8;

/// Clusters pure `FlatInterior` region centroids into at most `max_colors` palette entries.
///
/// `Transition` pixels are excluded so anti-aliased edge halos never pollute the palette.
pub fn build(regions: &[PureRegion], fallback_lab: &[[f32; 3]], max_colors: usize) -> PaletteModel {
    if regions.is_empty() {
        let mut lab_entries = Vec::new();
        for &c in fallback_lab {
            if !lab_entries.iter().any(|&e| dist_sq_oklab(e, c) <= MERGE_TOL_SQ) {
                lab_entries.push(c);
                if lab_entries.len() == max_colors {
                    break;
                }
            }
        }
        if lab_entries.is_empty() {
            lab_entries.push([0.0, 0.0, 0.0]);
        }
        let rgb = lab_entries.iter().copied().map(oklab::to_srgb).collect();
        return PaletteModel { lab: lab_entries, rgb };
    }

    let k = max_colors.min(regions.len()).max(1);
    let mut centroids = furthest_point_init(regions, k);
    weighted_lloyd(regions, &mut centroids);
    let pruned = deduplicate_centroids(regions, &centroids);

    let rgb = pruned.iter().copied().map(oklab::to_srgb).collect();
    PaletteModel { lab: pruned, rgb }
}

/// Deterministic weighted furthest-point initialization.
///
/// Starts from the largest-area region, then greedily selects the region maximizing
/// minimum Oklab distance to existing centroids so tiny high-contrast features survive.
fn furthest_point_init(regions: &[PureRegion], k: usize) -> Vec<[f32; 3]> {
    let mut centroids = Vec::with_capacity(k);
    let first_idx = regions
        .iter()
        .enumerate()
        .max_by_key(|&(_, r)| r.pixel_count)
        .map_or(0, |(idx, _)| idx);
    centroids.push(regions[first_idx].mean_lab);

    let mut min_dists: Vec<f32> = regions
        .iter()
        .map(|r| dist_sq_oklab(r.mean_lab, centroids[0]))
        .collect();

    while centroids.len() < k {
        let mut best_idx = 0;
        let mut best_dist = -1.0_f32;
        for (i, &d) in min_dists.iter().enumerate() {
            if d > best_dist {
                best_dist = d;
                best_idx = i;
            }
        }
        if best_dist <= 1e-8 {
            break;
        }
        let next_c = regions[best_idx].mean_lab;
        centroids.push(next_c);
        for (i, r) in regions.iter().enumerate() {
            let d = dist_sq_oklab(r.mean_lab, next_c);
            if d < min_dists[i] {
                min_dists[i] = d;
            }
        }
    }
    centroids
}

/// Refines centroids using `pixel_count`-weighted Lloyd iterations.
fn weighted_lloyd(regions: &[PureRegion], centroids: &mut [[f32; 3]]) {
    let k = centroids.len();
    let mut sums = vec![[0.0_f64; 3]; k];
    let mut weights = vec![0.0_f64; k];

    for _ in 0..LLOYD_ITERS {
        sums.fill([0.0; 3]);
        weights.fill(0.0);

        for r in regions {
            let best = nearest_centroid(r.mean_lab, centroids);
            let w = r.pixel_count as f64;
            sums[best][0] += f64::from(r.mean_lab[0]) * w;
            sums[best][1] += f64::from(r.mean_lab[1]) * w;
            sums[best][2] += f64::from(r.mean_lab[2]) * w;
            weights[best] += w;
        }

        for i in 0..k {
            if weights[i] > 0.0 {
                let inv = 1.0 / weights[i];
                centroids[i] = [
                    (sums[i][0] * inv) as f32,
                    (sums[i][1] * inv) as f32,
                    (sums[i][2] * inv) as f32,
                ];
            }
        }
    }
}

/// Drops empty centroids and merges centroids within `MERGE_TOL_SQ`.
fn deduplicate_centroids(regions: &[PureRegion], centroids: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut counts = vec![0_usize; centroids.len()];
    for r in regions {
        let best = nearest_centroid(r.mean_lab, centroids);
        counts[best] += r.pixel_count;
    }

    let mut order: Vec<usize> = (0..centroids.len()).filter(|&i| counts[i] > 0).collect();
    order.sort_by(|&a, &b| counts[b].cmp(&counts[a]).then_with(|| a.cmp(&b)));

    let mut kept: Vec<[f32; 3]> = Vec::with_capacity(order.len());
    for idx in order {
        let c = centroids[idx];
        if !kept.iter().any(|&k| dist_sq_oklab(k, c) <= MERGE_TOL_SQ) {
            kept.push(c);
        }
    }
    if kept.is_empty() && !centroids.is_empty() {
        kept.push(centroids[0]);
    }
    kept
}

#[inline]
pub(crate) fn nearest_centroid(color: [f32; 3], centroids: &[[f32; 3]]) -> usize {
    let mut best_idx = 0;
    let mut best_dist = f32::INFINITY;
    for (i, &c) in centroids.iter().enumerate() {
        let d = dist_sq_oklab(color, c);
        if d < best_dist {
            best_dist = d;
            best_idx = i;
        }
    }
    best_idx
}
