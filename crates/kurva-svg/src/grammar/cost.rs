//! Byte cost model for command candidates.

use crate::digits::number_len;
use crate::lattice::{Lattice, LatticePoint};

use super::{Command, Form, GrammarContext};

/// Widest coordinate run of a single command.
pub(crate) const MAX_COORDS: usize = 6;

/// Coordinates a chosen command will stream, in emission order.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Coords {
    values: [i64; MAX_COORDS],
    len: usize,
}

impl Coords {
    pub(crate) fn as_slice(&self) -> &[i64] {
        &self.values[..self.len]
    }

    pub(crate) fn push(&mut self, value: i64) {
        if self.len < MAX_COORDS {
            self.values[self.len] = value;
            self.len += 1;
        }
    }

    /// Flattens `points` into a coordinate run for the requested frame.
    pub(crate) fn from_points(points: &[LatticePoint], form: Form, origin: LatticePoint) -> Self {
        let mut coords = Self::default();
        for point in points {
            match form {
                Form::Absolute => {
                    coords.push(point.x);
                    coords.push(point.y);
                }
                Form::Relative => {
                    coords.push(point.x.saturating_sub(origin.x));
                    coords.push(point.y.saturating_sub(origin.y));
                }
            }
        }
        coords
    }

    /// Exact emitted width, including separators and the optional letter.
    fn cost(self, letter: bool, lattice: Lattice) -> usize {
        let mut total = usize::from(letter);
        let mut follows_letter = letter;
        for &value in self.as_slice() {
            let separator = usize::from(!follows_letter && value >= 0);
            total += number_len(value, lattice) + separator;
            follows_letter = false;
        }
        total
    }
}

/// Running byte minimum across every legal candidate.
pub(crate) struct Best {
    pub(crate) command: Command,
    pub(crate) form: Form,
    pub(crate) elide: bool,
    pub(crate) coords: Coords,
    cost: usize,
}

impl Best {
    pub(crate) fn start() -> Self {
        Self {
            command: Command::Cubic,
            form: Form::Absolute,
            elide: false,
            coords: Coords::default(),
            cost: usize::MAX,
        }
    }

    /// Offers one candidate; ties break towards the later, more reduced form.
    pub(crate) fn consider(
        &mut self,
        context: &GrammarContext,
        command: Command,
        form: Form,
        coords: Coords,
        lattice: Lattice,
    ) {
        let elide = context.allows_elision(command, form);
        let total = coords.cost(!elide, lattice);
        if total <= self.cost {
            self.command = command;
            self.form = form;
            self.elide = elide;
            self.cost = total;
            self.coords = coords;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_runs_beat_absolute_runs() {
        let lattice = Lattice::new(2);
        let origin = LatticePoint::new(10_000, 10_000);
        let points = [LatticePoint::new(10_010, 10_020)];

        let absolute = Coords::from_points(&points, Form::Absolute, origin);
        let relative = Coords::from_points(&points, Form::Relative, origin);
        assert_eq!(relative.as_slice(), &[10, 20]);
        assert!(relative.cost(true, lattice) < absolute.cost(true, lattice));
    }

    #[test]
    fn separators_are_not_charged_before_negatives() {
        let lattice = Lattice::new(2);
        let mut coords = Coords::default();
        coords.push(-50);
        coords.push(50);
        // "M-.5 .5" is seven bytes: no space is needed before the minus sign.
        assert_eq!(coords.cost(true, lattice), 1 + 3 + 3);
    }
}
