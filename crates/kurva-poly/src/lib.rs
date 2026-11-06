//! Polygon simplification stage: integer contours in, simplified polylines out.

use kurva_types::{Point2D, RawContour, SimplifiedPolyline};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolyError {
    #[error("{0} is outside the valid range")]
    InvalidParameter(&'static str),
}

/// Placeholder simplification: forwards vertices without RDP reduction or cusp classification.
pub fn simplify_contours(
    contours: &[RawContour],
    epsilon: f64,
    corner_threshold_deg: f64,
) -> Result<Vec<SimplifiedPolyline>, PolyError> {
    if !epsilon.is_finite() || epsilon < 0.0 {
        return Err(PolyError::InvalidParameter("epsilon"));
    }
    if !(corner_threshold_deg > 0.0 && corner_threshold_deg <= 180.0) {
        return Err(PolyError::InvalidParameter("corner_threshold_deg"));
    }
    let simplified = contours.iter().map(|contour| SimplifiedPolyline {
        points: contour.points.iter().map(to_f64).collect(),
        corner_indices: Vec::new(),
        is_closed: true,
    });
    Ok(simplified.collect())
}

fn to_f64(point: &Point2D<i32>) -> Point2D<f64> {
    Point2D::new(f64::from(point.x), f64::from(point.y))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_validates_parameters_and_closes_loops() {
        let contour = RawContour {
            points: vec![Point2D::new(0, 0)],
            is_hole: false,
        };
        let out = simplify_contours(&[contour], 1.0, 120.0);
        assert!(matches!(out, Ok(ref l) if l.len() == 1 && l[0].is_closed));
        assert!(simplify_contours(&[], f64::NAN, 120.0).is_err());
    }
}
