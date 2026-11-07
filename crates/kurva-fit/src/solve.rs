//! Sub-arc decomposition at pinned vertices and `VectorPath` assembly.

use crate::error::FitError;
use crate::refine;
use crate::vec2::{cross, dot, norm, sub};
use kurva_types::{BezierSegment, Point2D, SimplifiedPolyline, VectorPath};

struct Arc {
    points: Vec<Point2D<f64>>,
    periodic: bool,
}

pub fn fit_polyline(polyline: &SimplifiedPolyline, eps: f64) -> Result<VectorPath, FitError> {
    let mut segments: Vec<BezierSegment> = Vec::new();
    for arc in sub_arcs(polyline) {
        segments.extend(refine::fit_arc(&arc.points, eps, arc.periodic)?);
    }
    Ok(VectorPath {
        color_id: polyline.color_id,
        segments,
        is_closed: polyline.is_closed,
    })
}

/// Corner indices pin the knot, so every corner becomes a sub-arc boundary and the
/// tangent discontinuity survives as a `G0` cusp. A closed ring without corners is
/// cut once at its sharpest turn and fitted with periodic `G1` continuity at the seam.
fn sub_arcs(polyline: &SimplifiedPolyline) -> Vec<Arc> {
    let points = &polyline.points;
    let count = points.len();
    if count < 2 {
        return Vec::new();
    }
    if !polyline.is_closed {
        let mut bounds = vec![0];
        bounds.extend(
            polyline
                .corner_indices
                .iter()
                .copied()
                .filter(|&corner| corner > 0 && corner < count - 1),
        );
        bounds.sort_unstable();
        bounds.dedup();
        bounds.push(count - 1);
        return bounds
            .windows(2)
            .filter(|pair| pair[1] > pair[0])
            .map(|pair| Arc {
                points: points[pair[0]..=pair[1]].to_vec(),
                periodic: false,
            })
            .collect();
    }
    let mut corners: Vec<usize> = polyline
        .corner_indices
        .iter()
        .copied()
        .filter(|&corner| corner < count)
        .collect();
    corners.sort_unstable();
    corners.dedup();
    if corners.len() <= 1 {
        let seam = corners
            .first()
            .copied()
            .unwrap_or_else(|| seam_index(points));
        let ring = (0..=count)
            .map(|step| points[(seam + step) % count])
            .collect();
        return vec![Arc {
            points: ring,
            periodic: true,
        }];
    }
    (0..corners.len())
        .map(|index| {
            let from = corners[index];
            let to = corners[(index + 1) % corners.len()];
            let steps = match (to + count - from) % count {
                0 => count,
                value => value,
            };
            Arc {
                points: (0..=steps)
                    .map(|step| points[(from + step) % count])
                    .collect(),
                periodic: false,
            }
        })
        .collect()
}

fn seam_index(points: &[Point2D<f64>]) -> usize {
    let count = points.len();
    let mut best = (f64::NEG_INFINITY, 0);
    for index in 0..count {
        let previous = points[(index + count - 1) % count];
        let next = points[(index + 1) % count];
        let incoming = sub(points[index], previous);
        let outgoing = sub(next, points[index]);
        let scale = norm(incoming) * norm(outgoing);
        let turn = if scale <= f64::EPSILON {
            0.0
        } else {
            (cross(incoming, outgoing) / scale)
                .atan2(dot(incoming, outgoing) / scale)
                .abs()
        };
        if turn > best.0 {
            best = (turn, index);
        }
    }
    best.1
}
