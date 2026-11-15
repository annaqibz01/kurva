//! `kurva-svg`: ALTO-v3 zero-allocation, watertight SVG serialization.
//!
//! Coordinates are quantized once onto an integer lattice, formatted straight
//! from their fixed-point form, and emitted through a context-safe command
//! grammar that picks the cheapest legal encoding of every segment. Paths that
//! share a palette slot become one compound path, and a base rectangle is laid
//! underneath whenever a dominant background color covers the canvas.
#![forbid(unsafe_code)]

mod ascii;
mod digits;
mod error;
mod grammar;
mod lattice;
mod segment;
mod underlay;
mod writer;

use kurva_types::{ColorRgb, VectorPath};

pub use error::SvgError;

use ascii::IntToFixedASCII;
use lattice::{Lattice, DEFAULT_SCALE};

/// Serializes `paths` into a standalone SVG document.
///
/// # Errors
///
/// Returns [`SvgError::EmptyCanvas`] when either canvas dimension is zero and
/// [`SvgError::PaletteOutOfRange`] when a path references a missing color.
pub fn render_svg(
    paths: &[VectorPath],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
) -> Result<String, SvgError> {
    render_svg_with_precision(paths, palette, width, height, DEFAULT_SCALE)
}

/// Serializes `paths` with an explicit fixed-point decimal scale.
pub fn render_svg_with_precision(
    paths: &[VectorPath],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
    scale: u32,
) -> Result<String, SvgError> {
    let bytes = serialize_svg_bytes(paths, palette, width, height, scale)?;
    String::from_utf8(bytes).map_err(|error| SvgError::Serialization(error.to_string()))
}

/// Streams a standalone SVG document into `writer` using the default precision.
pub fn write_svg<W: std::io::Write>(
    writer: &mut W,
    paths: &[VectorPath],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
) -> Result<(), SvgError> {
    write_svg_with_precision(writer, paths, palette, width, height, DEFAULT_SCALE)
}

/// Streams a standalone SVG document into `writer` with an explicit fixed-point
/// decimal scale.
pub fn write_svg_with_precision<W: std::io::Write>(
    writer: &mut W,
    paths: &[VectorPath],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
    scale: u32,
) -> Result<(), SvgError> {
    let bytes = serialize_svg_bytes(paths, palette, width, height, scale)?;
    writer.write_all(&bytes)?;
    Ok(())
}

fn serialize_svg_bytes(
    paths: &[VectorPath],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
    scale: u32,
) -> Result<Vec<u8>, SvgError> {
    if width == 0 || height == 0 {
        return Err(SvgError::EmptyCanvas { width, height });
    }
    if paths
        .iter()
        .any(|path| usize::from(path.color_id) >= palette.len())
    {
        return Err(SvgError::PaletteOutOfRange);
    }

    let lattice = Lattice::new(scale);
    let segments: usize = paths.iter().map(|path| path.segments.len()).sum();
    let capacity = 256 + segments.saturating_mul(80) + palette.len().saturating_mul(64);

    let mut out = IntToFixedASCII::with_capacity(capacity, lattice);
    emit_header(&mut out, width, height);

    let layers = underlay::group(paths, lattice);
    if let Some(color) = underlay::background(&layers, palette, width, height, lattice) {
        underlay::emit_rect(&mut out, width, height, color);
    }
    for layer in &layers {
        let Some(fill) = palette.get(usize::from(layer.color_id)).copied() else {
            continue;
        };
        writer::emit_compound(&mut out, &layer.paths, fill, lattice);
    }
    out.push_bytes(b"</svg>");
    Ok(out.into_bytes())
}

fn emit_header(out: &mut IntToFixedASCII, width: usize, height: usize) {
    out.push_bytes(b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"");
    out.push_unsigned(width as u64);
    out.push_bytes(b"\" height=\"");
    out.push_unsigned(height as u64);
    out.push_bytes(b"\" viewBox=\"0 0 ");
    out.push_unsigned(width as u64);
    out.push_bytes(b" ");
    out.push_unsigned(height as u64);
    out.push_bytes(b"\">");
}

#[cfg(test)]
mod tests;
