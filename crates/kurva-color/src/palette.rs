//! Palette management: stratified seeding, centroid updates, annihilation, coalescence.

use crate::oklab::delta_e_ok;
use crate::phase::PhaseField;

/// Phases carrying less than this share of the image are removed.
pub const TAU_AREA_RATIO: f32 = 0.002;
/// Just-noticeable Oklab distance; closer centroids coalesce into one phase.
pub const TAU_JND: f32 = 1.0 / 255.0;

/// Deterministic seeding by equal-mass strata along the Oklab lightness axis.
pub fn seed(raster: &[[f32; 3]], k: usize) -> Vec<[f32; 3]> {
    let count = raster.len();
    if count == 0 || k == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|left, right| raster[*left][0].total_cmp(&raster[*right][0]));
    (0..k)
        .map(|stratum| {
            let position = (stratum as f32 + 0.5) / k as f32 * count as f32;
            raster[order[(position as usize).min(count - 1)]]
        })
        .collect()
}

/// Seeds the simplex from a palette with a sharp softmax over `Delta E_OK`.
pub fn initialise(
    raster: &[[f32; 3]],
    dims: (usize, usize),
    palette: &[[f32; 3]],
    temperature: f32,
) -> PhaseField {
    let (w, h) = dims;
    let k = palette.len().max(1);
    let mut phi = vec![0.0f32; w * h * k];
    let inverse = 1.0 / temperature.max(f32::EPSILON);
    for (pixel, colour) in raster.iter().enumerate().take(w * h) {
        let base = pixel * k;
        let best = palette
            .iter()
            .fold(f32::INFINITY, |b, e| b.min(delta_e_ok(*colour, *e)));
        let mut total = 0.0;
        for (slot, entry) in phi[base..base + k].iter_mut().zip(palette.iter()) {
            *slot = (-(delta_e_ok(*colour, *entry) - best) * inverse).exp();
            total += *slot;
        }
        if total > 0.0 {
            for slot in phi[base..base + k].iter_mut() {
                *slot /= total;
            }
        } else {
            phi[base] = 1.0;
        }
    }
    PhaseField { w, h, k, phi }
}

/// Mass weighted centroid of every phase, keeping the previous estimate when a
/// phase has been emptied by the simplex projection.
pub fn centroids(field: &PhaseField, raster: &[[f32; 3]], palette: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let k = field.k;
    let mut sums = vec![[0.0f32; 3]; k];
    let mut mass = vec![0.0f32; k];
    for (pixel, colour) in raster.iter().enumerate() {
        let base = pixel * k;
        for index in 0..k {
            let weight = field.phi[base + index];
            if weight > 0.0 {
                mass[index] += weight;
                for (slot, value) in sums[index].iter_mut().zip(colour.iter()) {
                    *slot += weight * value;
                }
            }
        }
    }
    let estimate = |index: usize| {
        let total = mass[index];
        let sum = sums[index];
        if total > 1e-6 {
            [sum[0] / total, sum[1] / total, sum[2] / total]
        } else {
            palette.get(index).copied().unwrap_or([0.0; 3])
        }
    };
    (0..k).map(estimate).collect()
}

/// Drops negligible phases and folds perceptually indistinguishable ones together.
pub fn condense(field: &mut PhaseField, palette: &mut Vec<[f32; 3]>, tau_area: f32) {
    let k = field.k;
    if k == 0 || palette.len() < k {
        return;
    }
    let mass = field.mass();
    let heaviest = (0..k).fold(0, |b, i| if mass[i] > mass[b] { i } else { b });
    let survivors: Vec<usize> = (0..k)
        .filter(|index| mass[*index] >= tau_area || *index == heaviest)
        .collect();
    let mut representative = vec![usize::MAX; k];
    let mut merged: Vec<usize> = Vec::new();
    for &index in survivors.iter() {
        let target = merged
            .iter()
            .find(|kept| delta_e_ok(palette[index], palette[**kept]) < TAU_JND)
            .copied()
            .unwrap_or(index);
        representative[index] = target;
        if target == index {
            merged.push(index);
        }
    }
    if merged.len() == k {
        return;
    }
    let mut map = vec![usize::MAX; k];
    for (position, &index) in merged.iter().enumerate() {
        map[index] = position;
    }
    for &index in survivors.iter() {
        map[index] = map[representative[index]];
    }
    let kept: Vec<[f32; 3]> = merged.iter().map(|&index| palette[index]).collect();
    *field = remap(field, &map, merged.len());
    *palette = kept;
    field.normalise();
}

/// Re-indexes the phases through `map`, accumulating the merged densities.
pub fn remap(field: &PhaseField, map: &[usize], k: usize) -> PhaseField {
    let (w, h, k) = (field.w, field.h, k.max(1));
    let mut phi = vec![0.0f32; field.w * field.h * k];
    for pixel in 0..field.w * field.h {
        let base = pixel * field.k;
        let target = pixel * k;
        for (index, value) in field.phi[base..base + field.k].iter().enumerate() {
            if let Some(slot) = map.get(index).copied().filter(|slot| *slot < k) {
                phi[target + slot] += *value;
            }
        }
    }
    PhaseField { w, h, k, phi }
}

/// Sharp labels `pi(x) = argmax_i phi_i(x)`.
pub fn labels(field: &PhaseField) -> Vec<u8> {
    let mut labels = Vec::with_capacity(field.w * field.h);
    for row in field.phi.chunks_exact(field.k) {
        let peak = row.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1));
        labels.push(peak.map_or(0, |(index, _)| index).min(255) as u8);
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::INIT_TEMPERATURE;

    #[test]
    fn condense_merges_identical_centroids() {
        let mut palette = vec![[0.4f32, 0.0, 0.0], [0.4, 0.0, 0.0], [0.8, 0.0, 0.0]];
        let raster = vec![[0.4f32, 0.0, 0.0]; 9];
        let mut field = initialise(&raster, (3, 3), &palette, INIT_TEMPERATURE);
        condense(&mut field, &mut palette, 1.0);
        assert_eq!((palette.len(), field.k), (1, 1));
        assert!(field.phi.iter().all(|value| (value - 1.0).abs() < 1e-5));
    }

    #[test]
    fn labels_point_at_the_dominant_phase() {
        let palette = vec![[0.2f32, 0.0, 0.0], [0.8, 0.0, 0.0]];
        let raster: Vec<[f32; 3]> = (0..4)
            .map(|i| [if i % 2 == 0 { 0.2 } else { 0.8 }, 0.0, 0.0])
            .collect();
        let field = initialise(&raster, (2, 2), &palette, INIT_TEMPERATURE);
        assert_eq!(labels(&field), vec![0, 1, 0, 1]);
    }
}
