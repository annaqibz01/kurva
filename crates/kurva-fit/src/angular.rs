//! Outer angular trust-region refinement of the interior tangent directions.
//!
//! The envelope theorem makes the gradient of the minimised energy equal to its
//! explicit partial derivative with respect to the angle, so the angular step only
//! needs the data residual and the analytic control-point sensitivities
//! `dB_k/dtheta_k = 3 tau_k t (1-t)^2 n_k` and
//! `dB_k/dtheta_{k+1} = -3 tau_{k+1} t^2 (1-t) n_{k+1}`.

use crate::banded::Banded;
use crate::energy::{span_local, Model};
use crate::error::FitError;
use crate::metric;
use crate::vec2::{dot, mul, perp, sub};
use kurva_types::Point2D;

const MAX_STEP: f64 = std::f64::consts::FRAC_PI_6;
const TRIALS: usize = 4;

pub fn refine(model: &mut Model) -> Result<(), FitError> {
    let spans = model.spans();
    if spans < 2 {
        return Ok(());
    }
    let unknowns = spans - 1;
    let mut matrix = Banded::new(unknowns, 1);
    let mut gradient = vec![0.0; unknowns];
    let chain = model.chain();
    for (index, &sample) in model.samples.iter().enumerate() {
        let (k, t) = span_local(model.assign[index], spans);
        let Some(posed) = chain.evaluate(k, t) else {
            continue;
        };
        let residual = sub(posed.value, sample);
        let u = 1.0 - t;
        let near = mul(perp(model.dirs[k]), 3.0 * model.tau(k) * t * u * u);
        let far = mul(perp(model.dirs[k + 1]), -3.0 * model.tau(k + 1) * t * t * u);
        let mut rows: [Option<(usize, Point2D<f64>)>; 2] = [None, None];
        if k >= 1 {
            rows[0] = Some((k - 1, near));
        }
        if k + 1 < spans {
            rows[1] = Some((k, far));
        }
        for (p, entry) in rows.iter().enumerate() {
            let Some((row, left)) = *entry else {
                continue;
            };
            gradient[row] += 2.0 * dot(residual, left);
            for entry_right in rows.iter().skip(p) {
                let Some((column, right)) = *entry_right else {
                    continue;
                };
                matrix.add(row, column, 2.0 * dot(left, right));
            }
        }
    }
    matrix.add_ridge(1e-9);
    let step: Vec<f64> = gradient.iter().map(|value| -value).collect();
    let Ok(mut proposal) = matrix.solve(&step) else {
        return Ok(());
    };
    for value in proposal.iter_mut() {
        *value = value.clamp(-MAX_STEP, MAX_STEP);
    }
    let baseline = metric::total(model);
    let mut scale = 1.0;
    for _ in 0..TRIALS {
        let mut candidate = model.clone();
        candidate.turn_dirs(&proposal, scale);
        if metric::total(&candidate) <= baseline {
            *model = candidate;
            return Ok(());
        }
        scale *= 0.5;
    }
    Ok(())
}

impl Model {
    /// Rotate the interior directions by `scale * step`, keeping the unit norm.
    fn turn_dirs(&mut self, step: &[f64], scale: f64) {
        for (knot, &turn) in step.iter().enumerate() {
            let angle = turn * scale;
            if angle.abs() <= f64::EPSILON {
                continue;
            }
            let current = self.dirs[knot + 1];
            let cosine = angle.cos();
            let sine = angle.sin();
            self.dirs[knot + 1] = kurva_types::Point2D {
                x: current.x * cosine - current.y * sine,
                y: current.x * sine + current.y * cosine,
            };
        }
    }
}
