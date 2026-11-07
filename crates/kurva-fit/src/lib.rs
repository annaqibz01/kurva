//! ORPHEUS-v2 variational cubic Bezier fitting.
//!
//! Orthogonal-Footpoint Regularised parameterisation over a Hermite-reduced basis:
//! shared knots `Q_k`, shared tangent magnitudes `tau_k` and shared unit directions
//! `d_k` make interior G1 continuity structural rather than a penalty. The inner
//! bounded quadratic program (knot drift along the chord plus tangent magnitude) is
//! solved by PDAS over a banded Cholesky factorisation; the outer Gauss-Newton step
//! rotates the interior directions inside a trust region. Spans are split at the
//! curvature-weighted worst deviation until the sample error drops below the
//! requested threshold.

mod angular;
mod banded;
mod curve;
mod energy;
mod error;
mod measure;
mod metric;
mod parameter;
mod pdas;
mod refine;
mod solve;
#[cfg(test)]
mod tests;
mod vec2;

pub use error::FitError;

use kurva_types::{SimplifiedPolyline, VectorPath};

/// Fit every polyline into an independent `VectorPath` of cubic segments.
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
    polylines
        .iter()
        .map(|polyline| solve::fit_polyline(polyline, error_threshold))
        .collect()
}
