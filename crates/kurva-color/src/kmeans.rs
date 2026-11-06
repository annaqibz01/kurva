//! Deterministic K-Means clustering over CIELAB samples.

use crate::cielab::{delta_e_squared, LabColor};

/// Iteration cap used by callers that do not supply their own budget.
pub const DEFAULT_MAX_ITERATIONS: usize = 20;

/// Centroid movement in squared CIELAB distance below which the loop stops.
const CONVERGENCE_EPSILON_SQUARED: f64 = 1e-4 * 1e-4;

/// Upper bound imposed by the u8 index buffer of an indexed raster.
const MAX_CLUSTERS: usize = 256;

/// Required iterations are never zero, even when the caller asks for that.
fn effective_iterations(max_iterations: usize) -> usize {
    max_iterations.max(1)
}

/// Seeds centroids from evenly spaced strata of the pixels sorted by lightness.
fn initial_centroids(pixels: &[LabColor], cluster_count: usize) -> Vec<LabColor> {
    let mut order: Vec<usize> = (0..pixels.len()).collect();
    order.sort_by(|&left, &right| {
        pixels[left]
            .l
            .total_cmp(&pixels[right].l)
            .then(left.cmp(&right))
    });
    let total = order.len();
    (0..cluster_count)
        .map(|slot| pixels[order[slot * total / cluster_count]])
        .collect()
}

/// Assigns every pixel to its nearest centroid and records the residual error.
fn assign(pixels: &[LabColor], centroids: &[LabColor], indices: &mut [u8], errors: &mut [f64]) {
    for (slot, pixel) in pixels.iter().enumerate() {
        let mut best = 0usize;
        let mut best_distance = f64::INFINITY;
        for (index, centroid) in centroids.iter().enumerate() {
            let distance = delta_e_squared(*pixel, *centroid);
            if distance < best_distance {
                best_distance = distance;
                best = index;
            }
        }
        indices[slot] = best as u8;
        errors[slot] = best_distance;
    }
}

/// Index of the pixel that sits furthest from the centroid it was assigned to.
fn worst_pixel(errors: &[f64]) -> Option<usize> {
    errors
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index)
}

/// Moves every centroid to the mean of its members, reseeding empty clusters.
fn update(pixels: &[LabColor], indices: &[u8], errors: &[f64], centroids: &mut [LabColor]) {
    let mut sums = vec![[0.0f64; 3]; centroids.len()];
    let mut counts = vec![0usize; centroids.len()];
    for (slot, pixel) in pixels.iter().enumerate() {
        let bucket = indices[slot] as usize;
        sums[bucket][0] += pixel.l;
        sums[bucket][1] += pixel.a;
        sums[bucket][2] += pixel.b;
        counts[bucket] += 1;
    }

    let reseed = worst_pixel(errors);
    for (index, centroid) in centroids.iter_mut().enumerate() {
        let count = counts[index];
        if count == 0 {
            if let Some(source) = reseed {
                *centroid = pixels[source];
            }
        } else {
            let divisor = count as f64;
            *centroid = LabColor::new(
                sums[index][0] / divisor,
                sums[index][1] / divisor,
                sums[index][2] / divisor,
            );
        }
    }
}

/// Largest squared movement between the previous and the updated centroid set.
fn largest_shift(previous: &[LabColor], current: &[LabColor]) -> f64 {
    previous
        .iter()
        .zip(current.iter())
        .map(|(old, new)| delta_e_squared(*old, *new))
        .fold(0.0f64, f64::max)
}

