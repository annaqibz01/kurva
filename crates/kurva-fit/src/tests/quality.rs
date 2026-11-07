//! Fitting-quality tests: fidelity, continuity and symmetry of the produced paths.

use super::support::{arc_point, curve_deviation, max_sample_error, point, polyline};
use crate::curve::reverse_segment;
use crate::fit_polylines;
use crate::vec2::{cross, dot, norm, sub};
use kurva_types::Point2D;

#[test]
fn exact_straight_line_fit() {
    let input = polyline(
        vec![point(0.0, 0.0), point(5.0, 0.0), point(10.0, 0.0)],
        Vec::new(),
        false,
    );
    let Ok(paths) = fit_polylines(std::slice::from_ref(&input), 0.5) else {
        panic!("straight line fitting failed");
    };
    let segments = &paths[0].segments;
    assert_eq!(segments.len(), 1);
    let segment = segments[0];
    for control in [segment.p0, segment.p1, segment.p2, segment.p3] {
        assert!(
            control.y.abs() < 1e-10,
            "span is not collinear with the chord"
        );
    }
    assert!(segment.p0.x.abs() < 1e-10);
    assert!((segment.p3.x - 10.0).abs() < 1e-10);
    assert!(max_sample_error(segments, &input.points) < 1e-10);
    assert!(curve_deviation(segments, &input.points) < 1e-10);
}

#[test]
fn sharp_corner_tangent_discontinuity() {
    let input = polyline(
        vec![point(0.0, 0.0), point(10.0, 0.0), point(10.0, 10.0)],
        vec![1],
        false,
    );
    let Ok(paths) = fit_polylines(&[input], 0.5) else {
        panic!("corner fitting failed");
    };
    let segments = &paths[0].segments;
    assert_eq!(segments.len(), 2);
    let joint = segments[0].p3;
    assert!((joint.x - 10.0).abs() < 1e-9 && joint.y.abs() < 1e-9);
    assert!((segments[1].p0.x - joint.x).abs() < 1e-12);
    assert!((segments[1].p0.y - joint.y).abs() < 1e-12);
    let incoming = sub(segments[0].p3, segments[0].p2);
    let outgoing = sub(segments[1].p1, segments[1].p0);
    let scale = norm(incoming) * norm(outgoing);
    assert!(scale > 0.0);
    assert!((cross(incoming, outgoing) / scale).abs() > 0.5);
}

#[test]
fn smooth_arc_g1_continuity() {
    // A dense sampling keeps the polyline a faithful proxy for the circle, so the
    // tolerance is below the single-span error and a split is genuinely required.
    let threshold = 0.002;
    let samples: Vec<Point2D<f64>> = (0..=64)
        .map(|step| arc_point(10.0, step as f64 * std::f64::consts::FRAC_PI_2 / 64.0))
        .collect();
    let input = polyline(samples.clone(), Vec::new(), false);
    let Ok(paths) = fit_polylines(&[input], threshold) else {
        panic!("arc fitting failed");
    };
    let segments = &paths[0].segments;
    assert!(segments.len() >= 2, "threshold should force a split");
    for pair in segments.windows(2) {
        let incoming = sub(pair[0].p3, pair[0].p2);
        let outgoing = sub(pair[1].p1, pair[1].p0);
        let first = norm(incoming);
        let second = norm(outgoing);
        assert!(first > 0.0 && second > 0.0);
        let cosine = dot(incoming, outgoing) / (first * second);
        assert!(
            (cosine - 1.0).abs() < 1e-9,
            "joint tangents are not collinear"
        );
    }
    let measured = max_sample_error(segments, &samples);
    assert!(
        measured <= threshold,
        "deviation {measured} exceeds {threshold}"
    );
}

#[test]
fn bilateral_reversal_equivariance() {
    let samples = vec![
        point(0.0, 0.0),
        point(2.0, 1.0),
        point(5.0, 1.6),
        point(8.0, 0.4),
        point(10.0, -2.0),
        point(11.5, -5.0),
    ];
    let mut mirrored = samples.clone();
    mirrored.reverse();
    let forward = polyline(samples, Vec::new(), false);
    let backward = polyline(mirrored, Vec::new(), false);
    let Ok(a) = fit_polylines(std::slice::from_ref(&forward), 0.02) else {
        panic!("forward fitting failed");
    };
    let Ok(b) = fit_polylines(std::slice::from_ref(&backward), 0.02) else {
        panic!("reversed fitting failed");
    };
    assert_eq!(a[0].segments.len(), b[0].segments.len());
    for (front, back) in a[0].segments.iter().zip(b[0].segments.iter().rev()) {
        let expected = reverse_segment(*front);
        for (left, right) in [
            (expected.p0, back.p0),
            (expected.p1, back.p1),
            (expected.p2, back.p2),
            (expected.p3, back.p3),
        ] {
            assert!((left.x - right.x).abs() < 1e-12);
            assert!((left.y - right.y).abs() < 1e-12);
        }
    }
}

#[test]
fn hausdorff_tolerance_enforced() {
    let threshold = 0.02;
    let samples: Vec<Point2D<f64>> = (0..=40)
        .map(|step| {
            let turn = step as f64 * std::f64::consts::FRAC_PI_2 / 40.0;
            arc_point(10.0 + 1.5 * (step as f64 * 0.3).sin(), turn)
        })
        .collect();
    let input = polyline(samples.clone(), Vec::new(), false);
    let Ok(paths) = fit_polylines(&[input], threshold) else {
        panic!("tolerance fitting failed");
    };
    let segments = &paths[0].segments;
    let fidelity = max_sample_error(segments, &samples);
    assert!(
        fidelity <= threshold,
        "sample deviation {fidelity} exceeds {threshold}"
    );
    let bulge = curve_deviation(segments, &samples);
    assert!(
        bulge <= threshold,
        "curve deviation {bulge} exceeds {threshold}"
    );
}

#[test]
fn closed_ring_keeps_periodic_continuity() {
    let threshold = 0.05;
    let samples: Vec<Point2D<f64>> = (0..24)
        .map(|step| arc_point(6.0, step as f64 * std::f64::consts::TAU / 24.0))
        .collect();
    let input = polyline(samples.clone(), Vec::new(), true);
    let Ok(paths) = fit_polylines(&[input], threshold) else {
        panic!("ring fitting failed");
    };
    let path = &paths[0];
    assert!(path.is_closed);
    assert!(path.segments.len() >= 4);
    let first = path.segments[0];
    let last = path.segments[path.segments.len() - 1];
    assert!(norm(sub(first.p0, last.p3)) < 1e-9);
    let incoming = sub(last.p3, last.p2);
    let outgoing = sub(first.p1, first.p0);
    let cosine = dot(incoming, outgoing) / (norm(incoming) * norm(outgoing));
    assert!((cosine - 1.0).abs() < 1e-9, "seam is not G1");
    let fidelity = max_sample_error(&path.segments, &samples);
    assert!(fidelity <= threshold, "ring deviation {fidelity}");
}
