//! End-to-end vectorization: raster -> palette -> contours -> polylines -> bezier -> SVG.

use std::fs;

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

    let raster = kurva_color::load_and_quantize_image(&bytes, config.colors)
        .context("[ERROR] Color quantization failed")?;
    stats.times.color = watch.lap();

    let contours =
        kurva_contour::extract_contours(&raster).context("[ERROR] Contour extraction failed")?;
    stats.times.contour = watch.lap();

    let polylines =
        kurva_poly::simplify_contours(&contours, config.epsilon, config.corner_threshold)
            .context("[ERROR] Polygon simplification failed")?;
    stats.times.poly = watch.lap();

    let paths = kurva_fit::fit_polylines(&polylines, config.error_threshold)
        .context("[ERROR] Bezier curve fitting failed")?;
    stats.times.fit = watch.lap();

    let svg = kurva_svg::render_svg_with_precision(
        &paths,
        &raster.palette,
        raster.width,
        raster.height,
        config.precision,
    )
    .context("[ERROR] SVG rendering failed")?;
    stats.times.svg = watch.lap();

    fs::write(&config.output, &svg).with_context(|| {
        format!(
            "[ERROR] Failed to write SVG output: {}",
            config.output.display()
        )
    })?;
    stats.times.total = watch.total();

    stats.width = raster.width;
    stats.height = raster.height;
    stats.palette = raster.palette.len();
    stats.contours = contours.len();
    stats.vertices = polylines.iter().map(|line| line.points.len()).sum();
    stats.spans = paths.iter().map(|path| path.segments.len()).sum();
    stats.paths = paths.len();
    stats.svg_bytes = svg.len();
    stats.report(config.verbosity, &config.input, &config.output);
    Ok(())
}
