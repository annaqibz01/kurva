use super::support::{point, polyline};
use crate::fit_polylines;

#[test]
fn closed_square_produces_four_exact_collinear_segments() {
    let square = polyline(
        vec![
            point(0.0, 0.0),
            point(5.0, 0.0),
            point(10.0, 0.0),
            point(10.0, 10.0),
            point(0.0, 10.0),
        ],
        vec![0, 2, 3, 4],
        true,
    );
    let paths = fit_polylines(&[square], 0.5).expect("square should fit");
    assert_eq!(paths.len(), 1);
    let segs = &paths[0].segments;
    assert_eq!(segs.len(), 4);

    let expected_corners = [
        point(0.0, 0.0),
        point(10.0, 0.0),
        point(10.0, 10.0),
        point(0.0, 10.0),
    ];
    for (idx, seg) in segs.iter().enumerate() {
        let p0 = expected_corners[idx];
        let p3 = expected_corners[(idx + 1) % 4];
        assert_eq!(seg.p0, p0);
        assert_eq!(seg.p3, p3);
        let dx = p3.x - p0.x;
        let dy = p3.y - p0.y;
        assert!((seg.p1.x - (p0.x + dx / 3.0)).abs() < 1.0e-12);
        assert!((seg.p1.y - (p0.y + dy / 3.0)).abs() < 1.0e-12);
        assert!((seg.p2.x - (p0.x + 2.0 * dx / 3.0)).abs() < 1.0e-12);
        assert!((seg.p2.y - (p0.y + 2.0 * dy / 3.0)).abs() < 1.0e-12);
    }
}

#[test]
fn shared_curved_chain_produces_exact_reversed_twin_segments() {
    let mut curve_ab = Vec::new();
    for step in 0..=8 {
        let t = step as f64 / 8.0;
        let x = 20.0 * t;
        let y = 6.0 * (std::f64::consts::PI * t).sin();
        curve_ab.push(point(x, y));
    }

    let mut poly_a_pts = curve_ab.clone();
    poly_a_pts.push(point(20.0, 20.0));
    poly_a_pts.push(point(0.0, 20.0));
    let poly_a = polyline(poly_a_pts, vec![0, 8, 9, 10], true);

    let mut poly_b_pts: Vec<_> = curve_ab.iter().copied().rev().collect();
    poly_b_pts.push(point(0.0, -20.0));
    poly_b_pts.push(point(20.0, -20.0));
    let mut poly_b = polyline(poly_b_pts, vec![0, 8, 9, 10], true);
    poly_b.color_id = 1;

    let paths = fit_polylines(&[poly_a, poly_b], 0.25).expect("adjacent regions should fit");
    assert_eq!(paths.len(), 2);

    // Chain 0 in poly_a is 0 -> 8; the last chain in poly_b? No, in poly_b, corner 0..=8 is also chain 0!
    let a_shared: Vec<_> = paths[0]
        .segments
        .iter()
        .take_while(|seg| seg.p0.y <= 6.5 && seg.p3.y <= 6.5)
        .copied()
        .collect();
    let b_shared: Vec<_> = paths[1]
        .segments
        .iter()
        .take_while(|seg| seg.p0.y >= -0.1 && seg.p3.y >= -0.1)
        .copied()
        .collect();

    assert!(!a_shared.is_empty());
    assert_eq!(a_shared.len(), b_shared.len());
    for (seg_a, seg_b) in a_shared.iter().zip(b_shared.iter().rev()) {
        assert_eq!(seg_a.p0, seg_b.p3);
        assert_eq!(seg_a.p1, seg_b.p2);
        assert_eq!(seg_a.p2, seg_b.p1);
        assert_eq!(seg_a.p3, seg_b.p0);
    }
}
