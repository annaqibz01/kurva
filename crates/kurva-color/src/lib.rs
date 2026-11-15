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
    let mut indices = unmix::assign_indices(&lab, rgb_bytes, &grad_map.kinds, &palette_model, width, height);
    refine::icm_cleanup(&lab, &palette_model.lab, &mut indices, width, height);

    IndexedRaster::new(width, height, palette_model.rgb, indices)
        .ok_or(ColorError::InvalidBufferSize)
}

/// Decodes an encoded image byte stream into RGB8 and quantizes it.
pub fn load_and_quantize_image(
    image_bytes: &[u8],
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|err| ColorError::ImageDecode(err.to_string()))?;
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();
    quantize_rgb(rgb.as_raw(), width as usize, height as usize, max_colors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_color_quantizes_to_single_entry() {
        let mut buf = Vec::with_capacity(16 * 16 * 3);
        for _ in 0..(16 * 16) {
            buf.extend_from_slice(&[42, 137, 210]);
        }
        let raster = quantize_rgb(&buf, 16, 16, 16).expect("solid quantization should succeed");
        assert_eq!(raster.palette.len(), 1);
        let p = raster.palette[0];
        assert!(p.r.abs_diff(42) <= 1);
        assert!(p.g.abs_diff(137) <= 1);
        assert!(p.b.abs_diff(210) <= 1);
        assert!(raster.indices.iter().all(|&idx| idx == 0));
    }

    #[test]
    fn checkerboard_alternates_two_entries() {
        let mut buf = Vec::with_capacity(8 * 8 * 3);
        for y in 0..8 {
            for x in 0..8 {
                if (x + y) % 2 == 0 {
                    buf.extend_from_slice(&[10, 10, 10]);
                } else {
                    buf.extend_from_slice(&[245, 245, 245]);
                }
            }
        }
        let raster = quantize_rgb(&buf, 8, 8, 8).expect("checkerboard should succeed");
        assert!(raster.palette.len() >= 2);
        let top_left = raster.indices[0];
        let neighbor = raster.indices[1];
        assert_ne!(top_left, neighbor);
        for y in 0..8 {
            for x in 0..8 {
                let expected = if (x + y) % 2 == 0 { top_left } else { neighbor };
                assert_eq!(raster.indices[y * 8 + x], expected);
            }
        }
    }

    #[test]
    fn anti_aliased_edge_unmixes_without_gray_halo() {
        let width = 12;
        let height = 6;
        let mut buf = Vec::with_capacity(width * height * 3);
        for _y in 0..height {
            for x in 0..width {
                if x < 5 {
                    buf.extend_from_slice(&[0, 0, 0]);
                } else if x == 5 {
                    buf.extend_from_slice(&[128, 128, 128]);
                } else {
                    buf.extend_from_slice(&[255, 255, 255]);
                }
            }
        }
        let raster = quantize_rgb(&buf, width, height, 8).expect("AA edge should succeed");
        assert_eq!(raster.palette.len(), 2);
        let left_idx = raster.indices[0];
        let right_idx = raster.indices[width - 1];
        assert_ne!(left_idx, right_idx);
        for y in 0..height {
            let mid = raster.indices[y * width + 5];
            assert!(mid == left_idx || mid == right_idx);
        }
    }

    #[test]
    fn tiny_accent_dot_survives_on_large_background() {
        let width = 24;
        let height = 24;
        let mut buf = vec![0_u8; width * height * 3];
        for px in buf.as_chunks_mut::<3>().0 {
            px.copy_from_slice(&[20, 40, 180]);
        }
        for dy in 11..=12 {
            for dx in 11..=12 {
                let base = (dy * width + dx) * 3;
                buf[base] = 240;
                buf[base + 1] = 220;
                buf[base + 2] = 30;
            }
        }
        let raster = quantize_rgb(&buf, width, height, 4).expect("dot survival should succeed");
        let bg_idx = raster.indices[0];
        let dot_idx = raster.indices[11 * width + 11];
        assert_ne!(bg_idx, dot_idx);
        let dot_rgb = raster.palette[usize::from(dot_idx)];
        assert!(dot_rgb.r > 200 && dot_rgb.b < 80);
    }

    #[test]
    fn smooth_gradient_triggers_adaptive_fallback() {
        let width = 32;
        let height = 16;
        let mut buf = Vec::with_capacity(width * height * 3);
        for _y in 0..height {
            for x in 0..width {
                let v = ((x * 255) / (width - 1)) as u8;
                buf.extend_from_slice(&[v, 255 - v, 128]);
            }
        }
        let raster = quantize_rgb(&buf, width, height, 6).expect("smooth gradient should succeed");
        assert!(!raster.palette.is_empty());
        assert!(raster.palette.len() <= 6);
    }

    #[test]
    fn ringing_and_invalid_inputs_handled() {
        let mut ring = Vec::with_capacity(12 * 6 * 3);
        for _y in 0..6 {
            for x in 0..12 {
                let v = match x {
                    0..=3 => 0,
                    4 => 15,
                    5 => 240,
                    _ => 255,
                };
                ring.extend_from_slice(&[v, v, v]);
            }
        }
        let raster = quantize_rgb(&ring, 12, 6, 4).expect("ringing buffer should succeed");
        assert!(raster.palette.len() <= 4);

        let valid = [0_u8; 12];
        assert!(quantize_rgb(&valid, 2, 2, 0).is_err());
        assert!(quantize_rgb(&valid, 2, 2, 257).is_err());
        assert!(matches!(
            quantize_rgb(&valid[..11], 2, 2, 4),
            Err(InvalidBufferSize)
        ));
        assert!(matches!(
            quantize_rgb(&valid, usize::MAX, 2, 4),
            Err(InvalidBufferSize)
        ));
        assert!(matches!(quantize_rgb(&[], 0, 0, 4), Err(InvalidBufferSize)));
    }
}
