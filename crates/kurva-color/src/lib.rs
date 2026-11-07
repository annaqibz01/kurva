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
    pub fn for_image(preset: Preset, width: usize, height: usize) -> Self {
        let min_dim = width.min(height);
        let is_tiny = min_dim < 320;

        match preset {
            Preset::Logo => Self {
                // Pada citra mikro (<320px), kecilkan radius filter agar batang huruf tidak lumer
                gf_radius: if is_tiny { 1 } else { 2 },
                gf_eps: if is_tiny { 0.0008 } else { 0.0016 },
                shock_iters: 2,
                shock_threshold: 0.008,
                epsilon: if is_tiny { 0.35 } else { 0.45 },
                lambda_base: 42.0,
                beta_ridge: 2.5,
                kappa_normal: 0.035,
                kappa_corner: 0.025,
                // Batasi level piramida agar fitur sub-piksel tidak hilang di grid kasar
                max_mg_levels: if is_tiny { 2 } else { 3 },
                v_cycle_sweeps: 3,
                min_island_area: if is_tiny { 6 } else { 16 },
                min_stroke_area: if is_tiny { 3 } else { 6 },
                min_stroke_elongation: 5.0,
            },
            Preset::Illustration => Self {
                gf_radius: if is_tiny { 1 } else { 3 },
                gf_eps: 0.0025,
                shock_iters: 3,
                shock_threshold: 0.005,
                epsilon: 0.28,
                lambda_base: 50.0,
                beta_ridge: 3.5,
                kappa_normal: 0.025,
                kappa_corner: 0.022,
                max_mg_levels: if is_tiny { 2 } else { 3 },
                v_cycle_sweeps: 3,
                min_island_area: if is_tiny { 16 } else { 48 },
                min_stroke_area: if is_tiny { 6 } else { 10 },
                min_stroke_elongation: 4.0,
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

        self.oklab
            .resize_and_convert_srgb(rgb_pixels, width, height);
        let palette_oklab = prefilter::palette_to_oklab(&palette);

        // Stage 1: Separable Guided Filter + Morphological Shock Filter
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

/// Ekstraksi palet deterministik 3D berbasis densitas histogram & jarak Oklab.
pub fn seed_palette(rgb_bytes: &[u8], max_colors: usize) -> Vec<ColorRgb> {
    let n_pixels = rgb_bytes.len() / 3;
    if n_pixels == 0 || max_colors == 0 {
        return Vec::new();
    }

    let k = max_colors.min(n_pixels).min(MAX_PALETTE_SIZE);

    let mut bins = std::collections::HashMap::with_capacity(512);
    let step = (n_pixels / 16384).max(1);

    for i in (0..n_pixels).step_by(step) {
        let r = rgb_bytes[i * 3];
        let g = rgb_bytes[i * 3 + 1];
        let b = rgb_bytes[i * 3 + 2];
        let bin_key = ((r as u32 >> 3) << 10) | ((g as u32 >> 3) << 5) | (b as u32 >> 3);

        let entry = bins
            .entry(bin_key)
            .or_insert((0u32, r as u64, g as u64, b as u64));
        entry.0 += 1;
        entry.1 += r as u64;
        entry.2 += g as u64;
        entry.3 += b as u64;
    }

    let mut clusters: Vec<(u32, ColorRgb, prefilter::Oklab)> = bins
        .into_values()
        .map(|(count, r_sum, g_sum, b_sum)| {
            let r = (r_sum / count as u64) as u8;
            let g = (g_sum / count as u64) as u8;
            let b = (b_sum / count as u64) as u8;
            (
                count,
                ColorRgb::new(r, g, b),
                prefilter::srgb_to_oklab(r, g, b),
            )
        })
        .collect();

    clusters.sort_unstable_by(|a, b| b.0.cmp(&a.0));

    let mut palette = Vec::with_capacity(k);
    let mut palette_oklab = Vec::with_capacity(k);

    if let Some(first) = clusters.first() {
        palette.push(first.1);
        palette_oklab.push(first.2);
    }

    while palette.len() < k && palette.len() < clusters.len() {
        let mut best_idx = 0;
        let mut best_score = -1.0f32;

        for (idx, (count, _, ok)) in clusters.iter().enumerate() {
            let mut min_d = f32::INFINITY;
            for p_ok in &palette_oklab {
                let d = ok.dist_sq(*p_ok);
                if d < min_d {
                    min_d = d;
                }
            }

            let score = min_d * ((*count as f32).ln() + 1.0);
            if score > best_score {
                best_score = score;
                best_idx = idx;
            }
        }

        palette.push(clusters[best_idx].1);
        palette_oklab.push(clusters[best_idx].2);
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
    let pixels = width
        .checked_mul(height)
        .ok_or(ColorError::InvalidBufferSize)?;
    if width == 0 || height == 0 || pixels.checked_mul(3) != Some(rgb_bytes.len()) {
        return Err(ColorError::InvalidBufferSize);
    }

    let palette = seed_palette(rgb_bytes, max_colors);
    if palette.is_empty() {
        return Err(ColorError::InvalidBufferSize);
    }

    let config = PaphqConfig::for_image(Preset::Logo, width, height);
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
