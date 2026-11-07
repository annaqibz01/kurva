//! Error type shared by every stage of the PAPHQ quantiser.

use thiserror::Error;

/// Failures that can occur while quantizing an image into a palette.
#[derive(Debug, Error)]
pub enum ColorError {
    /// The raster dimensions or the byte buffer length are inconsistent.
    #[error("image dimension overflow or invalid buffer size")]
    InvalidBufferSize,
    /// The requested palette size is outside the supported `1..=256` range.
    #[error("requested palette size {0} must be between 1 and 256")]
    InvalidPaletteSize(usize),
    /// The encoded image could not be decoded.
    #[error("image decoding error: {0}")]
    ImageDecode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_size_error_reports_the_requested_value() {
        let error = ColorError::InvalidPaletteSize(300);
        assert!(error.to_string().contains("300"));
    }
}
