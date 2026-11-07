//! Centripetal parameterisation and safeguarded orthogonal footpoint inversion.

use crate::curve::Chain;
use crate::vec2::{dot, norm, sub};
use kurva_types::Point2D;

#[derive(Clone, Copy, Debug)]
pub struct Footpoint {
    pub u: f64,
    pub distance: f64,
}

const MAX_STEPS: usize = 16;
const EPSILON: f64 = 1e-14;

/// Theorem 1: `du_i = |q_i - q_{i-1}|^(1/2)`, normalised onto `[0, 1]`.
///
/// The square-root exponent keeps the parameter speed from collapsing at a tight
/// turn, which is what produces artificial hodograph cusps under chordal spacing.
pub fn centripetal(points: &[Point2D<f64>]) -> Vec<f64> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut params = Vec::with_capacity(points.len());
    params.push(0.0);
    let mut total = 0.0;
    for pair in points.windows(2) {
        total += norm(sub(pair[1], pair[0])).sqrt();
        params.push(total);
    }
    if total <= f64::EPSILON {
        let last = (points.len() - 1).max(1) as f64;
        return (0..points.len()).map(|i| i as f64 / last).collect();
    }
    params.iter().map(|value| value / total).collect()
}

/// Lemma 1: Levenberg-Marquardt descent on `f(t) = |B(t) - q|^2` clipped to the
/// bracket spanned by the neighbouring samples.
pub fn project(chain: &Chain, target: Point2D<f64>, hint: f64, lo: f64, hi: f64) -> Footpoint {
    let lower = lo.min(hi);
    let upper = lo.max(hi);
    let mut u = hint.clamp(lower, upper);
    for _ in 0..MAX_STEPS {
        let Some(posed) = chain.evaluate_global(u) else {
            break;
        };
        let residual = sub(posed.value, target);
        let first = 2.0 * dot(posed.first, residual);
        let second = 2.0 * (dot(posed.first, posed.first) + dot(residual, posed.second));
        let damping = (-second).max(0.0) + 1e-4;
        let step = first / (second + damping);
        if !step.is_finite() {
            break;
        }
        let next = (u - step).clamp(lower, upper);
        let settled = (next - u).abs() <= EPSILON;
        u = next;
        if settled {
            break;
        }
    }
    let distance = chain
        .evaluate_global(u)
        .map_or(f64::INFINITY, |posed| norm(sub(posed.value, target)));
    Footpoint { u, distance }
}

/// Monotone projection sweep: sample `i` is bracketed by the already projected
/// predecessor and the current estimate of its successor.
pub fn project_sweep(chain: &Chain, samples: &[Point2D<f64>], hints: &[f64]) -> Vec<f64> {
    let spans = chain.spans() as f64;
    let mut projected = hints.to_vec();
    for i in 0..samples.len() {
        let lo = if i == 0 { 0.0 } else { projected[i - 1] };
        let hi = if i + 1 == samples.len() {
            spans
        } else {
            hints[i + 1]
        };
        let hi = hi.max(lo);
        projected[i] = project(chain, samples[i], hints[i], lo, hi).u;
    }
    projected
}

#[cfg(test)]
mod tests {
    use super::{centripetal, project};
    use crate::curve::Chain;
    use kurva_types::Point2D;

    fn point(x: f64, y: f64) -> Point2D<f64> {
        Point2D { x, y }
    }

    #[test]
    fn centripetal_is_monotone_and_normalised() {
        let samples = [point(0.0, 0.0), point(1.0, 0.0), point(4.0, 0.0)];
        let params = centripetal(&samples);
        assert!((params[0]).abs() < 1e-15);
        assert!((params[2] - 1.0).abs() < 1e-15);
        assert!(params[1] > params[0] && params[1] < params[2]);
        // Chord length 1 then 3 gives increments 1 and sqrt(3) before normalisation.
        let ratio = (params[1] - params[0]) / (params[2] - params[1]);
        assert!((ratio - 1.0 / 3.0_f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn footpoint_recovers_the_closest_parameter_on_a_straight_span() {
        let chain = Chain {
            knots: vec![point(0.0, 0.0), point(10.0, 0.0)],
            dirs: vec![point(1.0, 0.0), point(1.0, 0.0)],
            taus: vec![10.0 / 3.0, 10.0 / 3.0],
        };
        let foot = project(&chain, point(2.5, 2.0), 0.5, 0.0, 1.0);
        assert!((foot.u - 0.25).abs() < 1e-9);
        assert!((foot.distance - 2.0).abs() < 1e-9);
    }
}
