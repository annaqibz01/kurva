use super::*;

fn contour(color_id: u8, is_hole: bool, points: &[(f64, f64)]) -> RawContour {
    RawContour {
        color_id,
        points: points.iter().map(|&(x, y)| Point2D::new(x, y)).collect(),
        is_hole,
    }
}

fn pixel_square(size: i32) -> RawContour {
    let mut points = Vec::new();
    for x in 0..=size {
        points.push((f64::from(x), 0.0));
    }
    for y in 1..=size {
        points.push((f64::from(size), f64::from(y)));
    }
    for x in (0..size).rev() {
        points.push((f64::from(x), f64::from(size)));
    }
    for y in (1..size).rev() {
        points.push((0.0, f64::from(y)));
    }
    contour(0, false, &points)
}

fn regular_polygon(sides: usize, radius: f64) -> RawContour {
    let points: Vec<(f64, f64)> = (0..sides)
        .map(|k| {
            let angle = std::f64::consts::TAU * k as f64 / sides as f64;
            (radius * angle.cos(), radius * angle.sin())
        })
        .collect();
    contour(0, false, &points)
}

fn poly_area(points: &[Point2D<f64>]) -> f64 {
    let v: Vec<V2> = points.iter().map(|p| V2::new(p.x, p.y)).collect();
    geometry::signed_area(&v)
}

#[test]
fn degenerate_loops_are_filtered_and_parameters_validated() {
    let one_pt = contour(0, false, &[(0.0, 0.0)]);
    let two_pt = contour(0, false, &[(0.0, 0.0), (1.0, 0.0), (1.0, 0.0)]);
    let out = simplify_contours(&[one_pt, two_pt], 1.0, 120.0);
    assert!(matches!(out, Ok(ref list) if list.is_empty()));
    assert!(simplify_contours(&[], f64::NAN, 120.0).is_err());
}

#[test]
fn parameter_validation_rejects_out_of_range_inputs() {
    let sample = [pixel_square(4)];
    assert!(simplify_contours(&sample, -1.0, 120.0).is_err());
    assert!(simplify_contours(&sample, f64::NAN, 120.0).is_err());
    assert!(simplify_contours(&sample, f64::INFINITY, 120.0).is_err());
    assert!(simplify_contours(&sample, 1.0, 0.0).is_err());
    assert!(simplify_contours(&sample, 1.0, 180.5).is_err());
    assert!(simplify_contours(&sample, 1.0, f64::NAN).is_err());
    assert!(simplify_contours(&sample, 0.0, 180.0).is_ok());
}

#[test]
fn sharp_corner_preservation_keeps_the_four_square_cusps() {
    let out = simplify_contours(&[pixel_square(40)], 1.0, 120.0)
        .unwrap_or_else(|error| panic!("expected success, got {error}"));
    let polyline = &out[0];
    assert_eq!(polyline.points.len(), 4);
    assert_eq!(polyline.corner_indices, vec![0, 1, 2, 3]);
    for point in &polyline.points {
        let on_corner =
            (point.x == 0.0 || point.x == 40.0) && (point.y == 0.0 || point.y == 40.0);
        assert!(on_corner, "corner moved to {point:?}");
    }
}

#[test]
fn reversal_symmetry_invariance() {
    let forward = contour(
        0,
        false,
        &[
            (0.0, 0.0),
            (12.25, 1.1),
            (18.5, 9.0),
            (9.0, 20.0),
            (2.0, 14.0),
            (-6.0, 7.0),
        ],
    );
    let mut reversed_points = forward.points.clone();
    reversed_points.reverse();
    let reversed = RawContour {
        color_id: forward.color_id,
        points: reversed_points,
        is_hole: false,
    };
    let left = simplify_contours(&[forward], 1.0, 120.0)
        .unwrap_or_else(|error| panic!("expected success, got {error}"))
        .remove(0);
    let right = simplify_contours(&[reversed], 1.0, 120.0)
        .unwrap_or_else(|error| panic!("expected success, got {error}"))
        .remove(0);
    assert_eq!(left.points, right.points);
    assert_eq!(left.corner_indices, right.corner_indices);
}

#[test]
fn smooth_circle_no_false_cusps() {
    let out = simplify_contours(&[regular_polygon(32, 10.0)], 1.0, 120.0)
        .unwrap_or_else(|error| panic!("expected success, got {error}"));
    let polyline = &out[0];
    assert!(polyline.corner_indices.is_empty(), "false cusps detected");
    assert!(polyline.points.len() >= 3 && polyline.points.len() < 32);
}

#[test]
fn triple_junction_locks_shared_vertex_as_cusp_across_all_regions() {
    let top = contour(
        0,
        false,
        &[(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0), (0.0, 10.0)],
    );
    let bottom_left = contour(1, false, &[(0.0, 10.0), (10.0, 10.0), (10.0, 20.0), (0.0, 20.0)]);
    let bottom_right = contour(
        2,
        false,
        &[(10.0, 10.0), (20.0, 10.0), (20.0, 20.0), (10.0, 20.0)],
    );
    let out = simplify_contours(&[top, bottom_left, bottom_right], 2.0, 120.0)
        .unwrap_or_else(|error| panic!("expected success, got {error}"));
    assert_eq!(out.len(), 3);
    for polyline in &out {
        let idx = polyline
            .points
            .iter()
            .position(|p| (p.x - 10.0).abs() < 1.0e-9 && (p.y - 10.0).abs() < 1.0e-9)
            .expect("triple-junction vertex must be preserved without drift");
        assert!(
            polyline.corner_indices.contains(&idx),
            "triple-junction vertex must be locked in corner_indices"
        );
    }
}

#[test]
fn shell_and_hole_enforce_opposite_winding_for_nonzero_fill() {
    let shell_cw = contour(0, false, &[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 0.0)]);
    let hole_ccw = contour(0, true, &[(2.0, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)]);
    let out = simplify_contours(&[shell_cw, hole_ccw], 0.5, 115.0)
        .unwrap_or_else(|error| panic!("expected success, got {error}"));
    assert_eq!(out.len(), 2);
    assert!(
        poly_area(&out[0].points) > 0.0,
        "outer shell must wind positively"
    );
    assert!(
        poly_area(&out[1].points) < 0.0,
        "inner hole must wind negatively"
    );
}
