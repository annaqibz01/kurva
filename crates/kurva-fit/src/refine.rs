//! Sub-arc refinement: bi-level optimisation plus curvature-weighted span splitting.

use crate::angular;
use crate::curve::reverse_segment;
use crate::energy::{self, Model};
use crate::error::FitError;
use crate::measure::{evaluate, split_index, Fitted};
use crate::metric;
use crate::parameter::{centripetal, project_sweep};
use crate::pdas;
use crate::vec2::{add, mul, norm, sub, unit};
use kurva_types::{BezierSegment, Point2D};

const ITERATIONS: usize = 32;
const MAX_SPANS: usize = 64;
const GAMMA: f64 = 0.30;

/// Fit one sub-arc, splitting the span count until the deviation is within `eps`.
///
/// A canonical traversal (lexicographic on the endpoint pair) makes the fit exactly
/// reversal-equivariant: mirroring the input mirrors the output bit for bit, because
/// both orders collapse onto the same sample array before any arithmetic runs.
pub fn fit_arc(
    points: &[Point2D<f64>],
    eps: f64,
    periodic: bool,
) -> Result<Vec<BezierSegment>, FitError> {
    let mut samples = dedupe(points);
    if samples.len() < 2 {
        return Ok(Vec::new());
    }
    let flip = lexicographic(&samples[samples.len() - 1], &samples[0]);
    if flip {
        samples.reverse();
    }
    let mut knots = if periodic && samples.len() >= 9 {
        let last = samples.len() - 1;
        vec![0, last / 4, last / 2, 3 * last / 4, last]
    } else {
        vec![0, samples.len() - 1]
    };
    let mut best: Option<Fitted> = None;
    for _ in 0..MAX_SPANS {
        let fitted = fit_spans(&samples, &knots, periodic)?;
        let reached = fitted.error <= eps;
        let candidate = split_index(&samples, &fitted.chain, &knots);
        if best.as_ref().is_none_or(|held| fitted.error < held.error) {
            best = Some(fitted);
        }
        if reached {
            break;
        }
        let Some(index) = candidate else {
            break;
        };
        knots.push(index);
        knots.sort_unstable();
    }
    let Some(fitted) = best else {
        return Err(FitError::Internal(String::from(
            "no span configuration was fitted",
        )));
    };
    let mut segments = fitted.chain.segments();
    if flip {
        segments.reverse();
        segments = segments.into_iter().map(reverse_segment).collect();
    }
    Ok(segments)
}

fn fit_spans(
    samples: &[Point2D<f64>],
    knots: &[usize],
    periodic: bool,
) -> Result<Fitted, FitError> {
    let mut model = initial_model(samples, knots)?;
    if periodic {
        model.unify_seam();
    }
    let mut best = evaluate(&model, knots);
    let mut previous = f64::INFINITY;
    for _ in 0..ITERATIONS {
        model.assign = project_sweep(&model.chain(), samples, &model.assign);
        let y = pdas::solve(&model)?;
        model.set_y(y);
        angular::refine(&mut model)?;
        if periodic {
            model.unify_seam();
        }
        let fitted = evaluate(&model, knots);
        let iter_error = fitted.error;
        if iter_error < best.error {
            best = fitted;
        }
        let energy = metric::total(&model);
        if (previous - energy).abs() <= 1e-12 * energy.abs().max(1.0) {
            break;
        }
        previous = energy;
    }
    Ok(best)
}

fn initial_model(samples: &[Point2D<f64>], knots: &[usize]) -> Result<Model, FitError> {
    let count = knots.len();
    if count < 2 {
        return Err(FitError::Internal(String::from(
            "a fitted arc needs at least two knots",
        )));
    }
    let base: Vec<Point2D<f64>> = knots.iter().map(|&index| samples[index]).collect();
    let params = centripetal(samples);
    let mut assign = vec![0.0; samples.len()];
    for (span, pair) in knots.windows(2).enumerate() {
        let (start, end) = (pair[0], pair[1]);
        let span_param = (params[end] - params[start]).max(1e-12);
        for index in start..=end {
            let fraction = ((params[index] - params[start]) / span_param).clamp(0.0, 1.0);
            assign[index] = span as f64 + fraction;
        }
    }
    let mut drift = Vec::with_capacity(count);
    for k in 0..count {
        let fallback = Point2D { x: 1.0, y: 0.0 };
        let direction = if k == 0 {
            tangent_at(samples, 0, true).or_else(|| unit(sub(samples[knots[1]], samples[0])))
        } else if k + 1 == count {
            tangent_at(samples, samples.len() - 1, false)
                .or_else(|| unit(sub(samples[samples.len() - 1], samples[knots[count - 2]])))
        } else {
            unit(sub(samples[knots[k + 1]], samples[knots[k - 1]]))
        };
        drift.push(direction.unwrap_or(fallback));
    }
    let mut y = vec![0.0; 2 * count];
    let mut lo = vec![0.0; 2 * count];
    let mut hi = vec![0.0; 2 * count];
    for k in 0..count {
        let span = k.min(count - 2);
        let chord = norm(sub(base[span + 1], base[span]));
        let previous = if k >= 1 {
            norm(sub(base[k], base[k - 1]))
        } else {
            chord
        };
        let feature = previous.min(chord).max(1e-9);
        let tau = energy::tau_var(k);
        y[tau] = chord / 3.0;
        lo[tau] = 0.10 * chord;
        hi[tau] = 3.0 * chord;
        let shift = energy::drift_var(k);
        if k == 0 || k + 1 == count {
            lo[shift] = 0.0;
            hi[shift] = 0.0;
        } else {
            let bound = GAMMA * feature;
            lo[shift] = -bound;
            hi[shift] = bound;
        }
    }
    let dirs = drift.clone();
    Ok(Model {
        base,
        drift,
        dirs,
        y,
        lo,
        hi,
        samples: samples.to_vec(),
        assign,
    })
}

/// Second-order one-sided tangent from three consecutive samples: `O(h^2)` error,
/// against `O(h)` for the raw chord, which matters because the end directions of an
/// open arc are fixed rather than optimised.
fn tangent_at(samples: &[Point2D<f64>], index: usize, forward: bool) -> Option<Point2D<f64>> {
    if samples.len() < 3 || index >= samples.len() {
        return None;
    }
    let vector = if forward {
        let ahead = samples.get(index + 2)?;
        sub(
            mul(samples[index + 1], 4.0),
            add(mul(samples[index], 3.0), *ahead),
        )
    } else {
        if index < 2 {
            return None;
        }
        let near = mul(samples[index - 1], 4.0);
        sub(mul(samples[index], 3.0), sub(near, samples[index - 2]))
    };
    unit(vector)
}

fn dedupe(points: &[Point2D<f64>]) -> Vec<Point2D<f64>> {
    let mut unique: Vec<Point2D<f64>> = Vec::with_capacity(points.len());
    for &point in points {
        let duplicate = unique.last().is_some_and(|held| {
            (held.x - point.x).abs() <= 1e-12 && (held.y - point.y).abs() <= 1e-12
        });
        if !duplicate {
            unique.push(point);
        }
    }
    unique
}

fn lexicographic(left: &Point2D<f64>, right: &Point2D<f64>) -> bool {
    if (left.x - right.x).abs() > 1e-12 {
        return left.x < right.x;
    }
    left.y < right.y
}
