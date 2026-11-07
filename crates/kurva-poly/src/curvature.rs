//! Discrete curvature, Lindeberg scale-space response and G0/G1/G2 classification.

use std::f64::consts::{PI, TAU};

use crate::geometry::{periodic_delta, LoopGeometry, EPS};

/// Smallest and largest diffusion scales of the octave ladder.
pub(crate) const SIGMA_MIN: f64 = 0.5;
pub(crate) const SIGMA_MAX: f64 = 8.0;
/// Number of scales sampled between `SIGMA_MIN` and `SIGMA_MAX`.
pub(crate) const OCTAVES: usize = 5;
/// Normalization scale of the peak-scale correction term.
pub(crate) const SIGMA_REF: f64 = 2.0;
/// Corner score thresholds separating G2, G1 and G0 vertices.
pub(crate) const TAU_G1: f64 = 0.08;
pub(crate) const TAU_G0: f64 = 0.32;
/// Curvature concentration cap and singular floor.
pub(crate) const RHO_MAX: f64 = 10.0;
pub(crate) const KAPPA_MIN: f64 = 1e-6;
/// Response below which a vertex belongs to a straight run.
pub(crate) const TAU_STRAIGHT: f64 = 0.02;
/// Minimum sign coherence of the local turn field for a cusp to be trusted.
pub(crate) const COHERENCE_MIN: f64 = 0.5;
/// Half width of the turn-coherence window, in vertices.
const TURN_WINDOW: usize = 2;

/// Geometric continuity class of a vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// Pinned cusp: a genuine G0 corner that shortcuts may not bypass.
    Cusp,
    /// G1 tangent-continuous joint.
    Smooth,
    /// G2 straight run member.
    Straight,
}

/// Scale-space descriptors evaluated at the peak scale of each vertex.
pub(crate) struct CurvatureProfile {
    /// Peak scale `sigma*` per vertex.
    pub sigma_star: Vec<f64>,
    /// Signed smoothed curvature at the peak scale.
    pub smoothed: Vec<f64>,
    /// Corner score per vertex.
    pub score: Vec<f64>,
}

fn gaussian(t: f64, sigma: f64) -> f64 {
    (-(t * t) / (2.0 * sigma * sigma)).exp() / (TAU.sqrt() * sigma)
}

/// Geometric ladder of scales spanning `SIGMA_MIN ..= SIGMA_MAX`.
fn scale_ladder() -> Vec<f64> {
    let ratio = (SIGMA_MAX / SIGMA_MIN).powf(1.0 / (OCTAVES - 1) as f64);
    (0..OCTAVES)
        .map(|k| SIGMA_MIN * ratio.powi(k as i32))
        .collect()
}

/// Discrete 1D Gaussian convolution of curvature over arc length.
///
/// The weights are normalized by the sampled mass so that a constant curvature field
/// is reproduced exactly; without that normalization the kernel would rescale curvature
/// by the local vertex density.
pub(crate) fn smoothed_curvature(geom: &LoopGeometry, sigma: f64) -> Vec<f64> {
    let n = geom.pts.len();
    let support = ((3.0 * sigma).ceil() as usize).clamp(1, n - 1);
    let mut out = vec![0.0; n];
    for (j, slot) in out.iter_mut().enumerate() {
        let mut numerator = 0.0;
        let mut mass = 0.0;
        for offset in -(support as isize)..=(support as isize) {
            let i = (j as isize + offset).rem_euclid(n as isize) as usize;
            let prev = (i + n - 1) % n;
            let cell = 0.5 * (geom.edge[prev] + geom.edge[i]);
            let delta = periodic_delta(geom.arc[j], geom.arc[i], geom.perimeter).abs();
            let weight = gaussian(delta, sigma) * cell;
            numerator += geom.kappa[i] * weight;
            mass += weight;
        }
        *slot = if mass > EPS {
            numerator / mass
        } else {
            geom.kappa[j]
        };
    }
    out
}

