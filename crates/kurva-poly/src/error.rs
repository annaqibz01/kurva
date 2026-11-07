//! Error taxonomy for the polygon simplification stage.

use thiserror::Error;

/// Failure modes surfaced by the polygon stage.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolyError {
    #[error("parameter '{0}' is outside the valid range")]
    InvalidParameter(&'static str),
    #[error("degenerate polyline with fewer than 3 points")]
    DegeneratePolyline,
    #[error("simplification failed: {0}")]
    Internal(String),
}
