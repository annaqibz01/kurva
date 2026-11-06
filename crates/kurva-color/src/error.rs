//! Error types for the palette quantization stage.

use thiserror::Error;

/// Failures that can occur while quantizing an image into a palette.
#[derive(Debug, Error)]
pub enum ColorError {
    #[error("Image dimension overflow or invalid buffer size")]
    InvalidBufferSize,
    #[error("Requested palette size {0} must be between 1 and 256")]
    InvalidPaletteSize(usize),
    #[error("Image decoding error: {0}")]
    ImageDecode(String),
}
