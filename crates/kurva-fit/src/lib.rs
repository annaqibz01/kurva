//! Curve fitting stage: simplified polylines in, cubic Bezier paths out.

use kurva_types::{SimplifiedPolyline, VectorPath};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FitError {
    #[error("error threshold {0} must be finite and positive")]
    InvalidThreshold(f64),
    #[error("polyline {index} holds {points} vertices, at least 2 are required")]
    DegeneratePolyline { index: usize, points: usize },
}

/// Placeholder fitting: validates its input and returns no paths until Schneider's fit lands.
pub fn fit_polylines(
    polylines: &[SimplifiedPolyline],
    error_threshold: f64,
) -> Result<Vec<VectorPath>, FitError> {
    if !error_threshold.is_finite() || error_threshold <= 0.0 {
        return Err(FitError::InvalidThreshold(error_threshold));
    }
    for (index, polyline) in polylines.iter().enumerate() {
        if polyline.points.len() < 2 {
            let points = polyline.points.len();
            return Err(FitError::DegeneratePolyline { index, points });
        }
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_rejects_degenerate_polyline() {
        let polyline = SimplifiedPolyline {
            points: Vec::new(),
            corner_indices: Vec::new(),
            is_closed: false,
        };
        let out = fit_polylines(&[polyline], 2.0);
        assert!(matches!(out, Err(FitError::DegeneratePolyline { .. })));
        assert!(fit_polylines(&[], 2.0).is_ok());
        assert!(fit_polylines(&[], 0.0).is_err());
    }
}
