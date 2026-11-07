//! Banded symmetric positive-definite Cholesky factorisation and substitution.
//!
//! Storage is the compact lower band: `data[i * (w + 1) + d]` holds `A[i][i - d]`.
//! Factorisation is `O(n * w^2)`, substitution `O(n * w)`, and both operate in place
//! on the pre-allocated band, so a solve performs no heap allocation.
//!
//! Half-bandwidth ceiling is 5 rather than the 4 quoted for the inner program: the
//! corrected G2 stencil couples knots `k` and `k + 2`, whose four variables occupy six
//! consecutive slots, so the `(tau_k, s_{k+2})` cross product needs `|i - j| = 5`. A
//! 4-band store would silently drop that term and minimise a different energy.

use crate::error::FitError;

const MAX_WIDTH: usize = 5;

pub struct Banded {
    order: usize,
    width: usize,
    data: Vec<f64>,
}

impl Banded {
    pub fn new(order: usize, width: usize) -> Self {
        let width = width.min(MAX_WIDTH).min(order.saturating_sub(1));
        Self {
            order,
            width,
            data: vec![0.0; order * (width + 1)],
        }
    }

    pub fn order(&self) -> usize {
        self.order
    }

    fn slot(&self, row: usize, col: usize) -> Option<usize> {
        let high = row.max(col);
        let low = row.min(col);
        if high >= self.order || high - low > self.width {
            return None;
        }
        Some(high * (self.width + 1) + (high - low))
    }

    /// Accumulate a symmetric entry; pairs outside the band are rejected silently.
    pub fn add(&mut self, row: usize, col: usize, value: f64) {
        if let Some(index) = self.slot(row, col) {
            self.data[index] += value;
        }
    }

    pub fn get(&self, row: usize, col: usize) -> f64 {
        self.slot(row, col).map_or(0.0, |index| self.data[index])
    }

    /// Add `eps * max(1, max diagonal)` to every diagonal entry.
    pub fn add_ridge(&mut self, eps: f64) {
        let stride = self.width + 1;
        let peak = self
            .data
            .iter()
            .step_by(stride)
            .fold(0.0_f64, |a, &v| a.max(v));
        let delta = eps * peak.max(1.0);
        for slot in self.data.iter_mut().step_by(stride) {
            *slot += delta;
        }
    }

    pub fn multiply(&self, vector: &[f64]) -> Vec<f64> {
        let stride = self.width + 1;
        (0..self.order)
            .map(|i| {
                let start = i.saturating_sub(self.width);
                let stop = (i + self.width + 1).min(self.order);
                (start..stop)
                    .map(|j| self.data[j.max(i) * stride + j.abs_diff(i)])
                    .zip(&vector[start..stop])
                    .map(|(value, x)| value * x)
                    .sum()
            })
            .collect()
    }

    /// In-place `A = L L^top`; the strict upper band is left untouched and unused.
    pub fn factor(&mut self) -> Result<(), FitError> {
        let n = self.order;
        let w = self.width;
        let stride = w + 1;
        for j in 0..n {
            let mut diagonal = self.data[j * stride];
            for k in j.saturating_sub(w)..j {
                let value = self.data[j * stride + (j - k)];
                diagonal -= value * value;
            }
            if !diagonal.is_finite() || diagonal <= 0.0 {
                return Err(FitError::Internal(String::from(
                    "banded factorisation lost positive definiteness",
                )));
            }
            let root = diagonal.sqrt();
            self.data[j * stride] = root;
            let stop = (j + w + 1).min(n);
            for i in (j + 1)..stop {
                let mut sum = self.data[i * stride + (i - j)];
                for k in i.saturating_sub(w)..j {
                    sum -= self.data[i * stride + (i - k)] * self.data[j * stride + (j - k)];
                }
                self.data[i * stride + (i - j)] = sum / root;
            }
        }
        Ok(())
    }

    /// Factor in place and solve `A x = rhs` into a caller-supplied buffer. Neither
    /// leg allocates: the factorisation overwrites the band and the elimination runs
    /// in place on the solution slice.
    pub fn solve_into(&mut self, rhs: &[f64], out: &mut [f64]) -> Result<(), FitError> {
        if rhs.len() != self.order || out.len() != self.order {
            return Err(FitError::Internal(String::from(
                "banded solve received a mismatched right-hand side",
            )));
        }
        self.factor()?;
        let n = self.order;
        let w = self.width;
        let stride = w + 1;
        for (i, &value) in rhs.iter().enumerate() {
            let (head, tail) = out.split_at_mut(i);
            let mut sum = value;
            for (k, &prior) in head.iter().enumerate().skip(i.saturating_sub(w)) {
                sum -= self.data[i * stride + (i - k)] * prior;
            }
            tail[0] = sum / self.data[i * stride];
        }
        for i in (0..n).rev() {
            let stop = (i + w + 1).min(n);
            let (head, tail) = out.split_at_mut(i + 1);
            let mut sum = head[i];
            for (k, &value) in tail.iter().take(stop - i - 1).enumerate() {
                sum -= self.data[(i + 1 + k) * stride + (k + 1)] * value;
            }
            head[i] = sum / self.data[i * stride];
        }
        Ok(())
    }

    /// Convenience wrapper: copies the band and allocates the solution.
    pub fn solve(&self, rhs: &[f64]) -> Result<Vec<f64>, FitError> {
        let mut factor = Self {
            order: self.order,
            width: self.width,
            data: self.data.clone(),
        };
        let mut out = vec![0.0; self.order];
        factor.solve_into(rhs, &mut out)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::Banded;

    #[test]
    fn tridiagonal_solve_matches_dense_reference() {
        // A = tridiag(-1, 2, -1) of order 4 is SPD; rhs chosen for a known x.
        let mut matrix = Banded::new(4, 2);
        for i in 0..4 {
            matrix.add(i, i, 2.0);
        }
        for i in 0..3 {
            matrix.add(i, i + 1, -1.0);
        }
        let expected = [1.0, -2.0, 3.0, 0.5];
        let rhs = matrix.multiply(&expected);
        let Ok(solution) = matrix.solve(&rhs) else {
            panic!("expected a positive definite band to factor");
        };
        for (value, reference) in solution.iter().zip(expected.iter()) {
            assert!((value - reference).abs() < 1e-12);
        }
    }

    #[test]
    fn indefinite_band_is_rejected() {
        let mut matrix = Banded::new(3, 1);
        matrix.add(0, 0, 0.0);
        matrix.add(1, 1, -1.0);
        matrix.add(2, 2, 1.0);
        assert!(matrix.factor().is_err());
    }
}
