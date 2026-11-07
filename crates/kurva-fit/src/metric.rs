//! Energy value of the current configuration, used by the angular line search.

use crate::energy::{
    curvature_residual, span_local, weights, Model, LAMBDA_ANCHOR, LAMBDA_CURV, LAMBDA_FAIR,
};
use crate::vec2::{add, dot, mul, sub};

/// Full penalty of the configuration: data residual, strain, corrected G2, anchor.
pub fn total(model: &Model) -> f64 {
    let spans = model.spans();
    let mut energy = 0.0;
    for (index, &sample) in model.samples.iter().enumerate() {
        let (k, t) = span_local(model.assign[index], spans);
        let (a, b, c, d) = weights(t);
        let value = add(
            add(mul(model.knot(k), a), mul(model.knot(k + 1), b)),
            sub(
                mul(model.dirs[k], c * model.tau(k)),
                mul(model.dirs[k + 1], d * model.tau(k + 1)),
            ),
        );
        let residual = sub(value, sample);
        energy += dot(residual, residual);
    }
    for k in 0..spans {
        let chord = sub(model.knot(k + 1), model.knot(k));
        let d0 = model.dirs[k];
        let d1 = model.dirs[k + 1];
        let u0 = sub(
            sub(chord, mul(d0, 2.0 * model.tau(k))),
            mul(d1, model.tau(k + 1)),
        );
        let v0 = add(
            sub(mul(d0, model.tau(k)), chord),
            mul(d1, 2.0 * model.tau(k + 1)),
        );
        energy += 12.0 * LAMBDA_FAIR * (dot(u0, u0) + dot(u0, v0) + dot(v0, v0));
    }
    if spans >= 2 {
        for k in 0..spans - 1 {
            let (residual, _, _) = curvature_residual(model, k);
            energy += LAMBDA_CURV * residual * residual;
        }
    }
    for k in 1..model.knots_len().saturating_sub(1) {
        let drift = model.y[2 * k + 1];
        energy += LAMBDA_ANCHOR * drift * drift;
    }
    energy
}
