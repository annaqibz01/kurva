//! Border following stage: indexed raster in, integer contours out.

use kurva_types::{IndexedRaster, RawContour};
use thiserror::Error;

/// Failure modes of the border-following stage.
#[derive(Debug, Error)]
pub enum ContourError {
    #[error("raster buffer holds {actual} indices, expected width * height = {expected}")]
    InconsistentRaster { expected: usize, actual: usize },
}

/// Placeholder extraction: validates raster topology and yields no loops yet.
pub fn extract_contours(raster: &IndexedRaster) -> Result<Vec<RawContour>, ContourError> {
    let expected = raster.width.saturating_mul(raster.height);
    if raster.indices.len() != expected {
        return Err(ContourError::InconsistentRaster {
            expected,
            actual: raster.indices.len(),
        });
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::extract_contours;
    use kurva_types::{ColorRgb, IndexedRaster};

    #[test]
    fn extract_contours_enforces_raster_topology() {
        let valid = IndexedRaster::new(2, 2, vec![ColorRgb::new(0, 0, 0)], vec![0; 4]);
        assert!(matches!(valid, Some(ref r) if extract_contours(r).is_ok()));
        let truncated = IndexedRaster {
            width: 2,
            height: 2,
            palette: Vec::new(),
            indices: vec![0; 3],
        };
        assert!(extract_contours(&truncated).is_err());
    }
}
