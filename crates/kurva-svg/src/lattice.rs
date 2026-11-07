//! Canonical lattice pre-quantization (ALTO-v3 stage 1).
//!
//! Coordinates are scaled by `10^k` and rounded half-to-even into `i64`, so
//! two faces that share an edge quantize that edge to identical integers. The
//! reconstructed boundary is therefore bit-identical across faces and the
//! Hausdorff error stays below `sqrt(2) / 2 * delta`.

/// Default decimal scale, `delta = 0.01`.
pub(crate) const DEFAULT_SCALE: u32 = 2;
/// Lowest supported decimal scale.
pub(crate) const MIN_SCALE: u32 = 2;
/// Highest supported decimal scale.
pub(crate) const MAX_SCALE: u32 = 3;
/// Widest fixed-point fraction, in digits.
pub(crate) const MAX_FRACTION_DIGITS: usize = MAX_SCALE as usize;

/// A coordinate pair on the integer lattice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LatticePoint {
    pub(crate) x: i64,
    pub(crate) y: i64,
}

impl LatticePoint {
    pub(crate) const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    /// Vector from `other` to `self`.
    pub(crate) const fn offset_by(self, other: Self) -> Self {
        Self {
            x: self.x.saturating_sub(other.x),
            y: self.y.saturating_sub(other.y),
        }
    }

    pub(crate) fn dot(self, other: Self) -> i128 {
        i128::from(self.x) * i128::from(other.x) + i128::from(self.y) * i128::from(other.y)
    }

    pub(crate) fn cross(self, other: Self) -> i128 {
        i128::from(self.x) * i128::from(other.y) - i128::from(self.y) * i128::from(other.x)
    }

    /// Mirror of `self` across `pivot`, that is `2 * pivot - self`.
    pub(crate) const fn reflected(self, pivot: Self) -> Self {
        Self {
            x: pivot.x.saturating_mul(2).saturating_sub(self.x),
            y: pivot.y.saturating_mul(2).saturating_sub(self.y),
        }
    }
}

/// Decimal scale factor `10^k`, clamped to the supported range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Lattice {
    scale: u32,
}

impl Lattice {
    pub(crate) fn new(scale: u32) -> Self {
        Self {
            scale: scale.clamp(MIN_SCALE, MAX_SCALE),
        }
    }

    pub(crate) const fn scale(self) -> u32 {
        self.scale
    }

    pub(crate) fn divisor(self) -> i64 {
        10_i64.pow(self.scale)
    }

    /// `Q_delta(x) = delta * round_half_to_even(x / delta)` on the lattice.
    pub(crate) fn quantize(self, value: f64) -> i64 {
        let scaled = value * self.divisor() as f64;
        if !scaled.is_finite() {
            return 0;
        }
        round_half_to_even(scaled.clamp(i64::MIN as f64, i64::MAX as f64))
    }

    pub(crate) fn point(self, x: f64, y: f64) -> LatticePoint {
        LatticePoint::new(self.quantize(x), self.quantize(y))
    }
}

fn round_half_to_even(value: f64) -> i64 {
    let floor = value.floor();
    let fraction = value - floor;
    let base = floor as i64;
    if fraction > 0.5 {
        base.saturating_add(1)
    } else if fraction < 0.5 || base & 1 == 0 {
        base
    } else {
        base.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantization_rounds_half_to_even() {
        let lattice = Lattice::new(2);
        // Exact binary ties: 12.5, 37.5, -12.5 and -37.5 in scaled space.
        assert_eq!(lattice.quantize(0.125), 12);
        assert_eq!(lattice.quantize(0.375), 38);
        assert_eq!(lattice.quantize(-0.125), -12);
        assert_eq!(lattice.quantize(-0.375), -38);
        assert_eq!(lattice.quantize(0.754), 75);
        assert_eq!(lattice.quantize(f64::NAN), 0);
        assert_eq!(lattice.quantize(f64::INFINITY), 0);
    }

    #[test]
    fn points_share_edges_bit_identically() {
        let lattice = Lattice::new(DEFAULT_SCALE);
        let left = lattice.point(12.25, -6.75);
        let right = lattice.point(-6.75, 12.25);
        assert_eq!(left, LatticePoint::new(1225, -675));
        assert_eq!(left.offset_by(left), LatticePoint::new(0, 0));
        assert_eq!(right.reflected(right), right);
        assert_eq!(LatticePoint::new(1, 0).cross(LatticePoint::new(0, 1)), 1);
    }

    #[test]
    fn scale_is_clamped_to_supported_range() {
        assert_eq!(Lattice::new(0).scale(), MIN_SCALE);
        assert_eq!(Lattice::new(9).scale(), MAX_SCALE);
        assert_eq!(Lattice::new(3).divisor(), 1000);
    }
}
