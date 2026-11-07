//! Thin one-shot orchestrator chaining the pipeline crates in sequence.
//!
//! Input image -> kurva-color -> kurva-contour -> kurva-poly -> kurva-fit -> kurva-svg.

mod args;

use std::process::ExitCode;

use anyhow::{Context, Result};

use crate::args::Args;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let Some(args) = args::parse(std::env::args().skip(1).collect())? else {
        return Ok(());
    };
    let (svg, paths, colors) = vectorize(&args)?;
    std::fs::write(&args.output, svg).with_context(|| {
        format!(
            "[ERROR] Failed to write SVG output: {}",
            args.output.display()
        )
    })?;
    println!(
        "[INFO] kurva: {} -> {} ({} paths, {} colors)",
        args.input.display(),
        args.output.display(),
        paths,
        colors
    );
    Ok(())
}

/// Run the five pipeline stages and return the document with its path and color counts.
fn vectorize(args: &Args) -> Result<(String, usize, usize)> {
    let image_bytes = std::fs::read(&args.input).with_context(|| {
        format!(
            "[ERROR] Failed to read input file: {}",
            args.input.display()
        )
    })?;
    let raster = kurva_color::load_and_quantize_image(&image_bytes, args.colors)
        .context("[ERROR] Color quantization failed")?;
    let contours =
        kurva_contour::extract_contours(&raster).context("[ERROR] Contour extraction failed")?;
    let polylines = kurva_poly::simplify_contours(&contours, args.epsilon, args.corner_threshold)
        .context("[ERROR] Polygon simplification failed")?;
    let paths = kurva_fit::fit_polylines(&polylines, args.error_threshold)
        .context("[ERROR] Bezier curve fitting failed")?;
    let svg = kurva_svg::render_svg(&paths, &raster.palette, raster.width, raster.height)
        .context("[ERROR] SVG rendering failed")?;
    Ok((svg, paths.len(), raster.palette.len()))
}
