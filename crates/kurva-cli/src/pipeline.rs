//! End-to-end vectorization: raster -> palette -> contours -> polylines -> bezier -> SVG.

use std::fs::{self, File};
use std::io::{BufWriter, Write};

use anyhow::{Context, Result};

use crate::args::{CliConfig, Verbosity};
use crate::stats::{RunStats, Stopwatch};

/// Execute every pipeline stage in order and write the SVG document.
pub fn run(config: &CliConfig) -> Result<()> {
    let mut watch = Stopwatch::start();
    let mut stats = RunStats::default();

    let bytes = fs::read(&config.input).with_context(|| {
        format!(
            "[ERROR] Failed to read input file: {}",
            config.input.display()
        )
    })?;
    if config.verbosity == Verbosity::Verbose {
        println!(
            "[INFO] input: {} ({} bytes read)",
            config.input.display(),
            bytes.len()
        );
        if let Some(preset) = config.preset {
            println!("[INFO] preset: {}", preset.name());
        }
    }

    let (raster, rgb_bytes) = kurva_color::load_and_quantize_image_with_rgb(&bytes, config.colors)
        .context("[ERROR] Color quantization failed")?;
    stats.times.color = watch.lap();

    let contours = kurva_contour::extract_contours_with_rgb(&raster, &rgb_bytes)
        .context("[ERROR] Contour extraction failed")?;
    stats.times.contour = watch.lap();

    let polylines =
        kurva_poly::simplify_contours(&contours, config.epsilon, config.corner_threshold)
            .context("[ERROR] Polygon simplification failed")?;
    stats.times.poly = watch.lap();

    let paths = kurva_fit::fit_polylines(&polylines, config.error_threshold)
        .context("[ERROR] Bezier curve fitting failed")?;
    stats.times.fit = watch.lap();

    let file = File::create(&config.output).with_context(|| {
        format!(
            "[ERROR] Failed to create SVG output: {}",
            config.output.display()
        )
    })?;
    let mut writer = BufWriter::new(file);
    kurva_svg::write_svg_with_precision(
        &mut writer,
        &paths,
        &raster.palette,
        raster.width,
        raster.height,
        config.precision,
    )
    .context("[ERROR] SVG rendering failed")?;
    writer.flush().with_context(|| {
        format!(
            "[ERROR] Failed to flush SVG output: {}",
            config.output.display()
        )
    })?;
    stats.times.svg = watch.lap();
    stats.times.total = watch.total();

    let svg_bytes = fs::metadata(&config.output)
        .map(|meta| meta.len() as usize)
        .unwrap_or(0);
    stats.width = raster.width;
    stats.height = raster.height;
    stats.palette = raster.palette.len();
    stats.contours = contours.len();
    stats.vertices = polylines.iter().map(|line| line.points.len()).sum();
    stats.spans = paths.iter().map(|path| path.segments.len()).sum();
    stats.paths = paths.len();
    stats.svg_bytes = svg_bytes;
    stats.report(config.verbosity, &config.input, &config.output);
    Ok(())
}
