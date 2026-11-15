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

#[test]
fn write_svg_matches_render_svg_and_compresses_collinear_cubics() {
    let diag = VectorPath {
        color_id: 0,
        is_closed: true,
        segments: vec![
            cubic((0.0, 0.0), (1.0 / 3.0, 2.0 / 3.0), (2.0 / 3.0, 4.0 / 3.0), (1.0, 2.0)),
            cubic((1.0, 2.0), (1.0, 4.0), (1.0, 6.0), (1.0, 8.0)),
            cubic((1.0, 8.0), (2.0 / 3.0, 8.0), (1.0 / 3.0, 8.0), (0.0, 8.0)),
            cubic((0.0, 8.0), (0.0, 6.0), (0.0, 2.0), (0.0, 0.0)),
        ],
    };
    let paths = [square(1), diag];
    let palette = [ink(), paper()];
    let rendered = render_svg(&paths, &palette, 8, 8).expect("render_svg");
    let mut streamed = Vec::new();
    write_svg(&mut streamed, &paths, &palette, 8, 8).expect("write_svg");
    assert_eq!(streamed, rendered.as_bytes());
    let path_start = rendered.find(" d=\"").expect("path d attribute");
    let path_data = &rendered[path_start..];
    assert!(!path_data.contains('C') && !path_data.contains('c'));
    assert!(path_data.contains('L') || path_data.contains('l'));
    assert!(
        path_data.contains('H')
            || path_data.contains('h')
            || path_data.contains('V')
            || path_data.contains('v')
    );
}
