//! Vector Magic-inspired perceptual color quantization and anti-aliasing unmixing.

mod error;
mod gradient;
mod oklab;
mod palette;
mod refine;
mod region;
mod unmix;

pub use error::{ColorError, ColorError::InvalidBufferSize};
use kurva_types::IndexedRaster;

/// Maximum number of palette entries supported by `IndexedRaster`.
pub const MAX_PALETTE_SIZE: usize = 256;

/// Quantizes an interleaved RGB8 buffer into an indexed raster using perceptual
/// Oklab segmentation, pure-region palette clustering, sRGB AA unmixing, and ICM cleanup.
pub fn quantize_rgb(
    rgb_bytes: &[u8],
    width: usize,
    height: usize,
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    if max_colors == 0 || max_colors > MAX_PALETTE_SIZE {
        return Err(ColorError::InvalidPaletteSize(max_colors));
    }

    if width == 0 || height == 0 {
        return Err(ColorError::InvalidBufferSize);
    }

    let expected = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or(ColorError::InvalidBufferSize)?;

    if rgb_bytes.len() != expected {
        return Err(ColorError::InvalidBufferSize);
    }

    let lab = oklab::raster(rgb_bytes);
    let grad_map = gradient::classify(&lab, width, height);
    let region_map = region::extract(&lab, rgb_bytes, &grad_map.kinds, width, height);
    let palette_model = palette::build(&region_map.regions, &lab, max_colors);
    let mut indices =
        unmix::assign_indices(&lab, rgb_bytes, &grad_map, &palette_model, width, height);
    refine::icm_cleanup(
        &lab,
        &palette_model.lab,
        &grad_map.thin_strokes,
        &mut indices,
        width,
        height,
    );

    IndexedRaster::new(width, height, palette_model.rgb, indices)
        .ok_or(ColorError::InvalidBufferSize)
}

/// Decodes an encoded image byte stream into RGB8 and quantizes it, returning both
/// the indexed raster and the decoded RGB8 pixel buffer.
pub fn load_and_quantize_image_with_rgb(
    image_bytes: &[u8],
    max_colors: usize,
) -> Result<(IndexedRaster, Vec<u8>), ColorError> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|err| ColorError::ImageDecode(err.to_string()))?;
    let rgb = img.into_rgb8();
    let (width, height) = rgb.dimensions();
    let raw = rgb.into_raw();
    let raster = quantize_rgb(&raw, width as usize, height as usize, max_colors)?;
    Ok((raster, raw))
}

/// Decodes an encoded image byte stream into RGB8 and quantizes it.
pub fn load_and_quantize_image(
    image_bytes: &[u8],
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    load_and_quantize_image_with_rgb(image_bytes, max_colors).map(|(raster, _)| raster)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_adaptive;
