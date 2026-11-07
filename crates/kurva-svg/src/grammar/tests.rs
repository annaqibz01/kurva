//! Unit tests for the command grammar automaton.

use crate::lattice::{Lattice, LatticePoint};
use crate::segment::Segment;

use super::{choose, Command, Form, GrammarContext};

fn point(x: i64, y: i64) -> LatticePoint {
    LatticePoint::new(x, y)
}

#[test]
fn grammar_degree_reduction() {
    let lattice = Lattice::new(2);
    let context = GrammarContext::new(point(0, 0));

    let straight = Segment {
        start: point(0, 0),
        first: point(50, 0),
        second: point(75, 0),
        end: point(100, 0),
    };
    let decision = choose(&context, &straight, lattice);
    assert_eq!(decision.command, Command::Horizontal);
    assert_eq!(decision.coords.as_slice(), &[100]);

    let vertical = Segment {
        start: point(0, 0),
        first: point(0, 40),
        second: point(0, 60),
        end: point(0, 100),
    };
    assert_eq!(
        choose(&context, &vertical, lattice).command,
        Command::Vertical
    );

    let diagonal = Segment {
        start: point(0, 0),
        first: point(30, 30),
        second: point(60, 60),
        end: point(100, 100),
    };
    assert_eq!(choose(&context, &diagonal, lattice).command, Command::Line);

    // Degree-elevated quadratic: control point (60, 120).
    let elevated = Segment {
        start: point(0, 0),
        first: point(40, 80),
        second: point(80, 80),
        end: point(120, 0),
    };
    assert_eq!(elevated.quadratic_control(), Some(point(60, 120)));
    let decision = choose(&context, &elevated, lattice);
    assert_eq!(decision.command, Command::Quadratic);
    assert_eq!(decision.coords.as_slice(), &[60, 120, 120, 0]);
}

#[test]
fn smooth_reflection_s_t_guards() {
    let lattice = Lattice::new(2);

    let cubic = Segment {
        start: point(0, 0),
        first: point(10, 50),
        second: point(90, 50),
        end: point(100, 0),
    };
    let mut context = GrammarContext::new(cubic.start);
    let decision = choose(&context, &cubic, lattice);
    assert_eq!(decision.command, Command::Cubic);
    context.advance(decision.command, decision.form, &cubic);

    // Second control point mirrors across the pen: (110, -50).
    let smooth = Segment {
        start: point(100, 0),
        first: point(110, -50),
        second: point(150, -40),
        end: point(200, -20),
    };
    let decision = choose(&context, &smooth, lattice);
    assert_eq!(decision.command, Command::Smooth);
    assert_eq!(decision.form, Form::Relative);
    assert_eq!(decision.coords.as_slice(), &[50, -40, 100, -20]);

    // The same segment is not smooth once a line broke the chain.
    let mut line_context = GrammarContext::new(point(0, 0));
    let line = Segment {
        start: point(0, 0),
        first: point(30, 0),
        second: point(60, 0),
        end: point(100, 0),
    };
    let decision = choose(&line_context, &line, lattice);
    line_context.advance(decision.command, decision.form, &line);
    assert_ne!(
        choose(&line_context, &smooth, lattice).command,
        Command::Smooth
    );

    // T mirrors the previous quadratic control point (180, -120).
    let first_quad = Segment {
        start: point(0, 0),
        first: point(40, 80),
        second: point(80, 80),
        end: point(120, 0),
    };
    let mut quad_context = GrammarContext::new(point(0, 0));
    let decision = choose(&quad_context, &first_quad, lattice);
    assert_eq!(decision.command, Command::Quadratic);
    quad_context.advance(decision.command, decision.form, &first_quad);

    let reflected = Segment {
        start: point(120, 0),
        first: point(160, -80),
        second: point(200, -80),
        end: point(240, 0),
    };
    let decision = choose(&quad_context, &reflected, lattice);
    assert_eq!(decision.command, Command::SmoothQuadratic);
    assert_eq!(decision.form, Form::Relative);
    assert_eq!(decision.coords.as_slice(), &[120, 0]);

    // Without the quadratic predecessor the same segment is a plain Q.
    let fresh = GrammarContext::new(point(120, 0));
    assert_eq!(
        choose(&fresh, &reflected, lattice).command,
        Command::Quadratic
    );
}

#[test]
fn command_letter_is_elided_on_repeats() {
    let lattice = Lattice::new(2);
    let mut context = GrammarContext::new(point(0, 0));
    let first = Segment {
        start: point(0, 0),
        first: point(10, 50),
        second: point(30, 50),
        end: point(40, 0),
    };
    let decision = choose(&context, &first, lattice);
    assert_eq!(decision.command, Command::Cubic);
    assert!(!decision.elide);
    context.advance(decision.command, decision.form, &first);

    let second = Segment {
        start: point(40, 0),
        first: point(50, 40),
        second: point(70, 40),
        end: point(80, 0),
    };
    let decision = choose(&context, &second, lattice);
    assert_eq!(decision.command, Command::Cubic);
    assert!(decision.elide);
}
