//! Unrolled view of a closed loop: every stage works on a linear index range whose
//! terminal node duplicates the seam vertex, so wrap-around never needs special cases.

use crate::curvature::{Class, CurvatureProfile, TAU_STRAIGHT};
use crate::geometry::{LoopGeometry, V2};

/// A closed loop rotated so that its anchor becomes index 0, with index `n` repeating it.
pub(crate) struct LoopView {
    /// Number of distinct vertices.
    pub n: usize,
    pub pts: Vec<V2>,
    pub arc: Vec<f64>,
    pub smoothed: Vec<f64>,
    pub sigma_star: Vec<f64>,
    pub eps: Vec<f64>,
    pub cusp: Vec<bool>,
    /// Straight-run member: flat enough to be collapsed by the pre-collapse stage.
    pub flat: Vec<bool>,
}

impl LoopView {
    pub(crate) fn build(
        geom: &LoopGeometry,
        prof: &CurvatureProfile,
        class: &[Class],
        eps: &[f64],
        offset: usize,
    ) -> Self {
        let n = geom.pts.len();
        let mut out = Self {
            n,
            pts: Vec::with_capacity(n + 1),
            arc: Vec::with_capacity(n + 1),
            smoothed: Vec::with_capacity(n + 1),
            sigma_star: Vec::with_capacity(n + 1),
            eps: Vec::with_capacity(n + 1),
            cusp: Vec::with_capacity(n + 1),
            flat: Vec::with_capacity(n + 1),
        };
        for r in 0..=n {
            let i = (r + offset) % n;
            out.pts.push(geom.pts[i]);
            out.arc.push(if r == n {
                geom.perimeter
            } else {
                (geom.arc[i] + geom.perimeter - geom.arc[offset]) % geom.perimeter
            });
            out.smoothed.push(prof.smoothed[i]);
            out.sigma_star.push(prof.sigma_star[i]);
            out.eps.push(eps[i]);
            let cusp = class[i] == Class::Cusp;
            out.cusp.push(cusp);
            let flat = !cusp && prof.sigma_star[i] * prof.smoothed[i].abs() < TAU_STRAIGHT;
            out.flat.push(flat);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clearance::tolerance_field;
    use crate::curvature::{classify, profile};
    use crate::geometry::analyze_closed;

    #[test]
    fn rotation_keeps_the_seam_duplicated() {
        let pts = [
            V2::new(0.0, 0.0),
            V2::new(10.0, 0.0),
            V2::new(10.0, 10.0),
            V2::new(0.0, 10.0),
        ];
        let geom = match analyze_closed(&pts) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        };
        let prof = profile(&geom);
        let class = classify(&geom, &prof, 120.0);
        let eps = tolerance_field(&geom, 1.0);
        let view = LoopView::build(&geom, &prof, &class, &eps, 2);
        assert_eq!(view.pts.len(), 5);
        assert_eq!(view.pts[0], view.pts[4]);
        assert_eq!(view.pts[0], V2::new(10.0, 10.0));
        assert!((view.arc[4] - geom.perimeter).abs() < 1e-12);
        assert!(view.arc.windows(2).all(|pair| pair[1] > pair[0]));
    }
}
