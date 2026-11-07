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
///
/// `scale` is the number of decimal digits kept on the coordinate lattice
/// (`2` = 0.01 delta, `3` = 0.001 delta); values outside the supported range
/// are clamped by [`Lattice`]. Higher scales cost bytes but shrink the
/// Hausdorff error between adjacent faces.
pub fn render_svg_with_precision(
    paths: &[VectorPath],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
    scale: u32,
) -> Result<String, SvgError> {
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

    String::from_utf8(out.into_bytes()).map_err(|error| SvgError::Serialization(error.to_string()))
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
mod tests {
    use super::*;
    use kurva_types::{BezierSegment, Point2D};

    fn point(value: (f64, f64)) -> Point2D<f64> {
        Point2D {
            x: value.0,
            y: value.1,
        }
    }

    fn cubic(
        start: (f64, f64),
        first: (f64, f64),
        second: (f64, f64),
        end: (f64, f64),
    ) -> BezierSegment {
        BezierSegment {
            p0: point(start),
            p1: point(first),
            p2: point(second),
            p3: point(end),
        }
    }

    fn square(color_id: u8) -> VectorPath {
        VectorPath {
            color_id,
            is_closed: true,
            segments: vec![
                cubic((0.0, 0.0), (2.0, 0.0), (6.0, 0.0), (8.0, 0.0)),
                cubic((8.0, 0.0), (8.0, 2.0), (8.0, 6.0), (8.0, 8.0)),
                cubic((8.0, 8.0), (6.0, 8.0), (2.0, 8.0), (0.0, 8.0)),
                cubic((0.0, 8.0), (0.0, 6.0), (0.0, 2.0), (0.0, 0.0)),
            ],
        }
    }

    fn ink() -> ColorRgb {
        ColorRgb { r: 0, g: 0, b: 0 }
    }

    fn paper() -> ColorRgb {
        ColorRgb {
            r: 255,
            g: 255,
            b: 255,
        }
    }

    #[test]
    fn canvas_and_palette_validation() {
        let empty = render_svg(&[], &[], 0, 8);
        assert!(matches!(empty, Err(SvgError::EmptyCanvas { width: 0, .. })));
        let flat = render_svg(&[], &[], 8, 0);
        assert!(matches!(flat, Err(SvgError::EmptyCanvas { height: 0, .. })));

        let palette = [ink()];
        let stray = render_svg(&[square(1)], &palette, 8, 8);
        assert!(matches!(stray, Err(SvgError::PaletteOutOfRange)));
        assert!(render_svg(&[square(0)], &palette, 8, 8).is_ok());
    }

    #[test]
    fn compound_path_layer_grouping() {
        let document = render_svg(&[square(0), square(0), square(0)], &[ink()], 8, 8).unwrap();
        assert_eq!(document.matches("<path ").count(), 1);
        assert_eq!(document.matches("fill-rule=\"nonzero\"").count(), 1);
        assert!(document.contains("fill=\"#000000\""));
    }

    #[test]
    fn base_underlay_precedes_the_layers() {
        let document = render_svg(&[square(1), square(0)], &[ink(), paper()], 8, 8).unwrap();
        let rect = document
            .find("<rect")
            .expect("dominant layer emits a base rect");
        let path = document.find("<path").expect("layers emit paths");
        assert!(rect < path);
        assert!(document.contains("<rect width=\"8\" height=\"8\" fill=\"#000000\"/>"));
    }

    #[test]
    fn bit_identical_deterministic_output() {
        let paths = [
            square(0),
            VectorPath {
                color_id: 1,
                is_closed: false,
                segments: vec![
                    cubic((1.0, 1.0), (2.5, 0.25), (5.5, 7.75), (7.0, 3.0)),
                    cubic((7.0, 3.0), (6.0, 5.0), (3.0, 6.0), (1.5, 2.5)),
                ],
            },
        ];
        let palette = [ink(), paper()];
        let first = render_svg(&paths, &palette, 8, 8).expect("valid canvas");
        let second = render_svg(&paths, &palette, 8, 8).expect("valid canvas");
        assert_eq!(first, second);
        assert!(first.starts_with("<svg "));
        assert!(first.ends_with("</svg>"));
    }

    #[test]
    fn render_svg_emits_envelope_and_rejects_empty_canvas() {
        let document = render_svg(&[], &[], 8, 8);
        assert!(matches!(document, Ok(ref d) if d.starts_with("<svg") && d.ends_with("</svg>")));
        assert!(render_svg(&[], &[], 0, 8).is_err());
    }
}
