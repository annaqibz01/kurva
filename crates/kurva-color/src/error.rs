use thiserror::Error;

#[derive(Debug, Error)]
pub enum ColorError {
    #[error("image dimension overflow or invalid buffer size")]
    InvalidBufferSize,
    #[error("requested palette size {0} must be between 1 and 256")]
    InvalidPaletteSize(usize),
    #[error("image decoding error: {0}")]
    ImageDecode(String),
}
