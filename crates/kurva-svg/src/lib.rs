//! SVG serialization stage: vector paths plus palette metadata in, XML document out.

use kurva_types::{ColorRgb, VectorPath};
use thiserror::Error;

/// Failure modes of the SVG emitter.
#[derive(Debug, Error)]
pub enum SvgError {
    #[error("canvas dimensions {width}x{height} must be non-zero")]
    EmptyCanvas { width: usize, height: usize },
    #[error("at least one path references a color outside the provided palette")]
    PaletteOutOfRange,
}

/// Placeholder emitter: writes the canvas envelope; path `d` attributes follow with fitting.
pub fn render_svg(
    paths: &[VectorPath],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
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
    let envelope = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"></svg>"
    );
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::render_svg;

    #[test]
    fn render_svg_emits_envelope_and_rejects_empty_canvas() {
        let document = render_svg(&[], &[], 8, 8);
        assert!(matches!(document, Ok(ref d) if d.starts_with("<svg") && d.ends_with("</svg>")));
        assert!(render_svg(&[], &[], 0, 8).is_err());
    }
}
