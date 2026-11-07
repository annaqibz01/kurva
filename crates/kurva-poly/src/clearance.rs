//! Clearance field: local feature size and the per-vertex tolerance envelope.

use crate::geometry::{periodic_delta, LoopGeometry};

/// Safety factor applied to the local feature size.
pub(crate) const BETA: f64 = 0.35;
/// Absolute floor of the tolerance field.
pub(crate) const EPS_MIN: f64 = 1e-3;
/// Boundary arc length below which two vertices count as one local feature.
///
/// The guard must clear the tolerance by a wide margin: a pixel staircase visits
/// vertices only one step apart in arc length, and treating those as a distinct
/// feature would clamp the tolerance below the staircase's own chord error and make
/// any diagonalisation impossible.
const LOCAL_ARC_FACTOR: f64 = 8.0;
/// Index neighbourhood excluded from the feature-size scan.
const LOCAL_INDEX_GUARD: usize = 1;

/// Per-vertex tolerance `eps(v_i) = max(EPS_MIN, min(epsilon, BETA * lfs(v_i)))`.
///
/// The feature-size scan skips vertices within `LOCAL_ARC_FACTOR * epsilon` of arc
/// length: those belong to the same local feature and their displacement is already
/// bounded by the tube constraint, so including them would drive the tolerance to the
/// sampling pitch and forbid any simplification of densely sampled curves.
pub(crate) fn tolerance_field(geom: &LoopGeometry, epsilon: f64) -> Vec<f64> {
    let n = geom.pts.len();
    let guard_arc = (LOCAL_ARC_FACTOR * epsilon).max(EPS_MIN);
    let mut out = vec![epsilon.max(EPS_MIN); n];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut feature = f64::INFINITY;
        for j in 0..n {
            let gap = i.abs_diff(j);
            if gap.min(n - gap) <= LOCAL_INDEX_GUARD {
                continue;
            }
            let arc_gap = periodic_delta(geom.arc[i], geom.arc[j], geom.perimeter).abs();
            if arc_gap <= guard_arc {
                continue;
            }
            feature = feature.min(geom.pts[i].distance(geom.pts[j]));
        }
        if feature.is_finite() {
            *slot = (BETA * feature).min(epsilon).max(EPS_MIN);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{analyze_closed, V2};

    fn narrow_corridor() -> LoopGeometry {
        let mut pts = Vec::new();
        for step in 0..=6 {
            pts.push(V2::new(f64::from(step) * 5.0, 0.0));
        }
        for step in (0..=6).rev() {
            pts.push(V2::new(f64::from(step) * 5.0, 2.0));
        }
        match analyze_closed(&pts) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        }
    }

    #[test]
    fn narrow_loops_clamp_the_tolerance_to_the_feature_size() {
        let geom = narrow_corridor();
        let field = tolerance_field(&geom, 1.0);
        let tightest = field.iter().fold(f64::INFINITY, |acc, eps| acc.min(*eps));
        assert!(tightest < 0.75, "feature size did not clamp: {tightest}");
        assert!(field.iter().all(|eps| *eps >= EPS_MIN));
    }

    #[test]
    fn wide_loops_keep_the_global_tolerance() {
        let pts = [
            V2::new(0.0, 0.0),
            V2::new(40.0, 0.0),
            V2::new(40.0, 40.0),
            V2::new(0.0, 40.0),
        ];
        let geom = match analyze_closed(&pts) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        };
        let field = tolerance_field(&geom, 1.0);
        assert!(field.iter().all(|eps| (*eps - 1.0).abs() < 1e-12));
    }
}
