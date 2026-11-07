#![forbid(unsafe_code)]

pub mod error;
pub mod paphq;
pub mod prefilter;
pub mod regularize;

pub use error::ColorError;
pub use kurva_types::{ColorRgb, IndexedRaster};

use paphq::{MultigridArena, PaphqSolver};
use prefilter::{OklabImage, PreFilterScratch};
use regularize::RegularizerScratch;

pub const MAX_PALETTE_SIZE: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Logo,
    Illustration,
}

#[derive(Clone, Copy, Debug)]
pub struct PaphqConfig {
    pub gf_radius: usize,
    pub gf_eps: f32,
    pub shock_iters: usize,
    pub shock_threshold: f32,
    pub epsilon: f32,
    pub lambda_base: f32,
    pub beta_ridge: f32,
    pub kappa_normal: f32,
    pub kappa_corner: f32,
    pub max_mg_levels: usize,
    pub v_cycle_sweeps: usize,
    pub min_island_area: u32,
    pub min_stroke_area: u32,
    pub min_stroke_elongation: f32,
}

impl PaphqConfig {
    pub const fn from_preset(preset: Preset) -> Self {
        match preset {
            Preset::Logo => Self {
                gf_radius: 2,
                gf_eps: 0.0016,
                shock_iters: 2,
                shock_threshold: 0.008,
                epsilon: 0.45,
                lambda_base: 42.0,
                beta_ridge: 2.5,
                kappa_normal: 0.035,
                kappa_corner: 0.025,
                max_mg_levels: 3,
                v_cycle_sweeps: 3,
                min_island_area: 16,
                min_stroke_area: 6,
                min_stroke_elongation: 5.5,
            },
            Preset::Illustration => Self {
                gf_radius: 1,
                gf_eps: 0.0006,
                shock_iters: 1,
                shock_threshold: 0.015,
                epsilon: 0.32,
                lambda_base: 65.0,
                beta_ridge: 4.0,
                kappa_normal: 0.022,
                kappa_corner: 0.020,
                max_mg_levels: 3,
                v_cycle_sweeps: 3,
                min_island_area: 8,
                min_stroke_area: 5,
                min_stroke_elongation: 4.5,
            },
        }
    }
}

/// Reusable workspace eliminating heap allocations across frames or runs.
pub struct KurvaColorWorkspace {
    pub oklab: OklabImage,
    pub prefilter_scratch: PreFilterScratch,
    pub mg_arena: MultigridArena,
    pub reg_scratch: RegularizerScratch,
}

impl KurvaColorWorkspace {
    pub fn new(max_width: usize, max_height: usize, max_palette: usize, max_levels: usize) -> Self {
        Self {
            oklab: OklabImage::new(max_width, max_height),
            prefilter_scratch: PreFilterScratch::new(max_width, max_height),
            mg_arena: MultigridArena::new(max_width, max_height, max_palette, max_levels),
            reg_scratch: RegularizerScratch::new(max_width, max_height, max_palette),
        }
    }

    pub fn quantize_dirty_raster(
        &mut self,
        rgb_pixels: &[u8],
        width: usize,
        height: usize,
        palette: Vec<ColorRgb>,
        config: &PaphqConfig,
    ) -> Result<IndexedRaster, ColorError> {
        let k = palette.len();
        if !(1..=MAX_PALETTE_SIZE).contains(&k) {
            return Err(ColorError::InvalidPaletteSize(k));
        }
        if rgb_pixels.len() != width * height * 3 {
            return Err(ColorError::InvalidBufferSize);
        }

        self.oklab.resize_and_convert_srgb(rgb_pixels, width, height);
        let palette_oklab = prefilter::palette_to_oklab(&palette);

        // Stage 1: O(N) Separable Guided Filter + Upwind Morphological Shock Filter
        prefilter::run_prefilter(
            &mut self.oklab,
            &mut self.prefilter_scratch,
            config.gf_radius,
            config.gf_eps,
            config.shock_iters,
            config.shock_threshold,
        );

        // Stage 2: Anisotropic Corner-Gated PAPHQ FMG Solver
        let mut indices = vec![0u8; width * height];
        PaphqSolver::solve(
            &self.oklab,
            &palette_oklab,
            config,
            &mut self.mg_arena,
            &mut indices,
        );

        // Stage 3: Discrete Spur Detangling & Elongation-Guarded Island Absorption
        regularize::regularize_indexed_raster(
            &mut indices,
            width,
            height,
            &palette_oklab,
            config,
            &mut self.reg_scratch,
        );

        IndexedRaster::new(width, height, palette, indices).ok_or(ColorError::InvalidBufferSize)
    }
}

