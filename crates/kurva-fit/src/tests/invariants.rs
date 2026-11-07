//! Contract and numerical-consistency tests.

use super::support::{arc_point, point, polyline};
use crate::energy::{assemble, Model};
use crate::fit_polylines;
use crate::metric::total;
use crate::vec2::{sub, unit};
use crate::FitError;
use kurva_types::{Point2D, SimplifiedPolyline};

#[test]
fn parameter_validation() {
    let shape = polyline(vec![point(0.0, 0.0), point(1.0, 0.0)], Vec::new(), false);
    for threshold in [f64::NAN, f64::INFINITY, 0.0, -1.5] {
        assert!(matches!(
            fit_polylines(std::slice::from_ref(&shape), threshold),
            Err(FitError::InvalidThreshold(_))
        ));
    }
    assert!(fit_polylines(&[shape], 1.0).is_ok());
    let empty = polyline(Vec::new(), Vec::new(), false);
    assert!(matches!(
        fit_polylines(&[empty], 1.0),
        Err(FitError::DegeneratePolyline {
            index: 0,
            points: 0
        })
    ));
    let single = polyline(vec![point(2.0, 2.0)], Vec::new(), false);
    assert!(matches!(
        fit_polylines(&[single], 1.0),
        Err(FitError::DegeneratePolyline {
            index: 0,
            points: 1
        })
    ));
}

#[test]
fn fit_rejects_degenerate_polyline() {
    let polyline = SimplifiedPolyline {
        color_id: 0,
        points: Vec::new(),
        corner_indices: Vec::new(),
        is_closed: false,
    };
    let out = fit_polylines(&[polyline], 2.0);
    assert!(matches!(out, Err(FitError::DegeneratePolyline { .. })));
    assert!(fit_polylines(&[], 2.0).is_ok());
    assert!(fit_polylines(&[], 0.0).is_err());
}

/// The assembled `(H, b)` must reproduce the energy exactly along any step, which is
/// what lets the inner solve claim to minimise it.
#[test]
fn assembled_quadratic_matches_the_energy() {
    let samples: Vec<Point2D<f64>> = (0..=12)
        .map(|step| arc_point(10.0, step as f64 * std::f64::consts::FRAC_PI_2 / 12.0))
        .collect();
    let knots = [0, 4, 8, 12];
    let base: Vec<Point2D<f64>> = knots.iter().map(|&index| samples[index]).collect();
    let raw = [
        unit(sub(samples[1], samples[0])),
        unit(sub(samples[5], samples[3])),
        unit(sub(samples[9], samples[7])),
        unit(sub(samples[12], samples[11])),
    ];
    let dirs: Vec<Point2D<f64>> = raw
        .into_iter()
        .map(|d| d.unwrap_or(point(1.0, 0.0)))
        .collect();
    let taus = [2.0, 3.0, 3.0, 2.0];
    let mut y = vec![0.0; 8];
    for k in 0..4 {
        y[2 * k] = taus[k];
    }
    let assign: Vec<f64> = (0..samples.len()).map(|i| i as f64 * 0.25).collect();
    let model = Model {
        base,
        drift: dirs.clone(),
        dirs,
        y,
        lo: vec![0.2, -1.0, 0.2, -1.0, 0.2, -1.0, 0.2, -1.0],
        hi: vec![9.0, 1.0, 9.0, 1.0, 9.0, 1.0, 9.0, 1.0],
        samples,
        assign,
    };
    let (matrix, rhs) = assemble(&model);
    let baseline = total(&model);
    let delta = [0.3, 0.2, -0.5, -0.2, 0.4, 0.25, -0.25, 0.1];
    let shifted: Vec<f64> = model
        .y
        .iter()
        .zip(delta.iter())
        .map(|(y, d)| y + d)
        .collect();
    let mut moved = model.clone();
    moved.set_y(shifted);
    let actual = total(&moved) - baseline;
    let linear: f64 = delta.iter().zip(rhs.iter()).map(|(d, b)| d * b).sum();
    let mut quadratic = 0.0;
    for i in 0..8 {
        for j in 0..8 {
            quadratic += delta[i] * matrix.get(i, j) * delta[j];
        }
    }
    let predicted = -linear + 0.5 * quadratic;
    let scale = actual.abs().max(predicted.abs()).max(1.0);
    assert!(
        (actual - predicted).abs() <= 1e-9 * scale,
        "actual {actual} predicted {predicted}"
    );
}

#[test]
fn fitted_path_inherits_polyline_color_id() {
    let ring: Vec<Point2D<f64>> = (0..8)
        .map(|step| arc_point(10.0, step as f64 * std::f64::consts::TAU / 8.0))
        .collect();
    let mut shape = polyline(ring, Vec::new(), true);
    shape.color_id = 5;
    let paths = fit_polylines(&[shape], 1.0);
    assert!(
        matches!(paths, Ok(ref paths) if paths.len() == 1 && paths[0].color_id == 5),
        "color_id must survive the fitting stage"
    );
}
