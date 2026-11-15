//! Polygon simplification stage: subpixel contours in, simplified polylines out.
//!
//! AEGIS-v2 (Anchored Edge Geometric Invariant Simplification) converts each closed
//! raster contour into a watertight polyline that keeps every G0 cusp and removes
//! pixel noise inside a clearance-bounded tolerance tube.

mod clearance;
mod curvature;
mod dp;
mod engine;
mod error;
mod geometry;
mod junction;
mod periodic;
mod precollapse;
mod view;
mod visibility;

pub use error::PolyError;

use kurva_types::{Point2D, RawContour, SimplifiedPolyline};

use crate::geometry::V2;

/// Minimum number of distinct vertices a closed contour needs to form a polygon.
const MIN_LOOP_VERTICES: usize = 3;
/// Consecutive vertices closer than this distance are treated as duplicates.
const DEDUP_TOLERANCE: f64 = 1.0e-9;

/// Simplify every contour with the same tolerance and corner classification threshold.
pub fn simplify_contours(
    contours: &[RawContour],
    epsilon: f64,
    corner_threshold_deg: f64,
) -> Result<Vec<SimplifiedPolyline>, PolyError> {
    if !epsilon.is_finite() || epsilon < 0.0 {
        return Err(PolyError::InvalidParameter("epsilon"));
    }
    if !corner_threshold_deg.is_finite()
        || corner_threshold_deg <= 0.0
        || corner_threshold_deg > 180.0
    {
        return Err(PolyError::InvalidParameter("corner_threshold_deg"));
    }

    let prepared: Vec<(u8, bool, Vec<V2>)> = contours
        .iter()
        .filter_map(|contour| {
            let pts = to_points(contour);
            (pts.len() >= MIN_LOOP_VERTICES).then_some((contour.color_id, contour.is_hole, pts))
        })
        .collect();

    let junctions = junction::detect_triple_junctions(
        prepared
            .iter()
            .map(|(color_id, _, pts)| (*color_id, pts.as_slice())),
    );

    let mut out = Vec::with_capacity(prepared.len());
    for (color_id, is_hole, mut pts) in prepared {
        // Outer shells wind positively and holes wind negatively so SVG `nonzero`
        // keeps enclosed counters (e.g. 'O', 'A', 'R') hollow.
        let area = geometry::signed_area(&pts);
        if (!is_hole && area < 0.0) || (is_hole && area > 0.0) {
            pts.reverse();
        }
        let locked = junction::locked_mask(&pts, &junctions);
        let outcome = engine::simplify_loop(&pts, epsilon, corner_threshold_deg, &locked)?;
        let mut out_pts = outcome.points;
        let mut corner_indices = outcome.corner_indices;
        let out_area = geometry::signed_area(&out_pts);
        if (!is_hole && out_area < 0.0) || (is_hole && out_area > 0.0) {
            out_pts.reverse();
            let count = out_pts.len();
            for idx in &mut corner_indices {
                *idx = count - 1 - *idx;
            }
            corner_indices.sort_unstable();
        }
        out.push(to_polyline(color_id, out_pts, corner_indices));
    }
    Ok(out)
}

fn to_polyline(color_id: u8, points: Vec<V2>, corner_indices: Vec<usize>) -> SimplifiedPolyline {
    SimplifiedPolyline {
        color_id,
        points: points
            .iter()
            .map(|point| Point2D::new(point.x, point.y))
            .collect(),
        corner_indices,
        is_closed: true,
    }
}

/// Read subpixel f64 coordinates, dropping non-finite values and near-duplicates.
fn to_points(contour: &RawContour) -> Vec<V2> {
    let mut pts: Vec<V2> = Vec::with_capacity(contour.points.len());
    for point in &contour.points {
        if !point.x.is_finite() || !point.y.is_finite() {
            continue;
        }
        let candidate = V2::new(point.x, point.y);
        if pts
            .last()
            .is_none_or(|prev| prev.distance(candidate) > DEDUP_TOLERANCE)
        {
            pts.push(candidate);
        }
    }
    if pts.len() > 1 && pts[0].distance(pts[pts.len() - 1]) <= DEDUP_TOLERANCE {
        pts.pop();
    }
    pts
}

#[cfg(test)]
mod tests;