/// Build the scale-space profile: peak scale, smoothed curvature and corner score.
///
/// The smoothed field is the density-normalized convolution `kappa_sigma`, so a run of
/// constant curvature keeps its magnitude instead of being rescaled by the local
/// vertex density. `sigma*` is the smallest scale attaining the largest
/// gamma-normalized response, which is the standard scale-selection tie-break for a
/// response that is flat over scale (an ideal corner).
pub(crate) fn profile(geom: &LoopGeometry) -> CurvatureProfile {
    let n = geom.pts.len();
    let scales = scale_ladder();
    let fields: Vec<Vec<f64>> = scales
        .iter()
        .map(|sigma| smoothed_curvature(geom, *sigma))
        .collect();
    let mut sigma_star = vec![scales[0]; n];
    let mut smoothed = vec![0.0; n];
    for j in 0..n {
        let mut best = f64::NEG_INFINITY;
        for (octave, sigma) in scales.iter().enumerate() {
            let response = sigma * fields[octave][j].abs();
            if response > best {
                best = response;
                sigma_star[j] = *sigma;
                smoothed[j] = fields[octave][j];
            }
        }
    }
    let mut score = vec![0.0; n];
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let next = (i + 1) % n;
        let neighbour = geom.kappa[prev].abs().max(geom.kappa[next].abs());
        let rho = geom.kappa[i].abs() / neighbour.max(KAPPA_MIN);
        score[i] = (geom.turn[i].abs() / PI) * rho.min(RHO_MAX) / (1.0 + sigma_star[i] / SIGMA_REF);
    }
    CurvatureProfile {
        sigma_star,
        smoothed,
        score,
    }
}

/// Fraction of the local turning mass that shares a single sign.
///
/// Raster staircases alternate the sign of their 90 degree steps, so their coherence
/// collapses towards zero even though every individual step looks like a cusp. The
/// guard keeps pixel quantisation from being pinned as a structural corner.
pub(crate) fn turn_coherence(geom: &LoopGeometry) -> Vec<f64> {
    let n = geom.pts.len();
    let mut out = vec![0.0; n];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut net = 0.0;
        let mut mass = 0.0;
        for offset in -(TURN_WINDOW as isize)..=(TURN_WINDOW as isize) {
            let k = (i as isize + offset).rem_euclid(n as isize) as usize;
            net += geom.turn[k];
            mass += geom.turn[k].abs();
        }
        *slot = if mass > KAPPA_MIN {
            (net.abs() / mass).min(1.0)
        } else {
            0.0
        };
    }
    out
}

/// Classify every vertex as G0 cusp, G1 smooth joint or G2 straight member.
pub(crate) fn classify(
    geom: &LoopGeometry,
    prof: &CurvatureProfile,
    corner_threshold_deg: f64,
) -> Vec<Class> {
    let coherence = turn_coherence(geom);
    (0..geom.pts.len())
        .map(|i| {
            let coherent = coherence[i] >= COHERENCE_MIN;
            let interior_deg = (PI - geom.turn[i].abs()).to_degrees();
            let angular_cusp = coherent && interior_deg < corner_threshold_deg;
            if angular_cusp || (coherent && prof.score[i] > TAU_G0) {
                Class::Cusp
            } else if prof.score[i] > TAU_G1 {
                Class::Smooth
            } else {
                Class::Straight
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::V2;

    fn pixel_square(size: usize) -> LoopGeometry {
        let side = size as f64;
        let mut pts = Vec::new();
        for x in 0..=size {
            pts.push(V2::new(x as f64, 0.0));
        }
        for y in 1..=size {
            pts.push(V2::new(side, y as f64));
        }
        for x in (0..size).rev() {
            pts.push(V2::new(x as f64, side));
        }
        for y in (1..size).rev() {
            pts.push(V2::new(0.0, y as f64));
        }
        match crate::geometry::analyze_closed(&pts) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        }
    }

    #[test]
    fn sharp_square_vertices_are_pinned_cusps() {
        let geom = pixel_square(40);
        let prof = profile(&geom);
        let classes = classify(&geom, &prof, 120.0);
        let cusps: Vec<usize> = (0..classes.len())
            .filter(|index| classes[*index] == Class::Cusp)
            .collect();
        assert_eq!(cusps, vec![0, 40, 80, 120]);
        for index in cusps {
            assert!(prof.score[index] > TAU_G0, "weak corner score at {index}");
        }
        assert_eq!(
            classes
                .iter()
                .filter(|class| **class == Class::Cusp)
                .count(),
            4
        );
    }

    #[test]
    fn regular_polygon_vertices_are_not_cusps() {
        let pts: Vec<V2> = (0..32)
            .map(|k| {
                let angle = TAU * f64::from(k) / 32.0;
                V2::new(10.0 * angle.cos(), 10.0 * angle.sin())
            })
            .collect();
        let geom = match crate::geometry::analyze_closed(&pts) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        };
        let prof = profile(&geom);
        let classes = classify(&geom, &prof, 120.0);
        assert!(classes.iter().all(|class| *class != Class::Cusp));
    }
}
