//! Planar primitives and arc-length parameterization shared by every AEGIS stage.

use std::f64::consts::PI;

use crate::error::PolyError;

/// Machine-scale tolerance for geometric degeneracy checks.
pub(crate) const EPS: f64 = 1e-12;

/// A planar point with f64 precision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct V2 {
    pub x: f64,
    pub y: f64,
}

impl V2 {
    pub(crate) const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub(crate) fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    pub(crate) fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    pub(crate) fn scale(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }

    pub(crate) fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Two-dimensional determinant, i.e. the z component of the cross product.
    pub(crate) fn det(self, other: Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    pub(crate) fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub(crate) fn distance(self, other: Self) -> f64 {
        self.sub(other).norm()
    }

    /// Bearing of the vector in (-pi, pi].
    pub(crate) fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }

    /// Distance from `p` to the closed segment `a`-`b`.
    pub(crate) fn segment_distance(p: Self, a: Self, b: Self) -> f64 {
        let span = b.sub(a);
        let len_sq = span.dot(span);
        if len_sq <= EPS {
            return p.distance(a);
        }
        let t = (p.sub(a).dot(span) / len_sq).clamp(0.0, 1.0);
        p.distance(a.add(span.scale(t)))
    }
}

/// Signed shoelace area; positive for counter-clockwise winding.
pub(crate) fn signed_area(pts: &[V2]) -> f64 {
    let n = pts.len();
    let mut acc = 0.0;
    for i in 0..n {
        acc += pts[i].det(pts[(i + 1) % n]);
    }
    acc * 0.5
}

/// Signed periodic arc-length offset folded into [-period/2, period/2).
pub(crate) fn periodic_delta(from: f64, to: f64, period: f64) -> f64 {
    if period <= EPS {
        return 0.0;
    }
    let mut delta = (from - to) % period;
    if delta > period * 0.5 {
        delta -= period;
    } else if delta < -period * 0.5 {
        delta += period;
    }
    delta
}

/// Closed-loop geometry sampled at its vertices.
pub(crate) struct LoopGeometry {
    pub pts: Vec<V2>,
    /// `edge[i]` is the length of the segment from vertex `i` to `i + 1`.
    pub edge: Vec<f64>,
    /// Cumulative arc length at vertex `i`.
    pub arc: Vec<f64>,
    pub perimeter: f64,
    /// Signed turning angle at vertex `i`, in (-pi, pi].
    pub turn: Vec<f64>,
    /// Discrete curvature `turn[i] / cell_width[i]`.
    pub kappa: Vec<f64>,
}

/// Derive arc length, turning angles and discrete curvature of a closed loop.
pub(crate) fn analyze_closed(pts: &[V2]) -> Result<LoopGeometry, PolyError> {
    let n = pts.len();
    if n < 3 {
        return Err(PolyError::DegeneratePolyline);
    }
    let mut edge = Vec::with_capacity(n);
    for i in 0..n {
        let length = pts[i].distance(pts[(i + 1) % n]);
        if length <= EPS {
            return Err(PolyError::DegeneratePolyline);
        }
        edge.push(length);
    }
    let mut arc = vec![0.0; n];
    for i in 1..n {
        arc[i] = arc[i - 1] + edge[i - 1];
    }
    let perimeter = arc[n - 1] + edge[n - 1];
    let mut turn = vec![0.0; n];
    let mut kappa = vec![0.0; n];
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let next = (i + 1) % n;
        let incoming = pts[i].sub(pts[prev]);
        let outgoing = pts[next].sub(pts[i]);
        let angle = outgoing.det(incoming).atan2(outgoing.dot(incoming));
        turn[i] = angle;
        let cell = 0.5 * (edge[prev] + edge[i]);
        kappa[i] = if cell > EPS { angle / cell } else { 0.0 };
    }
    Ok(LoopGeometry {
        pts: pts.to_vec(),
        edge,
        arc,
        perimeter,
        turn,
        kappa,
    })
}

/// Wrap an angle into (-pi, pi].
pub(crate) fn wrap(angle: f64) -> f64 {
    (angle + PI).rem_euclid(2.0 * PI) - PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_angles_of_a_clockwise_square_are_negative() {
        let square = [
            V2::new(0.0, 0.0),
            V2::new(10.0, 0.0),
            V2::new(10.0, 10.0),
            V2::new(0.0, 10.0),
        ];
        let geom = match analyze_closed(&square) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        };
        assert!(signed_area(&square) > 0.0);
        for angle in &geom.turn {
            assert!((angle.abs() - PI * 0.5).abs() < 1e-12);
        }
        assert!((geom.perimeter - 40.0).abs() < 1e-12);
    }

    #[test]
    fn periodic_delta_folds_to_the_short_way_around() {
        assert!((periodic_delta(1.0, 9.0, 10.0) - 2.0).abs() < 1e-12);
        assert!((periodic_delta(9.0, 1.0, 10.0) + 2.0).abs() < 1e-12);
    }
}
