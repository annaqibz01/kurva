//! Palette quantization stage: raw sRGB bytes in, indexed raster out.

pub mod cielab;
pub mod error;
pub mod kmeans;

pub use cielab::{delta_e_squared, lab_to_rgb, rgb_to_lab, LabColor};
pub use error::ColorError;
pub use kmeans::cluster_pixels;

use kurva_types::{ColorRgb, IndexedRaster};

/// Largest palette an indexed raster can address through its `u8` index buffer.
pub const MAX_PALETTE_SIZE: usize = 256;

/// Quantizes an interleaved RGB8 buffer into an indexed raster.
///
/// The buffer must hold exactly `width * height * 3` bytes and `max_colors` must
/// fall inside `1..=256`. Pixels are clustered in CIELAB space so the palette
/// follows perceptual distance rather than raw channel distance.
pub fn quantize_rgb(
    rgb_bytes: &[u8],
    width: usize,
    height: usize,
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    if !(1..=MAX_PALETTE_SIZE).contains(&max_colors) {
        return Err(ColorError::InvalidPaletteSize(max_colors));
    }

    let pixel_count = width
        .checked_mul(height)
        .ok_or(ColorError::InvalidBufferSize)?;
    let expected = pixel_count
        .checked_mul(3)
        .ok_or(ColorError::InvalidBufferSize)?;
    if rgb_bytes.len() != expected {
        return Err(ColorError::InvalidBufferSize);
    }

    let mut samples = Vec::with_capacity(pixel_count);
    for chunk in rgb_bytes.chunks_exact(3) {
        samples.push(rgb_to_lab(chunk[0], chunk[1], chunk[2]));
    }

    let (centroids, indices) =
        kmeans::cluster_pixels(&samples, max_colors, kmeans::DEFAULT_MAX_ITERATIONS);
    let converted: Vec<ColorRgb> = centroids
        .iter()
        .map(|centroid| lab_to_rgb(*centroid))
        .collect();
    let (palette, remapped) = collapse_duplicates(&converted, &indices);

    IndexedRaster::new(width, height, palette, remapped).ok_or(ColorError::InvalidBufferSize)
}

/// Merges palette entries that render to the same RGB value and remaps indices.
///
/// Clusters that converge onto the same color would otherwise occupy several
/// palette slots, so the palette is compacted while keeping first-seen order.
fn collapse_duplicates(palette: &[ColorRgb], indices: &[u8]) -> (Vec<ColorRgb>, Vec<u8>) {
    let mut unique: Vec<ColorRgb> = Vec::with_capacity(palette.len());
    let mut mapping = vec![0u8; palette.len()];
    for (slot, color) in palette.iter().enumerate() {
        let existing = unique
            .iter()
            .position(|seen| seen.r == color.r && seen.g == color.g && seen.b == color.b);
        mapping[slot] = match existing {
            Some(position) => position as u8,
            None => {
                unique.push(*color);
                (unique.len() - 1) as u8
            }
        };
    }
    let remapped = indices
        .iter()
        .map(|&index| mapping[usize::from(index)])
        .collect();
    (unique, remapped)
}

