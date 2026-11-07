//! Error surface for the SVG serializer.

use thiserror::Error;

/// Failure modes reported by [`crate::render_svg`].
#[derive(Debug, Error)]
pub enum SvgError {
    /// Canvas dimensions must both be non-zero.
    #[error("canvas dimensions {width}x{height} must be non-zero")]
    EmptyCanvas { width: usize, height: usize },
    /// A path referenced a palette slot that does not exist.
    #[error("at least one path references a color outside the provided palette")]
    PaletteOutOfRange,
    /// The serialized buffer was not valid UTF-8.
    #[error("serialization error: {0}")]
    Serialization(String),
}
