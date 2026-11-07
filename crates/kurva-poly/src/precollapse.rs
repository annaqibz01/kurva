//! Reversal-symmetric flat-run collapse: verified chords replace straight vertex runs
//! before the dynamic program sees them, which keeps the shortcut graph small.

use crate::geometry::V2;
use crate::view::LoopView;

/// Result of the pre-collapse pass.
pub(crate) struct Precollapse {
    /// Vertices that survive (never written as `false` for anchors or run endpoints).
    pub keep: Vec<bool>,
    /// Chords validated by the tube test that must exist in the shortcut graph.
    pub forced: Vec<(usize, usize)>,
}

/// Collapse every maximal flat run whose chord stays inside the tolerance tube.
///
/// A run that does not fit is bisected at its midpoint and the halves are retried.
/// The split index uses integer division, so the recursion is invariant under
/// reversal of the loop direction.
pub(crate) fn collapse(view: &LoopView, arcs: &[(usize, usize)]) -> Precollapse {
    let mut state = Precollapse {
        keep: vec![true; view.pts.len()],
        forced: Vec::new(),
    };
    for &(start, end) in arcs {
        let mut cursor = start + 1;
        while cursor < end {
            if !view.flat[cursor] {
                cursor += 1;
                continue;
            }
            let run_start = cursor;
            let mut run_end = cursor;
            while run_end + 1 < end && view.flat[run_end + 1] {
                run_end += 1;
            }
            collapse_span(view, run_start - 1, run_end + 1, &mut state);
            cursor = run_end + 1;
        }
    }
    state
}

fn collapse_span(view: &LoopView, left: usize, right: usize, state: &mut Precollapse) {
    if right <= left + 1 {
        return;
    }
    if tube_fits(view, left, right) {
        for index in left + 1..right {
            state.keep[index] = false;
        }
        state.forced.push((left, right));
        return;
    }
    let middle = left + (right - left) / 2;
    if middle == left || middle == right {
        return;
    }
    collapse_span(view, left, middle, state);
    collapse_span(view, middle, right, state);
}

fn tube_fits(view: &LoopView, left: usize, right: usize) -> bool {
    let a = view.pts[left];
    let b = view.pts[right];
    (left + 1..right).all(|k| V2::segment_distance(view.pts[k], a, b) <= view.eps[k])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clearance::tolerance_field;
    use crate::curvature::{classify, profile};
    use crate::geometry::analyze_closed;

    fn view_of(pts: &[V2], epsilon: f64) -> LoopView {
        let geom = match analyze_closed(pts) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        };
        let prof = profile(&geom);
        let class = classify(&geom, &prof, 120.0);
        let eps = tolerance_field(&geom, epsilon);
        LoopView::build(&geom, &prof, &class, &eps, 0)
    }

    #[test]
    fn collinear_runs_collapse_to_their_endpoints() {
        let mut pts = Vec::new();
        for step in 0..=100 {
            pts.push(V2::new(f64::from(step), 0.0));
        }
        for step in 1..=100 {
            pts.push(V2::new(100.0, f64::from(step)));
        }
        for step in (0..100).rev() {
            pts.push(V2::new(f64::from(step), 100.0));
        }
        for step in (1..100).rev() {
            pts.push(V2::new(0.0, f64::from(step)));
        }
        let view = view_of(&pts, 1.0);
        let arcs = vec![(0, view.n)];
        let state = collapse(&view, &arcs);
        let kept = state.keep.iter().take(view.n).filter(|keep| **keep).count();
        assert!(kept < view.n, "no run was collapsed");
        assert!(!state.forced.is_empty());
        assert!(state.keep[0], "run endpoints must survive");
    }
}
