//! Bounded-variable primal-dual active set solver for the inner quadratic program.
//!
//! `min 1/2 d^T H d - b^T d` subject to `lo <= d <= hi`, where the free block is
//! factorised with the banded Cholesky solver and the active block is pinned at its
//! bound. The active set is updated by Lagrange multiplier signs and terminates once
//! it stops changing (typically within five sweeps).

use crate::banded::Banded;
use crate::energy::{self, Model, BANDWIDTH};
use crate::error::FitError;

const MAX_SWEEPS: usize = 12;
const MULTIPLIER_TOLERANCE: f64 = 1e-10;

/// Solve the inner program and return the absolute variable vector.
pub fn solve(model: &Model) -> Result<Vec<f64>, FitError> {
    let count = model.vars();
    let (matrix, rhs) = energy::assemble(model);
    let lo: Vec<f64> = (0..count).map(|i| model.lo[i] - model.y[i]).collect();
    let hi: Vec<f64> = (0..count).map(|i| model.hi[i] - model.y[i]).collect();

    let mut delta = vec![0.0; count];
    let mut fixed = vec![false; count];
    let mut side = vec![0_i8; count];
    for i in 0..count {
        if (hi[i] - lo[i]).abs() <= f64::EPSILON {
            fixed[i] = true;
            side[i] = -1;
            delta[i] = lo[i];
        }
    }

    let mut sweep = 0;
    while sweep < MAX_SWEEPS {
        sweep += 1;
        let free: Vec<usize> = (0..count).filter(|&i| !fixed[i]).collect();
        if free.is_empty() {
            break;
        }
        let (mut reduced, block) = reduce(&matrix, &rhs, &free, &delta, &fixed);
        let mut step = vec![0.0; free.len()];
        reduced.solve_into(&block, &mut step)?;
        for (slot, &index) in free.iter().enumerate() {
            delta[index] = step[slot];
        }
        let mut clamped = false;
        for &index in &free {
            if delta[index] < lo[index] {
                delta[index] = lo[index];
                fixed[index] = true;
                side[index] = -1;
                clamped = true;
            } else if delta[index] > hi[index] {
                delta[index] = hi[index];
                fixed[index] = true;
                side[index] = 1;
                clamped = true;
            }
        }
        if clamped {
            continue;
        }
        let gradient = matrix.multiply(&delta);
        let mut released = false;
        for i in 0..count {
            if !fixed[i] || (hi[i] - lo[i]).abs() <= f64::EPSILON {
                continue;
            }
            let slope = gradient[i] - rhs[i];
            if (side[i] < 0 && slope < -MULTIPLIER_TOLERANCE)
                || (side[i] > 0 && slope > MULTIPLIER_TOLERANCE)
            {
                fixed[i] = false;
                released = true;
            }
        }
        if !released {
            return Ok(absolute(model, &delta));
        }
    }
    Ok(absolute(model, &delta))
}

fn absolute(model: &Model, delta: &[f64]) -> Vec<f64> {
    (0..delta.len())
        .map(|i| (model.y[i] + delta[i]).clamp(model.lo[i], model.hi[i]))
        .collect()
}

fn reduce(
    matrix: &Banded,
    rhs: &[f64],
    free: &[usize],
    delta: &[f64],
    fixed: &[bool],
) -> (Banded, Vec<f64>) {
    let mut map = vec![usize::MAX; matrix.order()];
    for (slot, &index) in free.iter().enumerate() {
        map[index] = slot;
    }
    let mut reduced = Banded::new(free.len(), BANDWIDTH);
    let mut block = Vec::with_capacity(free.len());
    for (slot, &index) in free.iter().enumerate() {
        let mut acc = rhs[index];
        let start = index.saturating_sub(BANDWIDTH);
        let stop = (index + BANDWIDTH + 1).min(matrix.order());
        for column in start..stop {
            let value = matrix.get(index, column);
            if fixed[column] {
                acc -= value * delta[column];
            } else if column <= index {
                reduced.add(slot, map[column], value);
            }
        }
        block.push(acc);
    }
    reduced.add_ridge(1e-12);
    (reduced, block)
}
