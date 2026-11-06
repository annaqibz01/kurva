//! Palette quantization stage: raw sRGB bytes in, indexed raster out.

use kurva_types::{ColorRgb, IndexedRaster};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ColorError {
    #[error("pixel buffer holds {actual} bytes, expected width * height * 3")]
    BufferSize { actual: usize },
    #[error("max_colors {0} is outside the supported range 1..=8")]
    ColorCount(usize),
    #[error("dimensions overflow the address space")]
    DimensionOverflow,
}

/// Placeholder quantization: derives a single-entry palette from the first pixel.
pub fn quantize_palette(
    rgb: &[u8],
    width: usize,
    height: usize,
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    if !(1..=8).contains(&max_colors) {
        return Err(ColorError::ColorCount(max_colors));
    }
    let pixels = width
        .checked_mul(height)
        .ok_or(ColorError::DimensionOverflow)?;
    if pixels.checked_mul(3) != Some(rgb.len()) {
        return Err(ColorError::BufferSize { actual: rgb.len() });
    }
    let palette = match (rgb.first(), rgb.get(1), rgb.get(2)) {
        (Some(&r), Some(&g), Some(&b)) => vec![ColorRgb::new(r, g, b)],
        _ => Vec::new(),
    };
    IndexedRaster::new(width, height, palette, vec![0; pixels]).ok_or(ColorError::DimensionOverflow)
}

#[cfg(test)]
mod tests {
    use super::quantize_palette;
    #[test]
    fn quantize_validates_buffer_and_indexes_pixels() {
        let buffer = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let raster = quantize_palette(&buffer, 2, 2, 4);
        assert!(matches!(raster, Ok(ref r) if r.pixel_count() == 4 && r.palette.len() == 1));
        assert!(quantize_palette(&[0, 0, 0], 2, 2, 4).is_err());
    }
}
