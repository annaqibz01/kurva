//! Test-only measurement helpers.

use crate::vec2::{dot, norm, sub};
use kurva_types::{BezierSegment, Point2D, SimplifiedPolyline};

pub fn point(x: f64, y: f64) -> Point2D<f64> {
    Point2D { x, y }
}

pub fn polyline(
    points: Vec<Point2D<f64>>,
    corners: Vec<usize>,
    closed: bool,
) -> SimplifiedPolyline {
    SimplifiedPolyline {
        points,
        corner_indices: corners,
        is_closed: closed,
    }
}

pub fn arc_point(radius: f64, turn: f64) -> Point2D<f64> {
    point(radius * turn.cos(), radius * turn.sin())
}

/// Independent footpoint search: coarse scan, then golden-section refinement inside
/// the bracketing sub-interval. Deliberately does not reuse the crate's solver, so a
/// broken projection cannot hide behind a matching test measurement.
pub fn distance_to_curve(segments: &[BezierSegment], sample: Point2D<f64>) -> f64 {
    segments
        .iter()
        .map(|segment| distance_to_segment(segment, sample))
        .fold(f64::INFINITY, f64::min)
}

fn distance_to_segment(segment: &BezierSegment, sample: Point2D<f64>) -> f64 {
    const DOF: usize = 32;
    let mut best = (f64::INFINITY, 0.0);
    for step in 0..=DOF {
        let t = step as f64 / DOF as f64;
        let distance = norm(sub(segment.sample(t), sample));
        if distance < best.0 {
            best = (distance, t);
        }
    }
    let mut low = (best.1 - 1.0 / DOF as f64).max(0.0);
    let mut high = (best.1 + 1.0 / DOF as f64).min(1.0);
    let golden = 0.618_033_988_749_895;
    let probe = |t: f64| norm(sub(segment.sample(t), sample));
    let mut left = high - golden * (high - low);
    let mut right = low + golden * (high - low);
    for _ in 0..64 {
        if probe(left) < probe(right) {
            high = right;
            right = left;
            left = high - golden * (high - low);
        } else {
            low = left;
            left = right;
            right = low + golden * (high - low);
        }
    }
    best.0.min(probe(0.5 * (low + high)))
}

/// `E_max = max_i |B(t_i) - q_i|`, the deviation the split loop controls.
pub fn max_sample_error(segments: &[BezierSegment], samples: &[Point2D<f64>]) -> f64 {
    samples
        .iter()
        .map(|&sample| distance_to_curve(segments, sample))
        .fold(0.0, f64::max)
}

/// Dense curve-to-polyline deviation: catches a span that bulges between samples.
pub fn curve_deviation(segments: &[BezierSegment], polyline: &[Point2D<f64>]) -> f64 {
    let mut worst: f64 = 0.0;
    for segment in segments {
        for step in 0..=64 {
            let probe = segment.sample(step as f64 / 64.0);
            let nearest = polyline
                .windows(2)
                .map(|edge| distance_to_edge(probe, edge[0], edge[1]))
                .fold(f64::INFINITY, f64::min);
            worst = worst.max(nearest);
        }
    }
    worst
}

fn distance_to_edge(probe: Point2D<f64>, start: Point2D<f64>, end: Point2D<f64>) -> f64 {
    let span = sub(end, start);
    let length = dot(span, span);
    if length <= f64::EPSILON {
        return norm(sub(probe, start));
    }
    let ratio = (dot(sub(probe, start), span) / length).clamp(0.0, 1.0);
    let foot = point(start.x + span.x * ratio, start.y + span.y * ratio);
    norm(sub(probe, foot))
}
