//! Segment geometry evaluated on the integer lattice.

use crate::lattice::LatticePoint;

/// Tolerance, in lattice units, accepted for degree reduction.
const REDUCTION_TOLERANCE: i128 = 1;

/// Axis a collapsed cubic runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Any,
    Horizontal,
    Vertical,
}

/// A cubic segment whose end points live on the integer lattice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) start: LatticePoint,
    pub(crate) first: LatticePoint,
    pub(crate) second: LatticePoint,
    pub(crate) end: LatticePoint,
}

impl Segment {
    /// Returns the axis when the cubic traces its chord exactly.
    pub(crate) fn linear(&self) -> Option<Axis> {
        if self.end == self.start || !self.on_chord(self.first) || !self.on_chord(self.second) {
            return None;
        }
        if self.end.y == self.start.y {
            Some(Axis::Horizontal)
        } else if self.end.x == self.start.x {
            Some(Axis::Vertical)
        } else {
            Some(Axis::Any)
        }
    }

    /// Control point of the quadratic this cubic is a degree elevation of.
    pub(crate) fn quadratic_control(&self) -> Option<LatticePoint> {
        let first_x = 3 * i128::from(self.first.x) - i128::from(self.start.x);
        let first_y = 3 * i128::from(self.first.y) - i128::from(self.start.y);
        let last_x = 3 * i128::from(self.second.x) - i128::from(self.end.x);
        let last_y = 3 * i128::from(self.second.y) - i128::from(self.end.y);
        if (first_x - last_x).abs() > REDUCTION_TOLERANCE
            || (first_y - last_y).abs() > REDUCTION_TOLERANCE
        {
            return None;
        }
        Some(LatticePoint::new(split_half(first_x), split_half(first_y)))
    }

    /// True when `point` lies on the chord spanned by start and end.
    fn on_chord(&self, point: LatticePoint) -> bool {
        let chord = self.end.offset_by(self.start);
        let arm = point.offset_by(self.start);
        chord.cross(arm) == 0 && chord.dot(arm) >= 0 && chord.dot(chord) >= arm.dot(arm)
    }
}

fn split_half(value: i128) -> i64 {
    let half = value / 2;
    let rounded = if value % 2 == 0 {
        half
    } else if value > 0 {
        half + 1
    } else {
        half - 1
    };
    rounded.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}