/// Seeds palette centroids along the perceptual Oklab lightness axis with deduplication.
pub fn seed_palette(rgb_bytes: &[u8], max_colors: usize) -> Vec<ColorRgb> {
    let n_pixels = rgb_bytes.len() / 3;
    if n_pixels == 0 || max_colors == 0 {
        return Vec::new();
    }

    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    let step = (n_pixels / 20000).max(1);

    for i in (0..n_pixels).step_by(step) {
        let c = ColorRgb::new(
            rgb_bytes[i * 3],
            rgb_bytes[i * 3 + 1],
            rgb_bytes[i * 3 + 2],
        );
        if seen.insert((c.r, c.g, c.b)) {
            unique.push(c);
            if unique.len() >= 1024 {
                break;
            }
        }
    }

    if unique.len() <= max_colors {
        return unique;
    }

    let mut samples: Vec<(f32, ColorRgb)> = unique
        .into_iter()
        .map(|c| {
            let ok = prefilter::srgb_to_oklab(c.r, c.g, c.b);
            (ok.l, c)
        })
        .collect();

    samples.sort_by(|a, b| a.0.total_cmp(&b.0));

    let count = samples.len();
    let k = max_colors.min(count).min(MAX_PALETTE_SIZE);
    let mut palette = Vec::with_capacity(k);
    let mut pal_seen = std::collections::HashSet::new();

    for stratum in 0..k {
        let pos = ((stratum as f32 + 0.5) / k as f32 * count as f32) as usize;
        let c = samples[pos.min(count - 1)].1;
        if pal_seen.insert((c.r, c.g, c.b)) {
            palette.push(c);
        }
    }

    if palette.len() < k {
        for (_, c) in &samples {
            if pal_seen.insert((c.r, c.g, c.b)) {
                palette.push(*c);
                if palette.len() == k {
                    break;
                }
            }
        }
    }

    if palette.is_empty() && !samples.is_empty() {
        palette.push(samples[0].1);
    }

    palette
}

/// Quantizes an interleaved RGB buffer into an IndexedRaster.
pub fn quantize_rgb(
    rgb_bytes: &[u8],
    width: usize,
    height: usize,
    max_colors: usize,
) -> Result<IndexedRaster, ColorError> {
    if !(1..=MAX_PALETTE_SIZE).contains(&max_colors) {
        return Err(ColorError::InvalidPaletteSize(max_colors));
    }
    let pixels = width.checked_mul(height).ok_or(ColorError::InvalidBufferSize)?;
    if width == 0 || height == 0 || pixels.checked_mul(3) != Some(rgb_bytes.len()) {
        return Err(ColorError::InvalidBufferSize);
    }

    let palette = seed_palette(rgb_bytes, max_colors);
    if palette.is_empty() {
        return Err(ColorError::InvalidBufferSize);
    }

    let config = PaphqConfig::from_preset(Preset::Logo);
    let mut ws = KurvaColorWorkspace::new(width, height, palette.len(), config.max_mg_levels);
    ws.quantize_dirty_raster(rgb_bytes, width, height, palette, &config)
}

/// Decodes an image from memory and quantizes it into an IndexedRaster.
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