/// Decodes an encoded image held in memory and quantizes it into a palette.
pub fn load_and_quantize_image(
    image_bytes: &[u8],
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    let decoded = image::load_from_memory(image_bytes)
        .map_err(|error| ColorError::ImageDecode(error.to_string()))?;
    let rgb = decoded.to_rgb8();
    let (width, height) = rgb.dimensions();
    quantize_rgb(rgb.as_raw(), width as usize, height as usize, max_colors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn solid(width: usize, height: usize, color: [u8; 3]) -> Vec<u8> {
        color.repeat(width * height)
    }

    fn checker() -> Vec<u8> {
        let mut bytes = Vec::new();
        for step in 0..4 {
            let color = if step % 2 == 0 {
                [10, 20, 30]
            } else {
                [230, 220, 210]
            };
            bytes.extend_from_slice(&color);
        }
        bytes
    }

    fn png_bytes(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(width, height, image::Rgb(color));
        let mut buffer = Vec::new();
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
            .expect("encoder writes into memory");
        buffer
    }

    #[test]
    fn solid_image_produces_one_palette_entry() {
        let raster = quantize_rgb(&solid(4, 4, [12, 34, 56]), 4, 4, 8).expect("valid input");

        assert_eq!(raster.width, 4);
        assert_eq!(raster.height, 4);
        assert_eq!(raster.pixel_count(), 16);
        assert_eq!(raster.indices.len(), 16);
        assert_eq!(raster.palette.len(), 1);
        assert!(raster.indices.iter().all(|&index| index == 0));
        let entry = raster.palette[0];
        assert!(entry.r.abs_diff(12) <= 1);
        assert!(entry.g.abs_diff(34) <= 1);
        assert!(entry.b.abs_diff(56) <= 1);
    }

    #[test]
    fn two_tone_image_keeps_both_colors_distinct() {
        let pixels = checker();
        let raster = quantize_rgb(&pixels, 2, 2, 2).expect("valid input");

        assert_eq!(raster.palette.len(), 2);
        assert_ne!(raster.indices[0], raster.indices[1]);
        assert_eq!(raster.indices[0], raster.indices[2]);
        assert_eq!(raster.indices[1], raster.indices[3]);
    }

    #[test]
    fn every_index_addresses_a_palette_entry() {
        let pixels: Vec<u8> = (0..36u8).map(|value| value.wrapping_mul(7)).collect();
        let raster = quantize_rgb(&pixels, 3, 4, 256).expect("valid input");

        assert_eq!(raster.indices.len(), 12);
        assert!(!raster.palette.is_empty());
        assert!(raster.palette.len() <= 256);
        assert!(raster
            .indices
            .iter()
            .all(|&index| usize::from(index) < raster.palette.len()));
    }

    #[test]
    fn palette_size_is_validated() {
        let pixels = solid(2, 2, [0, 0, 0]);

        assert!(matches!(
            quantize_rgb(&pixels, 2, 2, 0),
            Err(ColorError::InvalidPaletteSize(0))
        ));
        assert!(matches!(
            quantize_rgb(&pixels, 2, 2, 257),
            Err(ColorError::InvalidPaletteSize(257))
        ));
    }

    #[test]
    fn buffer_length_is_validated() {
        assert!(matches!(
            quantize_rgb(&[0, 0, 0], 2, 2, 4),
            Err(ColorError::InvalidBufferSize)
        ));
        assert!(matches!(
            quantize_rgb(&solid(2, 2, [1, 2, 3]), 2, 3, 4),
            Err(ColorError::InvalidBufferSize)
        ));
    }

    #[test]
    fn dimension_overflow_is_rejected() {
        assert!(matches!(
            quantize_rgb(&[], usize::MAX, 2, 4),
            Err(ColorError::InvalidBufferSize)
        ));
    }

    #[test]
    fn encoded_image_is_decoded_and_quantized() {
        let encoded = png_bytes(3, 2, [90, 120, 150]);
        let raster = load_and_quantize_image(&encoded, 4).expect("valid png");

        assert_eq!(raster.width, 3);
        assert_eq!(raster.height, 2);
        assert_eq!(raster.indices.len(), 6);
        let entry = raster.palette[0];
        assert!(entry.r.abs_diff(90) <= 1);
        assert!(entry.g.abs_diff(120) <= 1);
        assert!(entry.b.abs_diff(150) <= 1);
    }

    #[test]
    fn undecodable_bytes_surface_a_decode_error() {
        let result = load_and_quantize_image(&[1, 2, 3, 4, 5, 6, 7, 8], 4);

        assert!(matches!(result, Err(ColorError::ImageDecode(_))));
    }
}
