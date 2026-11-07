//! Two-dimensional vector algebra on the frozen `Point2D<f64>` contract.

use kurva_types::Point2D;

pub fn add(a: Point2D<f64>, b: Point2D<f64>) -> Point2D<f64> {
    Point2D {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}

pub fn sub(a: Point2D<f64>, b: Point2D<f64>) -> Point2D<f64> {
    Point2D {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

pub fn mul(a: Point2D<f64>, s: f64) -> Point2D<f64> {
    Point2D {
        x: a.x * s,
        y: a.y * s,
    }
}

pub fn dot(a: Point2D<f64>, b: Point2D<f64>) -> f64 {
    a.x * b.x + a.y * b.y
}

pub fn cross(a: Point2D<f64>, b: Point2D<f64>) -> f64 {
    a.x * b.y - a.y * b.x
}

pub fn norm(a: Point2D<f64>) -> f64 {
    dot(a, a).sqrt()
}

/// Unit direction, or `None` when the input is degenerate.
pub fn unit(a: Point2D<f64>) -> Option<Point2D<f64>> {
    let length = norm(a);
    if length > f64::EPSILON {
        Some(mul(a, 1.0 / length))
    } else {
        None
    }
}

/// Left normal `(-y, x)`, the inward normal used by the G2 residual.
pub fn perp(a: Point2D<f64>) -> Point2D<f64> {
    Point2D { x: -a.y, y: a.x }
}
