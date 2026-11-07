//! Unified quadratic energy: data fidelity, bilinear strain, corrected G2 and anchor.
//!
//! Variables are interleaved as `[tau_0, s_0, tau_1, s_1, ...]` where `tau_k` is the
//! shared tangent magnitude at knot `k` and `s_k` is the drift of knot `k` along the
//! unit chord direction `e_k`, so `Q_k = base_k + s_k e_k`.

use crate::banded::Banded;
use crate::curve::Chain;
use crate::vec2::{add, dot, mul, perp, sub, unit};
use kurva_types::Point2D;

pub const LAMBDA_FAIR: f64 = 1e-4;
pub const LAMBDA_CURV: f64 = 1e-3;
pub const LAMBDA_ANCHOR: f64 = 1e-2;
pub const BANDWIDTH: usize = 5;

pub fn tau_var(knot: usize) -> usize {
    2 * knot
}

pub fn drift_var(knot: usize) -> usize {
    2 * knot + 1
}

/// Split a global parameter into its span index and local parameter.
pub fn span_local(u: f64, spans: usize) -> (usize, f64) {
    if spans == 0 {
        return (0, 0.0);
    }
    let clamped = u.clamp(0.0, spans as f64);
    let k = (clamped.floor() as usize).min(spans - 1);
    (k, clamped - k as f64)
}

/// Bernstein weights of the knot and tangent contributions of one span.
pub fn weights(t: f64) -> (f64, f64, f64, f64) {
    let u = 1.0 - t;
    (
        u * u * (1.0 + 2.0 * t),
        t * t * (3.0 - 2.0 * t),
        3.0 * u * u * t,
        3.0 * u * t * t,
    )
}

#[derive(Clone)]
pub struct Model {
    pub base: Vec<Point2D<f64>>,
    pub drift: Vec<Point2D<f64>>,
    pub dirs: Vec<Point2D<f64>>,
    pub y: Vec<f64>,
    pub lo: Vec<f64>,
    pub hi: Vec<f64>,
    pub samples: Vec<Point2D<f64>>,
    pub assign: Vec<f64>,
}

impl Model {
    pub fn knots_len(&self) -> usize {
        self.base.len()
    }

    pub fn spans(&self) -> usize {
        self.base.len().saturating_sub(1)
    }

    pub fn vars(&self) -> usize {
        self.y.len()
    }

    pub fn tau(&self, knot: usize) -> f64 {
        self.y[tau_var(knot)]
    }

    pub fn knot(&self, knot: usize) -> Point2D<f64> {
        add(
            self.base[knot],
            mul(self.drift[knot], self.y[drift_var(knot)]),
        )
    }

    pub fn chain(&self) -> Chain {
        let taus = (0..self.knots_len()).map(|k| self.tau(k)).collect();
        let knots = (0..self.knots_len()).map(|k| self.knot(k)).collect();
        Chain {
            knots,
            dirs: self.dirs.clone(),
            taus,
        }
    }

    pub fn set_y(&mut self, y: Vec<f64>) {
        for (slot, value) in self.y.iter_mut().zip(y) {
            *slot = value;
        }
    }

    /// Periodic seam: the wrap tangent is a single direction, so both endpoint
    /// directions collapse onto their bisector.
    pub fn unify_seam(&mut self) {
        let last = self.dirs.len().saturating_sub(1);
        if last == 0 {
            return;
        }
        if let Some(shared) = unit(add(self.dirs[0], self.dirs[last])) {
            self.dirs[0] = shared;
            self.dirs[last] = shared;
        }
    }
}

/// Assemble `1/2 d^T H d - b^T d` for the inner program, expressed in the delta of
/// the current configuration.
pub fn assemble(model: &Model) -> (Banded, Vec<f64>) {
    let mut matrix = Banded::new(model.vars(), BANDWIDTH);
    let mut rhs = vec![0.0; model.vars()];
    data_term(model, &mut matrix, &mut rhs);
    bending_term(model, &mut matrix, &mut rhs);
    g2_term(model, &mut matrix, &mut rhs);
    anchor_term(model, &mut matrix);
    (matrix, rhs)
}

