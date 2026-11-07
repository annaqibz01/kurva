//! Pen, subpath origin and the two reflection control points.
//!
//! The context is the memory of the grammar automaton: it decides which
//! commands are legal (`S` and `T` need a matching predecessor) and which
//! command letter may be elided.

use crate::lattice::LatticePoint;
use crate::segment::Segment;

use super::{Command, Form, State};

/// Mutable automaton state carried across the segments of one subpath.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GrammarContext {
    pen: LatticePoint,
    origin: LatticePoint,
    state: State,
    last: Option<(Command, Form)>,
    cubic_control: LatticePoint,
    quad_control: LatticePoint,
}

impl GrammarContext {
    pub(crate) fn new(origin: LatticePoint) -> Self {
        Self {
            pen: origin,
            origin,
            state: State::Start,
            last: None,
            cubic_control: origin,
            quad_control: origin,
        }
    }

    pub(crate) const fn pen(self) -> LatticePoint {
        self.pen
    }

    pub(crate) const fn state(self) -> State {
        self.state
    }

    /// Mirror of the previous cubic control point across the pen.
    pub(crate) fn cubic_reflection(self) -> LatticePoint {
        self.cubic_control.reflected(self.pen)
    }

    /// Mirror of the previous quadratic control point across the pen.
    pub(crate) fn quadratic_reflection(self) -> LatticePoint {
        self.quad_control.reflected(self.pen)
    }

    /// True when `command` repeats the active letter in the same frame.
    pub(crate) fn allows_elision(self, command: Command, form: Form) -> bool {
        matches!(
            self.last,
            Some((previous, previous_form)) if previous == command && previous_form == form
        )
    }

    pub(crate) fn advance_move(&mut self) {
        self.state = State::Move(Form::Absolute);
        self.last = Some((Command::Move, Form::Absolute));
    }

    pub(crate) fn advance(&mut self, command: Command, form: Form, segment: &Segment) {
        self.last = Some((command, form));
        self.state = match command {
            Command::Move => State::Move(form),
            Command::Cubic | Command::Smooth => State::Cubic(form),
            Command::Quadratic | Command::SmoothQuadratic => State::Quad(form),
            Command::Line | Command::Horizontal | Command::Vertical => State::Line(form),
            Command::Close => State::Close(form),
        };
        match command {
            Command::Cubic | Command::Smooth => self.cubic_control = segment.second,
            Command::Quadratic | Command::SmoothQuadratic => {
                self.quad_control = segment.quadratic_control().unwrap_or(segment.first);
            }
            _ => {}
        }
        self.pen = segment.end;
    }

    /// Enters the `Za` state and returns the pen to the subpath origin.
    pub(crate) fn close(&mut self) {
        self.state = State::Close(Form::Absolute);
        self.last = Some((Command::Close, Form::Absolute));
        self.pen = self.origin;
    }

    /// Terminal transition: no further segment may follow a close.
    pub(crate) fn seal(&mut self) {
        self.state = State::Closed;
    }
}
