//! End-to-end Vector Magic pipeline integration test:
//! quantize_rgb -> extract_contours_with_rgb -> simplify_contours -> fit_polylines -> render_svg.

use kurva_types::{BezierSegment, Point2D};

fn close_pt(a: Point2D<f64>, b: Point2D<f64>) -> bool {
    (a.x - b.x).abs() < 1.0e-6 && (a.y - b.y).abs() < 1.0e-6
}

fn is_twin(a: BezierSegment, b: BezierSegment) -> bool {
    close_pt(a.p0, b.p3) && close_pt(a.p1, b.p2) && close_pt(a.p2, b.p1) && close_pt(a.p3, b.p0)
}

#[test]
fn vector_magic_end_to_end_antialiased_triple_junction() {
    let width = 48usize;
    let height = 48usize;
    let mut rgb = Vec::with_capacity(width * height * 3);

    // Three regions:
    // - Top (y < 24): split diagonally/curved along x = 24 + 4 * sin(pi * y / 24)
    //   with anti-aliasing between Deep Blue (15, 45, 160) and Warm Gold (240, 190, 30).
    // - Bottom (y >= 24): Crimson (195, 30, 50), forming a 3-color triple junction at (24, 24).
    let blue = [15.0_f64, 45.0, 160.0];
    let gold = [240.0_f64, 190.0, 30.0];
    let crimson = [195u8, 30, 50];

    for y in 0..height {
        for x in 0..width {
            if y >= 24 {
                rgb.extend_from_slice(&crimson);
            } else {
                let boundary = 24.0 + 4.0 * (std::f64::consts::PI * (y as f64 + 0.5) / 24.0).sin();
                let coverage = (boundary - (x as f64 + 0.5) + 0.5).clamp(0.0, 1.0);
                let r = (coverage * blue[0] + (1.0 - coverage) * gold[0]).round() as u8;
                let g = (coverage * blue[1] + (1.0 - coverage) * gold[1]).round() as u8;
                let b = (coverage * blue[2] + (1.0 - coverage) * gold[2]).round() as u8;
                rgb.extend_from_slice(&[r, g, b]);
            }
        }
    }

    let raster = kurva_color::quantize_rgb(&rgb, width, height, 8).expect("quantize_rgb");
    assert_eq!(
        raster.palette.len(),
        3,
        "anti-aliased boundary must not create gray halo palette entries"
    );

    let contours =
        kurva_contour::extract_contours_with_rgb(&raster, &rgb).expect("extract_contours_with_rgb");
    assert_eq!(contours.len(), 3);

    let polylines =
        kurva_poly::simplify_contours(&contours, 0.8, 120.0).expect("simplify_contours");
    assert_eq!(polylines.len(), 3);

    let paths = kurva_fit::fit_polylines(&polylines, 0.35).expect("fit_polylines");
    assert_eq!(paths.len(), 3);

    // Shared straight segments meeting at the (24, 24) triple junction must have exact
    // reversed twins on adjacent color regions, and all 3 regions must share the junction.
    let mut twin_matches = 0usize;
    for (i, path_a) in paths.iter().enumerate() {
        for &seg_a in &path_a.segments {
            let found_twin = paths
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .any(|(_, path_b)| path_b.segments.iter().any(|&seg_b| is_twin(seg_a, seg_b)));
            if found_twin {
                twin_matches += 1;
            }
        }
    }
    assert!(twin_matches >= 4, "expected shared interior twin segments, got {twin_matches}");

    let target = Point2D::new(24.0, 24.0);
    let junction = paths[0]
        .segments
        .iter()
        .map(|seg| seg.p0)
        .min_by(|a, b| a.distance_squared(target).total_cmp(&b.distance_squared(target)))
        .expect("path 0 must have segments");
    assert!(
        junction.distance(target) <= 0.5,
        "triple junction drifted too far from (24, 24): {junction:?}"
    );
    for path in &paths {
        assert!(
            path.segments.iter().any(|seg| close_pt(seg.p0, junction)),
            "every region meeting at the triple junction must share {junction:?}"
        );
    }

    let svg = kurva_svg::render_svg(&paths, &raster.palette, width, height).expect("render_svg");
    assert!(svg.starts_with("<svg "));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("<path "));
    assert!(
        svg.contains('L')
            || svg.contains('l')
            || svg.contains('H')
            || svg.contains('h')
            || svg.contains('V')
            || svg.contains('v'),
        "expected compact line commands in SVG output"
    );
}
