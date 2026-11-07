//! Deviation measurement and curvature-weighted split-point selection.

use crate::curve::Chain;
use crate::energy::Model;
use crate::parameter::project;
use crate::vec2::{cross, dot, norm, sub};
use kurva_types::Point2D;

pub struct Fitted {
    pub chain: Chain,
    pub error: f64,
}

/// Two-sided deviation, the quantity the split loop drives below the threshold:
///
/// * fidelity `E_max = max_i |B(t_i) - q_i|` - every sample projected onto the curve;
/// * bulge - interior span probes measured against the sample polyline, which catches
///   a span that bows away between two samples while still passing through both.
pub fn evaluate(model: &Model, knots: &[usize]) -> Fitted {
    let chain = model.chain();
    let spans = chain.spans();
    let mut error: f64 = 0.0;
    for (index, &sample) in model.samples.iter().enumerate() {
        let foot = project(&chain, sample, model.assign[index], 0.0, spans as f64);
        error = error.max(foot.distance);
    }
    for (span, pair) in knots.windows(2).enumerate() {
        let (start, end) = (pair[0], pair[1]);
        if end <= start {
            continue;
        }
        let edges = &model.samples[start..=end];
        for step in 1..16 {
            let Some(posed) = chain.evaluate(span, step as f64 / 16.0) else {
                continue;
            };
            error = error.max(distance_to_polyline(edges, posed.value));
        }
    }
    Fitted { chain, error }
}

/// Curvature-weighted split point: `t* = argmax |B(t) - q(t)|^2 (1 + kappa^2)`,
/// restricted to samples strictly interior to a span so a split never produces a span
/// without data.
pub fn split_index(samples: &[Point2D<f64>], chain: &Chain, knots: &[usize]) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;
    for (span, pair) in knots.windows(2).enumerate() {
        let (start, end) = (pair[0], pair[1]);
        if end <= start + 1 {
            continue;
        }
        let width = (end - start) as f64;
        for index in (start + 1)..end {
            let fraction = (index - start) as f64 / width;
            let hint = span as f64 + fraction;
            let foot = project(chain, samples[index], hint, span as f64, span as f64 + 1.0);
            let kappa = curvature_at(samples, index);
            // The balance weight keeps the split near mid-span: tangent magnitude is
            // shared across a knot, so neighbouring spans of very different length
            // cannot both be fitted well by one tau.
            let balance = (std::f64::consts::PI * fraction).sin().max(1e-9);
            let score = foot.distance * foot.distance * (1.0 + kappa * kappa) * balance;
            if best.is_none_or(|(held, _)| score > held) {
                best = Some((score, index));
            }
        }
    }
    best.map(|(_, index)| index)
}

fn curvature_at(samples: &[Point2D<f64>], index: usize) -> f64 {
    let incoming = sub(samples[index], samples[index - 1]);
    let outgoing = sub(samples[index + 1], samples[index]);
    let first = norm(incoming);
    let second = norm(outgoing);
    if first <= f64::EPSILON || second <= f64::EPSILON {
        return 0.0;
    }
    let sine = cross(incoming, outgoing) / (first * second);
    sine.abs() / (0.5 * (first + second))
}

fn distance_to_polyline(samples: &[Point2D<f64>], probe: Point2D<f64>) -> f64 {
    samples
        .windows(2)
        .map(|edge| distance_to_edge(probe, edge[0], edge[1]))
        .fold(f64::INFINITY, f64::min)
}

fn distance_to_edge(probe: Point2D<f64>, start: Point2D<f64>, end: Point2D<f64>) -> f64 {
    let span = sub(end, start);
    let length = dot(span, span);
    if length <= f64::EPSILON {
        return norm(sub(probe, start));
    }
    let ratio = (dot(sub(probe, start), span) / length).clamp(0.0, 1.0);
    let foot = Point2D {
        x: start.x + span.x * ratio,
        y: start.y + span.y * ratio,
    };
    norm(sub(probe, foot))
}
