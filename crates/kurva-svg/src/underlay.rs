//! Layer grouping, deterministic ordering and the gapless base underlay.

use kurva_types::{ColorRgb, VectorPath};

use crate::ascii::IntToFixedASCII;
use crate::lattice::Lattice;

/// Every path that shares one palette slot.
pub(crate) struct Layer<'a> {
    pub(crate) color_id: u8,
    pub(crate) paths: Vec<&'a VectorPath>,
    /// Twice the enclosed area, in lattice units squared.
    double_area: i128,
}

/// Groups `paths` by color id, largest layer first.
pub(crate) fn group<'a>(paths: &'a [VectorPath], lattice: Lattice) -> Vec<Layer<'a>> {
    let mut ordered: Vec<&VectorPath> = paths.iter().collect();
    ordered.sort_by_key(|path| path.color_id);

    let mut layers: Vec<Layer<'a>> = Vec::new();
    for path in ordered {
        let area = double_area(path, lattice);
        let joined = match layers.last_mut() {
            Some(layer) if layer.color_id == path.color_id => {
                layer.paths.push(path);
                layer.double_area += area;
                true
            }
            _ => false,
        };
        if !joined {
            layers.push(Layer {
                color_id: path.color_id,
                paths: vec![path],
                double_area: area,
            });
        }
    }

    layers.sort_by(|left, right| {
        right
            .double_area
            .cmp(&left.double_area)
            .then(left.color_id.cmp(&right.color_id))
    });
    layers
}

/// Base underlay color when a single layer covers at least half the canvas.
pub(crate) fn background(
    layers: &[Layer<'_>],
    palette: &[ColorRgb],
    width: usize,
    height: usize,
    lattice: Lattice,
) -> Option<ColorRgb> {
    let dominant = layers.first()?;
    let scale = i128::from(lattice.divisor());
    let canvas = width as i128 * height as i128 * scale * scale;
    if dominant.double_area < canvas {
        return None;
    }
    palette.get(usize::from(dominant.color_id)).copied()
}

/// Emits the full-canvas base rectangle that defeats background bleed.
pub(crate) fn emit_rect(out: &mut IntToFixedASCII, width: usize, height: usize, color: ColorRgb) {
    out.push_bytes(b"<rect width=\"");
    out.push_unsigned(width as u64);
    out.push_bytes(b"\" height=\"");
    out.push_unsigned(height as u64);
    out.push_bytes(b"\" fill=\"");
    out.push_hex(color);
    out.push_bytes(b"\"/>");
}

fn double_area(path: &VectorPath, lattice: Lattice) -> i128 {
    let Some(first) = path.segments.first() else {
        return 0;
    };
    let start = lattice.point(first.p0.x, first.p0.y);
    let mut previous = start;
    let mut total = 0_i128;
    for segment in &path.segments {
        let end = lattice.point(segment.p3.x, segment.p3.y);
        total +=
            i128::from(previous.x) * i128::from(end.y) - i128::from(previous.y) * i128::from(end.x);
        previous = end;
    }
    total +=
        i128::from(previous.x) * i128::from(start.y) - i128::from(previous.y) * i128::from(start.x);
    total.abs()
}
