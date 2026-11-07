//! Failure surface of the ORPHEUS-v2 fitting pipeline.

/// Every recoverable failure of the cubic Bezier fitting stage.
#[derive(Debug, thiserror::Error)]
pub enum FitError {
    #[error("error threshold {0} must be finite and positive")]
    InvalidThreshold(f64),
    #[error("polyline {index} holds {points} vertices, at least 2 are required")]
    DegeneratePolyline { index: usize, points: usize },
    #[error("fitting optimization failed: {0}")]
    Internal(String),
}