/// Clusters pixels into at most `k` centroids and returns them with per-pixel indices.
///
/// The request is clamped to the pixel count and to 256 clusters, which is the
/// largest palette a `u8` index buffer can address. Initialization is derived from
/// the pixel ordering rather than a random seed, so identical inputs always produce
/// identical palettes.
pub fn cluster_pixels(
    pixels: &[LabColor],
    k: usize,
    max_iterations: usize,
) -> (Vec<LabColor>, Vec<u8>) {
    let cluster_count = k.min(pixels.len()).min(MAX_CLUSTERS);
    if cluster_count == 0 {
        return (Vec::new(), vec![0u8; pixels.len()]);
    }

    let mut centroids = initial_centroids(pixels, cluster_count);
    let mut indices = vec![0u8; pixels.len()];
    let mut errors = vec![0.0f64; pixels.len()];

    for _ in 0..effective_iterations(max_iterations) {
        assign(pixels, &centroids, &mut indices, &mut errors);
        let previous = centroids.clone();
        update(pixels, &indices, &errors, &mut centroids);
        if largest_shift(&previous, &centroids) < CONVERGENCE_EPSILON_SQUARED {
            break;
        }
    }

    assign(pixels, &centroids, &mut indices, &mut errors);
    (centroids, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(lightness: f64, count: usize) -> Vec<LabColor> {
        vec![LabColor::new(lightness, 0.0, 0.0); count]
    }

    #[test]
    fn separated_clusters_receive_distinct_indices() {
        let mut pixels = gray(5.0, 100);
        pixels.extend(gray(95.0, 100));

        let (centroids, indices) = cluster_pixels(&pixels, 2, DEFAULT_MAX_ITERATIONS);

        assert_eq!(centroids.len(), 2);
        assert_eq!(indices.len(), 200);
        assert!(centroids[0].l < 20.0);
        assert!(centroids[1].l > 80.0);
        assert!(indices[..100].iter().all(|&index| index == indices[0]));
        assert!(indices[100..].iter().all(|&index| index == indices[100]));
        assert_ne!(indices[0], indices[100]);
    }

    #[test]
    fn centroids_match_cluster_means() {
        let mut pixels = gray(0.0, 4);
        pixels.extend(gray(40.0, 4));
        pixels.extend(gray(80.0, 4));

        let (centroids, _) = cluster_pixels(&pixels, 3, DEFAULT_MAX_ITERATIONS);
        let lightness: Vec<f64> = centroids.iter().map(|centroid| centroid.l).collect();

        assert!((lightness[0] - 0.0).abs() < 1e-6);
        assert!((lightness[1] - 40.0).abs() < 1e-6);
        assert!((lightness[2] - 80.0).abs() < 1e-6);
    }

    #[test]
    fn clustering_is_deterministic_across_runs() {
        let pixels: Vec<LabColor> = (0..64)
            .map(|step| LabColor::new(f64::from(step) * 1.5, 3.0 - f64::from(step) / 8.0, 0.5))
            .collect();

        let first = cluster_pixels(&pixels, 5, DEFAULT_MAX_ITERATIONS);
        let second = cluster_pixels(&pixels, 5, DEFAULT_MAX_ITERATIONS);

        assert_eq!(first.0, second.0);
        assert_eq!(first.1, second.1);
    }

    #[test]
    fn uniform_input_collapses_to_a_single_value() {
        let pixels = gray(42.0, 16);
        let (centroids, indices) = cluster_pixels(&pixels, 8, DEFAULT_MAX_ITERATIONS);

        assert!(centroids
            .iter()
            .all(|centroid| (centroid.l - 42.0).abs() < 1e-6));
        assert!(indices.iter().all(|&index| index == 0));
    }

    #[test]
    fn empty_input_yields_empty_palette() {
        let (centroids, indices) = cluster_pixels(&[], 4, DEFAULT_MAX_ITERATIONS);

        assert!(centroids.is_empty());
        assert!(indices.is_empty());
    }

    #[test]
    fn cluster_count_is_clamped_to_the_pixel_count() {
        let (centroids, indices) = cluster_pixels(&gray(10.0, 3), 9, DEFAULT_MAX_ITERATIONS);

        assert_eq!(centroids.len(), 3);
        assert_eq!(indices.len(), 3);
        assert!(indices
            .iter()
            .all(|&index| usize::from(index) < centroids.len()));
    }
}
