//! Context-safe command grammar (ALTO-v3 stage 3).
//!
//! The automaton tracks twelve states: `start`, `closed`, and the absolute /
//! relative form of the move, cubic, quadratic, line and close commands
//! (`Ma`, `Mr`, `Ca`, `Cr`, `Qa`, `Qr`, `La`, `Lr`, `Za`, `Zr`). Every legal
//! encoding of a segment is costed in bytes and the cheapest one wins, with
//! the command letter elided whenever the active command repeats.

mod context;
mod cost;

#[cfg(test)]
mod tests;

use crate::lattice::{Lattice, LatticePoint};
use crate::segment::{Axis, Segment};

pub(crate) use context::GrammarContext;
use cost::{Best, Coords};

/// Byte-minimizing SVG path commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Move,
    Line,
    Horizontal,
    Vertical,
    Cubic,
    Smooth,
    Quadratic,
    SmoothQuadratic,
    Close,
}

impl Command {
    pub(crate) const fn letter(self, form: Form) -> u8 {
        match (self, form) {
            (Self::Move, Form::Absolute) => b'M',
            (Self::Move, Form::Relative) => b'm',
            (Self::Line, Form::Absolute) => b'L',
            (Self::Line, Form::Relative) => b'l',
            (Self::Horizontal, Form::Absolute) => b'H',
            (Self::Horizontal, Form::Relative) => b'h',
            (Self::Vertical, Form::Absolute) => b'V',
            (Self::Vertical, Form::Relative) => b'v',
            (Self::Cubic, Form::Absolute) => b'C',
            (Self::Cubic, Form::Relative) => b'c',
            (Self::Smooth, Form::Absolute) => b'S',
            (Self::Smooth, Form::Relative) => b's',
            (Self::Quadratic, Form::Absolute) => b'Q',
            (Self::Quadratic, Form::Relative) => b'q',
            (Self::SmoothQuadratic, Form::Absolute) => b'T',
            (Self::SmoothQuadratic, Form::Relative) => b't',
            (Self::Close, Form::Absolute) => b'Z',
            (Self::Close, Form::Relative) => b'z',
        }
    }
}

/// Coordinate frame of the active command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    Absolute,
    Relative,
}

/// Grammar state; `start` and `closed` are the two non-command states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Start,
    Move(Form),
    Cubic(Form),
    Quad(Form),
    Line(Form),
    Close(Form),
    Closed,
}

/// A chosen command plus the exact coordinates to emit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Decision {
    pub(crate) command: Command,
    pub(crate) form: Form,
    pub(crate) elide: bool,
    pub(crate) coords: Coords,
}

/// Picks the cheapest legal encoding of `segment`.
pub(crate) fn choose(context: &GrammarContext, segment: &Segment, lattice: Lattice) -> Decision {
    let mut best = Best::start();
    let start = segment.start;

    // Cubic is unconditionally legal.
    offer(
        &mut best,
        context,
        Command::Cubic,
        &[segment.first, segment.second, segment.end],
        start,
        lattice,
    );

    // Smooth cubic: first control point mirrors the previous cubic control.
    if matches!(context.state(), State::Cubic(_)) && segment.first == context.cubic_reflection() {
        offer(
            &mut best,
            context,
            Command::Smooth,
            &[segment.second, segment.end],
            start,
            lattice,
        );
    }

    if let Some(control) = segment.quadratic_control() {
        // Smooth quadratic: reduced control mirrors the previous quad control.
        if matches!(context.state(), State::Quad(_)) && control == context.quadratic_reflection() {
            offer(
                &mut best,
                context,
                Command::SmoothQuadratic,
                &[segment.end],
                start,
                lattice,
            );
        }
        offer(
            &mut best,
            context,
            Command::Quadratic,
            &[control, segment.end],
            start,
            lattice,
        );
    }

    if let Some(axis) = segment.linear() {
        match axis {
            Axis::Horizontal => offer_scalar(
                &mut best,
                context,
                Command::Horizontal,
                segment.end.x,
                segment.end.x.saturating_sub(start.x),
                lattice,
            ),
            Axis::Vertical => offer_scalar(
                &mut best,
                context,
                Command::Vertical,
                segment.end.y,
                segment.end.y.saturating_sub(start.y),
                lattice,
            ),
            Axis::Any => {}
        }
        offer(
            &mut best,
            context,
            Command::Line,
            &[segment.end],
            start,
            lattice,
        );
    }

    Decision {
        command: best.command,
        form: best.form,
        elide: best.elide,
        coords: best.coords,
    }
}

fn offer(
    best: &mut Best,
    context: &GrammarContext,
    command: Command,
    points: &[LatticePoint],
    start: LatticePoint,
    lattice: Lattice,
) {
    for form in [Form::Absolute, Form::Relative] {
        let coords = Coords::from_points(points, form, start);
        best.consider(context, command, form, coords, lattice);
    }
}

fn offer_scalar(
    best: &mut Best,
    context: &GrammarContext,
    command: Command,
    absolute: i64,
    relative: i64,
    lattice: Lattice,
) {
    for (form, value) in [(Form::Absolute, absolute), (Form::Relative, relative)] {
        let mut coords = Coords::default();
        coords.push(value);
        best.consider(context, command, form, coords, lattice);
    }
}
