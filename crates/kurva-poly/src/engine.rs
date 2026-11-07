//! AEGIS-v2 pipeline for one closed loop: curvature, clearance, collapse, visibility, DP.

use crate::clearance::tolerance_field;
use crate::curvature::{classify, profile};
use crate::dp;
use crate::error::PolyError;
use crate::geometry::{analyze_closed, V2};
use crate::periodic;
use crate::precollapse;
use crate::view::LoopView;
use crate::visibility;

/// Simplified loop expressed in the rotated, anchor-first vertex order.
pub(crate) struct Outcome {
    pub points: Vec<V2>,
    pub corner_indices: Vec<usize>,
}

/// Loops at or below this size explore every shortcut span.
const SMALL_LOOP: usize = 128;
/// Larger loops bound the shortcut span to keep the tube scan near linear.
const LOCAL_SPAN: usize = 64;

pub(crate) fn simplify_loop(
    pts: &[V2],
    epsilon: f64,
    corner_threshold_deg: f64,
) -> Result<Outcome, PolyError> {
    let geom = analyze_closed(pts)?;
    let prof = profile(&geom);
    let class = classify(&geom, &prof, corner_threshold_deg);
    let eps = tolerance_field(&geom, epsilon);
    let offset = periodic::anchor_offset(&geom, &prof, &class);
    let view = LoopView::build(&geom, &prof, &class, &eps, offset);

    if view.cusp.iter().take(view.n).all(|cusp| !*cusp) {
        if let Some(polygon) = periodic::circular_primitive(&geom, offset, epsilon) {
            return Ok(Outcome {
                points: polygon,
                corner_indices: Vec::new(),
            });
        }
    }

    let arcs = periodic::arcs(&view);
    let pre = precollapse::collapse(&view, &arcs);
    let graph = visibility::build(&view, &pre.keep, &pre.forced, span_cap(view.n));
    let bend = dp::prefix_bend(&view);

    let mut kept: Vec<usize> = Vec::with_capacity(view.n);
    for &(start, end) in &arcs {
        let path = dp::solve_range(&graph, &bend, start, end).ok_or_else(|| {
            PolyError::Internal(String::from("arc has no admissible shortcut path"))
        })?;
        for index in path {
            if kept.last() != Some(&index) {
                kept.push(index);
            }
        }
    }
    let outcome = collect(&view, &kept);
    if outcome.points.len() < 3 {
        // A tolerance larger than the loop itself has no polygon to represent.
        return Ok(Outcome {
            points: view.pts[..view.n].to_vec(),
            corner_indices: (0..view.n).filter(|index| view.cusp[*index]).collect(),
        });
    }
    Ok(outcome)
}

fn span_cap(n: usize) -> usize {
    if n <= SMALL_LOOP {
        n
    } else {
        LOCAL_SPAN
    }
}

fn collect(view: &LoopView, kept: &[usize]) -> Outcome {
    let mut points = Vec::with_capacity(kept.len());
    let mut corner_indices = Vec::new();
    for &index in kept {
        // The terminal index repeats the seam vertex, which is already emitted first.
        if index >= view.n {
            continue;
        }
        if view.cusp[index] {
            corner_indices.push(points.len());
        }
        points.push(view.pts[index]);
    }
    Outcome {
        points,
        corner_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_staircase_hypotenuse_collapses_to_its_chord() {
        let mut pts = Vec::new();
        for step in 0..=10 {
            pts.push(V2::new(0.0, f64::from(step)));
        }
        let mut x = 0;
        let mut y = 10;
        while y > 0 {
            x += 1;
            pts.push(V2::new(f64::from(x), f64::from(y)));
            y -= 1;
            pts.push(V2::new(f64::from(x), f64::from(y)));
        }
        for step in (1..10).rev() {
            pts.push(V2::new(f64::from(step), 0.0));
        }
        let outcome = match simplify_loop(&pts, 1.0, 120.0) {
            Ok(outcome) => outcome,
            Err(error) => panic!("expected success, got {error}"),
        };
        assert!(
            outcome.points.len() <= 5,
            "staircase jitter survived: {} points",
            outcome.points.len()
        );
        // Every surviving vertex must sit within one pixel of the ideal triangle
        // boundary: a pixel-scale staircase may keep its entrance vertex, but no step
        // of the run may survive as a separate vertex.
        for point in &outcome.points {
            let to_diagonal = (point.x + point.y - 10.0).abs() / std::f64::consts::SQRT_2;
            let to_boundary = point.x.abs().min(point.y.abs()).min(to_diagonal);
            assert!(
                to_boundary <= 1.0 + 1e-9,
                "vertex {point:?} is off the triangle boundary"
            );
        }
    }
}