fn data_term(model: &Model, matrix: &mut Banded, rhs: &mut [f64]) {
    for (index, &sample) in model.samples.iter().enumerate() {
        let (k, t) = span_local(model.assign[index], model.spans());
        let (a, b, c, d) = weights(t);
        let value = add(
            add(mul(model.knot(k), a), mul(model.knot(k + 1), b)),
            sub(
                mul(model.dirs[k], c * model.tau(k)),
                mul(model.dirs[k + 1], d * model.tau(k + 1)),
            ),
        );
        let residual = sub(value, sample);
        let jacobian = [
            mul(model.drift[k], a),
            mul(model.drift[k + 1], b),
            mul(model.dirs[k], c),
            mul(model.dirs[k + 1], -d),
        ];
        let indices = [drift_var(k), drift_var(k + 1), tau_var(k), tau_var(k + 1)];
        for (p, &row) in indices.iter().enumerate() {
            rhs[row] -= 2.0 * dot(residual, jacobian[p]);
            for (q, &col) in indices.iter().enumerate().skip(p) {
                matrix.add(row, col, 2.0 * dot(jacobian[p], jacobian[q]));
            }
        }
    }
}

fn bending_term(model: &Model, matrix: &mut Banded, rhs: &mut [f64]) {
    let weight = 12.0 * LAMBDA_FAIR;
    for k in 0..model.spans() {
        let chord = sub(model.knot(k + 1), model.knot(k));
        let d0 = model.dirs[k];
        let d1 = model.dirs[k + 1];
        let t1 = model.tau(k + 1);
        let u0 = sub(sub(chord, mul(d0, 2.0 * model.tau(k))), mul(d1, t1));
        let v0 = add(sub(mul(d0, model.tau(k)), chord), mul(d1, 2.0 * t1));
        let gu = [
            mul(model.drift[k], -1.0),
            model.drift[k + 1],
            mul(d0, -2.0),
            mul(d1, -1.0),
        ];
        let gv = [
            model.drift[k],
            mul(model.drift[k + 1], -1.0),
            d0,
            mul(d1, 2.0),
        ];
        let indices = [drift_var(k), drift_var(k + 1), tau_var(k), tau_var(k + 1)];
        for (p, &row) in indices.iter().enumerate() {
            let linear =
                2.0 * dot(u0, gu[p]) + dot(gu[p], v0) + dot(u0, gv[p]) + 2.0 * dot(v0, gv[p]);
            rhs[row] -= weight * linear;
            for (q, &col) in indices.iter().enumerate().skip(p) {
                let curvature = 2.0 * dot(gu[p], gu[q])
                    + dot(gu[q], gv[p])
                    + dot(gu[p], gv[q])
                    + 2.0 * dot(gv[p], gv[q]);
                matrix.add(row, col, weight * curvature);
            }
        }
    }
}

pub fn curvature_residual(model: &Model, k: usize) -> (f64, [usize; 4], [f64; 4]) {
    let normal = perp(model.dirs[k + 1]);
    let base = dot(normal, sub(model.knot(k), model.knot(k + 2)))
        + dot(normal, model.dirs[k]) * model.tau(k)
        + dot(normal, model.dirs[k + 2]) * model.tau(k + 2);
    let indices = [drift_var(k), drift_var(k + 2), tau_var(k), tau_var(k + 2)];
    let coefficients = [
        dot(normal, model.drift[k]),
        -dot(normal, model.drift[k + 2]),
        dot(normal, model.dirs[k]),
        dot(normal, model.dirs[k + 2]),
    ];
    (base, indices, coefficients)
}

fn g2_term(model: &Model, matrix: &mut Banded, rhs: &mut [f64]) {
    if model.spans() < 2 {
        return;
    }
    for k in 0..model.spans().saturating_sub(1) {
        let (base, indices, coefficients) = curvature_residual(model, k);
        for (p, &row) in indices.iter().enumerate() {
            rhs[row] -= 2.0 * LAMBDA_CURV * base * coefficients[p];
            for (q, &col) in indices.iter().enumerate().skip(p) {
                matrix.add(
                    row,
                    col,
                    2.0 * LAMBDA_CURV * coefficients[p] * coefficients[q],
                );
            }
        }
    }
}

fn anchor_term(model: &Model, matrix: &mut Banded) {
    for k in 1..model.knots_len().saturating_sub(1) {
        let variable = drift_var(k);
        matrix.add(variable, variable, 2.0 * LAMBDA_ANCHOR);
    }
}
