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

    let prepared: Vec<(u8, Vec<V2>)> = contours
        .iter()
        .filter_map(|contour| {
            let pts = to_points(contour);
            (pts.len() >= MIN_LOOP_VERTICES).then_some((contour.color_id, pts))
        })
        .collect();

    let junctions = junction::detect_triple_junctions(
        prepared
            .iter()
            .map(|(color_id, pts)| (*color_id, pts.as_slice())),
    );

    let mut out = Vec::with_capacity(prepared.len());
    for (color_id, mut pts) in prepared {
        // Canonical winding makes every later stage reversal-invariant.
        if geometry::signed_area(&pts) < 0.0 {
            pts.reverse();
        }
        let locked = junction::locked_mask(&pts, &junctions);
        let outcome = engine::simplify_loop(&pts, epsilon, corner_threshold_deg, &locked)?;
        out.push(to_polyline(
            color_id,
            outcome.points,
            outcome.corner_indices,
        ));
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
mod tests {
    use super::*;

    fn contour(color_id: u8, points: &[(f64, f64)]) -> RawContour {
        RawContour {
            color_id,
            points: points.iter().map(|&(x, y)| Point2D::new(x, y)).collect(),
            is_hole: false,
        }
    }

    fn pixel_square(size: i32) -> RawContour {
        let mut points = Vec::new();
        for x in 0..=size {
            points.push((f64::from(x), 0.0));
        }
        for y in 1..=size {
            points.push((f64::from(size), f64::from(y)));
        }
        for x in (0..size).rev() {
            points.push((f64::from(x), f64::from(size)));
        }
        for y in (1..size).rev() {
            points.push((0.0, f64::from(y)));
        }
        contour(0, &points)
    }

    fn regular_polygon(sides: usize, radius: f64) -> RawContour {
        let points: Vec<(f64, f64)> = (0..sides)
            .map(|k| {
                let angle = std::f64::consts::TAU * k as f64 / sides as f64;
                (radius * angle.cos(), radius * angle.sin())
            })
            .collect();
        contour(0, &points)
    }

    #[test]
    fn degenerate_loops_are_filtered_and_parameters_validated() {
        let one_pt = contour(0, &[(0.0, 0.0)]);
        let two_pt = contour(0, &[(0.0, 0.0), (1.0, 0.0), (1.0, 0.0)]);
        let out = simplify_contours(&[one_pt, two_pt], 1.0, 120.0);
        assert!(matches!(out, Ok(ref list) if list.is_empty()));
        assert!(simplify_contours(&[], f64::NAN, 120.0).is_err());
    }

    #[test]
    fn parameter_validation_rejects_out_of_range_inputs() {
        let sample = [pixel_square(4)];
        assert!(simplify_contours(&sample, -1.0, 120.0).is_err());
        assert!(simplify_contours(&sample, f64::NAN, 120.0).is_err());
        assert!(simplify_contours(&sample, f64::INFINITY, 120.0).is_err());
        assert!(simplify_contours(&sample, 1.0, 0.0).is_err());
        assert!(simplify_contours(&sample, 1.0, 180.5).is_err());
        assert!(simplify_contours(&sample, 1.0, f64::NAN).is_err());
        assert!(simplify_contours(&sample, 0.0, 180.0).is_ok());
    }

    #[test]
    fn sharp_corner_preservation_keeps_the_four_square_cusps() {
        let out = simplify_contours(&[pixel_square(40)], 1.0, 120.0)
            .unwrap_or_else(|error| panic!("expected success, got {error}"));
        let polyline = &out[0];
        assert_eq!(polyline.points.len(), 4);
        assert_eq!(polyline.corner_indices, vec![0, 1, 2, 3]);
        for point in &polyline.points {
            let on_corner =
                (point.x == 0.0 || point.x == 40.0) && (point.y == 0.0 || point.y == 40.0);
            assert!(on_corner, "corner moved to {point:?}");
        }
    }

    #[test]
    fn reversal_symmetry_invariance() {
        let forward = contour(
            0,
            &[
                (0.0, 0.0),
                (12.25, 1.1),
                (18.5, 9.0),
                (9.0, 20.0),
                (2.0, 14.0),
                (-6.0, 7.0),
            ],
        );
        let mut reversed_points = forward.points.clone();
        reversed_points.reverse();
        let reversed = RawContour {
            color_id: forward.color_id,
            points: reversed_points,
            is_hole: false,
        };
        let left = simplify_contours(&[forward], 1.0, 120.0)
            .unwrap_or_else(|error| panic!("expected success, got {error}"))
            .remove(0);
        let right = simplify_contours(&[reversed], 1.0, 120.0)
            .unwrap_or_else(|error| panic!("expected success, got {error}"))
            .remove(0);
        assert_eq!(left.points, right.points);
        assert_eq!(left.corner_indices, right.corner_indices);
    }

    #[test]
    fn smooth_circle_no_false_cusps() {
        let out = simplify_contours(&[regular_polygon(32, 10.0)], 1.0, 120.0)
            .unwrap_or_else(|error| panic!("expected success, got {error}"));
        let polyline = &out[0];
        assert!(polyline.corner_indices.is_empty(), "false cusps detected");
        assert!(polyline.points.len() >= 3 && polyline.points.len() < 32);
    }

    #[test]
    fn triple_junction_locks_shared_vertex_as_cusp_across_all_regions() {
        // Three regions meet at T-junction (10.0, 10.0), which is collinear on region 0's bottom edge.
        let top = contour(
            0,
            &[(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0), (0.0, 10.0)],
        );
        let bottom_left = contour(1, &[(0.0, 10.0), (10.0, 10.0), (10.0, 20.0), (0.0, 20.0)]);
        let bottom_right = contour(
            2,
            &[(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)],
        );
        let out = simplify_contours(&[top, bottom_left, bottom_right], 2.0, 120.0)
            .unwrap_or_else(|error| panic!("expected success, got {error}"));
        assert_eq!(out.len(), 3);
        for polyline in &out {
            let idx = polyline
                .points
                .iter()
                .position(|p| (p.x - 10.0).abs() < 1.0e-9 && (p.y - 10.0).abs() < 1.0e-9)
                .expect("triple-junction vertex must be preserved without drift");
            assert!(
                polyline.corner_indices.contains(&idx),
                "triple-junction vertex must be locked in corner_indices"
            );
        }
    }
}
