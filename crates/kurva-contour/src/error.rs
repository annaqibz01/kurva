//! Failure surface of the KARST-v2 contour pipeline.

/// Every failure mode that contour extraction can report.
#[derive(Debug, thiserror::Error)]
pub enum ContourError {
    /// The flat index buffer disagrees with the declared canvas dimensions.
    #[error("raster buffer holds {actual} indices, expected width * height = {expected}")]
    InconsistentRaster { expected: usize, actual: usize },

    /// The raster declares a zero width or a zero height.
    #[error("empty raster canvas")]
    EmptyCanvas,

    /// The half-edge planar map violated a structural invariant.
    #[error("topology error: {0}")]
    Topology(String),
}
