//! Hermite-reduced variational basis: shared knots and shared tangent magnitudes.
//!
//! Span `k` is `P0 = Q_k`, `P1 = Q_k + tau_k d_k`, `P2 = Q_{k+1} - tau_{k+1} d_{k+1}`,
//! `P3 = Q_{k+1}`. Because both adjacent spans read the same `tau_{k+1} d_{k+1}`, the
//! structural G1 invariant `P3^(k) - P2^(k) = P1^(k+1) - P0^(k+1)` holds identically.

use crate::vec2::{add, mul, sub};
use kurva_types::{BezierSegment, Point2D};

#[derive(Clone, Debug)]
pub struct Chain {
    pub knots: Vec<Point2D<f64>>,
    pub dirs: Vec<Point2D<f64>>,
    pub taus: Vec<f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct Posed {
    pub value: Point2D<f64>,
    pub first: Point2D<f64>,
    pub second: Point2D<f64>,
}

/// Bezier control polygon of one span.
pub type Controls = (Point2D<f64>, Point2D<f64>, Point2D<f64>, Point2D<f64>);

impl Chain {
    pub fn spans(&self) -> usize {
        self.knots.len().saturating_sub(1)
    }

    pub fn controls(&self, k: usize) -> Option<Controls> {
        if k + 1 >= self.knots.len() {
            return None;
        }
        let p0 = self.knots[k];
        let p3 = self.knots[k + 1];
        let p1 = add(p0, mul(self.dirs[k], self.taus[k]));
        let p2 = sub(p3, mul(self.dirs[k + 1], self.taus[k + 1]));
        Some((p0, p1, p2, p3))
    }

    pub fn segment(&self, k: usize) -> Option<BezierSegment> {
        self.controls(k)
            .map(|(p0, p1, p2, p3)| BezierSegment { p0, p1, p2, p3 })
    }

    pub fn segments(&self) -> Vec<BezierSegment> {
        (0..self.spans()).filter_map(|k| self.segment(k)).collect()
    }

    pub fn evaluate(&self, k: usize, t: f64) -> Option<Posed> {
        let (p0, p1, p2, p3) = self.controls(k)?;
        let u = 1.0 - t;
        let b0 = u * u * u;
        let b1 = 3.0 * u * u * t;
        let b2 = 3.0 * u * t * t;
        let b3 = t * t * t;
        let value = add(add(mul(p0, b0), mul(p1, b1)), add(mul(p2, b2), mul(p3, b3)));
        let d1 = mul(sub(p1, p0), 3.0);
        let d2 = mul(sub(p2, p1), 3.0);
        let d3 = mul(sub(p3, p2), 3.0);
        let first = add(add(mul(d1, u * u), mul(d2, 2.0 * u * t)), mul(d3, t * t));
        let left = sub(add(p2, p0), mul(p1, 2.0));
        let right = add(sub(p3, mul(p2, 2.0)), p1);
        let second = add(mul(left, 6.0 * u), mul(right, 6.0 * t));
        Some(Posed {
            value,
            first,
            second,
        })
    }

    /// Evaluate the spline at the global parameter `u` in `[0, spans]`.
    pub fn evaluate_global(&self, u: f64) -> Option<Posed> {
        let spans = self.spans();
        if spans == 0 {
            return None;
        }
        let clamped = u.clamp(0.0, spans as f64);
        let k = (clamped.floor() as usize).min(spans - 1);
        self.evaluate(k, clamped - k as f64)
    }
}

/// A cubic traversed in the opposite direction keeps its geometric trace.
pub fn reverse_segment(segment: BezierSegment) -> BezierSegment {
    BezierSegment {
        p0: segment.p3,
        p1: segment.p2,
        p2: segment.p1,
        p3: segment.p0,
    }
}
