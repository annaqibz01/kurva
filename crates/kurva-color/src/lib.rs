//! PAPHQ colour quantiser: variational phase-field relaxation in Oklab.
//!
//! Phases relax over a full multigrid V-cycle, then threshold into palette labels.

pub mod error;
pub mod multigrid;
pub mod oklab;
pub mod palette;
pub mod phase;
pub mod stencil;
pub mod tensor;

pub use error::{ColorError, ColorError::InvalidBufferSize};

use kurva_types::{ColorRgb, IndexedRaster};
use multigrid::{Pyramid, ACTIVE_ETA, EPSILON_SCALE, SMOOTH_STEPS};
use palette::TAU_AREA_RATIO;
use phase::{PhaseField, INIT_TEMPERATURE};

/// Largest palette an indexed raster can address through its `u8` index buffer.
pub const MAX_PALETTE_SIZE: usize = 256;
const COLLAPSE_THRESHOLD: f32 = 0.995;

/// Quantizes an interleaved RGB8 buffer (`width * height * 3` bytes) into an
/// indexed raster whose palette follows perceptual Oklab distance.
pub fn quantize_rgb(
    rgb_bytes: &[u8],
    width: usize,
    height: usize,
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    if !(1..=MAX_PALETTE_SIZE).contains(&max_colors) {
        return Err(ColorError::InvalidPaletteSize(max_colors));
    }
    let pixels = width.checked_mul(height).ok_or(InvalidBufferSize)?;
    if width == 0 || height == 0 || pixels.checked_mul(3) != Some(rgb_bytes.len()) {
        return Err(ColorError::InvalidBufferSize);
    }

    let seeds = max_colors.min(pixels);
    let pyramid = Pyramid::build(oklab::raster(rgb_bytes), width, height, seeds);
    let mut palette = palette::seed(&pyramid.levels[0].data, seeds);
    let mut field: Option<PhaseField> = None;
    for level in (0..=pyramid.coarsest()).rev() {
        let (w, h) = (pyramid.levels[level].w, pyramid.levels[level].h);
        let data = &pyramid.levels[level].data;
        let mut current = match field.take() {
            Some(coarse) if coarse.k == palette.len() => multigrid::prolongate(&coarse, w, h),
            _ => palette::initialise(data, (w, h), &palette, INIT_TEMPERATURE),
        };
        if palette.len() > 1 && current.collapsed(COLLAPSE_THRESHOLD) {
            current = palette::initialise(data, (w, h), &palette, INIT_TEMPERATURE);
        }
        let stencil = stencil::assemble(&tensor::diffusion_field(data, w, h), w, h);
        let eps = EPSILON_SCALE[level];
        let mask = (level == 0).then(|| multigrid::active_mask(&current, ACTIVE_ETA));
        for _ in 0..SMOOTH_STEPS[level] {
            phase::step(&mut current, &stencil, data, &palette, eps, mask.as_deref());
        }
        if level == 0 {
            let floor = (TAU_AREA_RATIO * (w * h) as f32).max(1.0);
            palette::condense(&mut current, &mut palette, floor);
            palette = palette::centroids(&current, data, &palette);
        }
        current.normalise();
        field = Some(current);
    }

    let Some(field) = field else {
        return Err(ColorError::InvalidBufferSize);
    };
    let entries: Vec<ColorRgb> = palette.iter().map(|lab| oklab::to_srgb(*lab)).collect();
    IndexedRaster::new(width, height, entries, palette::labels(&field))
        .ok_or(ColorError::InvalidBufferSize)
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

    fn checker(width: usize, height: usize) -> Vec<u8> {
        (0..width * height)
            .flat_map(|pixel| {
                let dark = (pixel % width + pixel / width).is_multiple_of(2);
                [if dark { 8 } else { 248 }; 3]
            })
            .collect()
    }

    fn ringing(side: usize) -> Vec<u8> {
        (0..side * side)
            .map(|pixel| (60 + (pixel * 37) % 150) as u8)
            .flat_map(|level| [level, level.saturating_sub(6), level.saturating_add(4)])
            .collect()
    }

    fn message_of(pixels: &[u8], dims: (usize, usize), colors: usize) -> String {
        let error = quantize_rgb(pixels, dims.0, dims.1, colors).err();
        error.map(|error| error.to_string()).unwrap_or_default()
    }

    #[test]
    fn end_to_end_quantize() {
        let flat = quantize_rgb(&[40, 90, 160].repeat(108), 12, 9, 8).expect("solid image");
        assert_eq!((flat.width, flat.height, flat.palette.len()), (12, 9, 1));
        assert!(flat.indices.iter().all(|index| *index == 0));
        let entry = flat.palette[0];
        assert!(entry.r.abs_diff(40) <= 1 && entry.g.abs_diff(90) <= 1);
        assert!(entry.b.abs_diff(160) <= 1);

        let board = quantize_rgb(&checker(24, 24), 24, 24, 4).expect("checkerboard");
        assert!(board.palette.len() >= 2);
        assert_ne!(board.indices[0], board.indices[1]);
        assert!(board
            .indices
            .iter()
            .all(|i| usize::from(*i) < board.palette.len()));

        let noisy = quantize_rgb(&ringing(20), 20, 20, 8).expect("ringing buffer");
        assert_eq!(noisy.indices.len(), 400);
        assert!(!noisy.palette.is_empty() && noisy.palette.len() <= 8);
        assert!(noisy
            .indices
            .iter()
            .all(|i| usize::from(*i) < noisy.palette.len()));
    }

    #[test]
    fn inputs_are_validated() {
        let pixels = [0u8; 12];
        assert!(message_of(&pixels, (2, 2), 0).contains("palette size 0"));
        assert!(message_of(&pixels, (2, 2), 257).contains("palette size 257"));
        assert!(message_of(&[0, 0, 0], (2, 2), 4).contains("buffer size"));
        assert!(message_of(&[], (usize::MAX, 2), 4).contains("buffer size"));
        assert!(message_of(&[], (0, 0), 4).contains("buffer size"));
    }
}
