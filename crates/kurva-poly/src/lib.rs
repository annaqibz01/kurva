//! Polygon simplification stage: integer contours in, simplified polylines out.
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
mod periodic;
mod precollapse;
mod view;
mod visibility;

pub use error::PolyError;

use kurva_types::{Point2D, RawContour, SimplifiedPolyline};

use crate::geometry::V2;

/// Minimum number of distinct vertices a closed contour needs to form a polygon.
const MIN_LOOP_VERTICES: usize = 3;

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
    contours
        .iter()
        .map(|contour| simplify_one(contour, epsilon, corner_threshold_deg))
        .collect()
}

fn simplify_one(
    contour: &RawContour,
    epsilon: f64,
    corner_threshold_deg: f64,
) -> Result<SimplifiedPolyline, PolyError> {
    let mut pts = to_points(contour);
    if pts.len() < MIN_LOOP_VERTICES {
        return Ok(to_polyline(contour.color_id, pts, Vec::new()));
    }
    // Canonical winding makes every later stage reversal-invariant: a clockwise input is
    // flipped so that mirrored contours produce the same output sequence.
    if geometry::signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    let outcome = engine::simplify_loop(&pts, epsilon, corner_threshold_deg)?;
    Ok(to_polyline(
        contour.color_id,
        outcome.points,
        outcome.corner_indices,
    ))
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

/// Convert to f64 and drop consecutive duplicates, including the closing repeat.
fn to_points(contour: &RawContour) -> Vec<V2> {
    let mut pts: Vec<V2> = Vec::with_capacity(contour.points.len());
    for point in &contour.points {
        let candidate = V2::new(f64::from(point.x), f64::from(point.y));
        if pts.last() != Some(&candidate) {
            pts.push(candidate);
        }
    }
    if pts.len() > 1 && pts[0] == pts[pts.len() - 1] {
        pts.pop();
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contour(points: &[(i32, i32)]) -> RawContour {
        RawContour {
            color_id: 0,
            points: points.iter().map(|&(x, y)| Point2D::new(x, y)).collect(),
            is_hole: false,
        }
    }

    /// Pixel square of `size` pixels per side, sampled at every integer coordinate.
    fn pixel_square(size: i32) -> RawContour {
        let mut points = Vec::new();
        for x in 0..=size {
            points.push((x, 0));
        }
        for y in 1..=size {
            points.push((size, y));
        }
        for x in (0..size).rev() {
            points.push((x, size));
        }
        for y in (1..size).rev() {
            points.push((0, y));
        }
        contour(&points)
    }

    fn regular_polygon(sides: usize, radius: f64) -> RawContour {
        let points: Vec<(i32, i32)> = (0..sides)
            .map(|k| {
                let angle = std::f64::consts::TAU * k as f64 / sides as f64;
                (
                    (radius * angle.cos()).round() as i32,
                    (radius * angle.sin()).round() as i32,
                )
            })
            .collect();
        contour(&points)
    }

    #[test]
    fn simplify_validates_parameters_and_closes_loops() {
        let contour = RawContour {
            color_id: 0,
            points: vec![Point2D::new(0, 0)],
            is_hole: false,
        };
        let out = simplify_contours(&[contour], 1.0, 120.0);
        assert!(matches!(out, Ok(ref l) if l.len() == 1 && l[0].is_closed));
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
        let out = match simplify_contours(&[pixel_square(40)], 1.0, 120.0) {
            Ok(out) => out,
            Err(error) => panic!("expected success, got {error}"),
        };
        let polyline = &out[0];
        assert_eq!(
            polyline.points.len(),
            4,
            "square must collapse to its cusps"
        );
        assert_eq!(polyline.corner_indices, vec![0, 1, 2, 3]);
        for point in &polyline.points {
            let on_corner =
                (point.x == 0.0 || point.x == 40.0) && (point.y == 0.0 || point.y == 40.0);
            assert!(on_corner, "corner moved to {point:?}");
        }
    }

    #[test]
    fn reversal_symmetry_invariance() {
        let forward = contour(&[(0, 0), (12, 1), (18, 9), (9, 20), (2, 14), (-6, 7)]);
        let mut reversed_points = forward.points.clone();
        reversed_points.reverse();
        let reversed = RawContour {
            color_id: forward.color_id,
            points: reversed_points,
            is_hole: false,
        };
        let left = match simplify_contours(&[forward], 1.0, 120.0) {
            Ok(mut out) => out.remove(0),
            Err(error) => panic!("expected success, got {error}"),
        };
        let right = match simplify_contours(&[reversed], 1.0, 120.0) {
            Ok(mut out) => out.remove(0),
            Err(error) => panic!("expected success, got {error}"),
        };
        assert_eq!(left.points, right.points);
        assert_eq!(left.corner_indices, right.corner_indices);
    }

    #[test]
    fn color_id_is_forwarded_from_contour() {
        let mut square = pixel_square(10);
        square.color_id = 3;
        let out = match simplify_contours(&[square], 1.0, 120.0) {
            Ok(out) => out,
            Err(error) => panic!("expected success, got {error}"),
        };
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].color_id, 3);
    }

    #[test]
    fn smooth_circle_no_false_cusps() {
        let out = match simplify_contours(&[regular_polygon(32, 10.0)], 1.0, 120.0) {
            Ok(out) => out,
            Err(error) => panic!("expected success, got {error}"),
        };
        let polyline = &out[0];
        assert!(polyline.corner_indices.is_empty(), "false cusps detected");
        assert!(polyline.points.len() >= 3);
        assert!(
            polyline.points.len() < 32,
            "no reduction: {} points",
            polyline.points.len()
        );
    }
}
