//! Subpath emission: quantized segments into minimal path data.
//!
//! A `VectorPath` is one continuous subpath: consecutive segments chain
//! `p3 -> p0`, so the pen doubles as the start of the next segment and the
//! shared edge is quantized exactly once. A discontinuity in `p0` starts a
//! new subpath rather than bridging the gap.

use kurva_types::{ColorRgb, VectorPath};

use crate::ascii::IntToFixedASCII;
use crate::grammar::{self, Command, Form, GrammarContext};
use crate::lattice::{Lattice, LatticePoint};
use crate::segment::Segment;

/// Emits one compound `<path>` holding every subpath that shares `fill`.
pub(crate) fn emit_compound(
    out: &mut IntToFixedASCII,
    paths: &[&VectorPath],
    fill: ColorRgb,
    lattice: Lattice,
) {
    let mut opened = false;
    for path in paths {
        if path.segments.is_empty() {
            continue;
        }
        if !opened {
            out.push_bytes(b"<path d=\"");
            opened = true;
        }
        emit_subpath(out, path, lattice);
    }
    if !opened {
        return;
    }
    out.push_bytes(b"\" fill=\"");
    out.push_hex(fill);
    out.push_bytes(b"\" fill-rule=\"nonzero\"/>");
}

fn emit_subpath(out: &mut IntToFixedASCII, path: &VectorPath, lattice: Lattice) {
    let Some(first) = path.segments.first() else {
        return;
    };
    let mut origin = lattice.point(first.p0.x, first.p0.y);
    let mut context = GrammarContext::new(origin);
    begin_subpath(out, &mut context, origin);

    let last_index = path.segments.len().saturating_sub(1);
    for (index, raw) in path.segments.iter().enumerate() {
        let declared = lattice.point(raw.p0.x, raw.p0.y);
        if declared != context.pen() {
            origin = declared;
            context = GrammarContext::new(origin);
            begin_subpath(out, &mut context, origin);
        }
        let segment = Segment {
            start: context.pen(),
            first: lattice.point(raw.p1.x, raw.p1.y),
            second: lattice.point(raw.p2.x, raw.p2.y),
            end: lattice.point(raw.p3.x, raw.p3.y),
        };
        let closing = path.is_closed && index == last_index;
        // A straight closing edge carries no geometry beyond the close itself.
        if closing && segment.end == origin && segment.linear().is_some() {
            close(out, &mut context);
            return;
        }
        let decision = grammar::choose(&context, &segment, lattice);
        if !decision.elide {
            out.push_command(decision.command.letter(decision.form));
        }
        for value in decision.coords.as_slice() {
            out.push_coord(*value);
        }
        context.advance(decision.command, decision.form, &segment);
        if closing {
            close(out, &mut context);
            return;
        }
    }
}

fn begin_subpath(out: &mut IntToFixedASCII, context: &mut GrammarContext, origin: LatticePoint) {
    out.push_command(Command::Move.letter(Form::Absolute));
    out.push_point(origin);
    context.advance_move();
}

fn close(out: &mut IntToFixedASCII, context: &mut GrammarContext) {
    out.push_command(Command::Close.letter(Form::Absolute));
    context.close();
    context.seal();
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurva_types::{BezierSegment, Point2D};

    fn point(value: (f64, f64)) -> Point2D<f64> {
        Point2D {
            x: value.0,
            y: value.1,
        }
    }

    fn cubic(
        start: (f64, f64),
        first: (f64, f64),
        second: (f64, f64),
        end: (f64, f64),
    ) -> BezierSegment {
        BezierSegment {
            p0: point(start),
            p1: point(first),
            p2: point(second),
            p3: point(end),
        }
    }

    fn render(path: &VectorPath) -> String {
        let lattice = Lattice::new(2);
        let mut out = IntToFixedASCII::with_capacity(512, lattice);
        emit_subpath(&mut out, path, lattice);
        String::from_utf8(out.into_bytes()).expect("ascii only")
    }

    #[test]
    fn z_closure_preservation() {
        let square = VectorPath {
            color_id: 0,
            is_closed: true,
            segments: vec![
                cubic((0.0, 0.0), (2.0, 0.0), (6.0, 0.0), (8.0, 0.0)),
                cubic((8.0, 0.0), (8.0, 2.0), (8.0, 6.0), (8.0, 8.0)),
                cubic((8.0, 8.0), (6.0, 8.0), (2.0, 8.0), (0.0, 8.0)),
                cubic((0.0, 8.0), (0.0, 6.0), (0.0, 2.0), (0.0, 0.0)),
            ],
        };
        let data = render(&square);
        assert!(data.ends_with('Z'));
        assert!(!data.contains('C'));

        // A curved closing edge is emitted before the close, never flattened.
        let curved = VectorPath {
            color_id: 0,
            is_closed: true,
            segments: vec![
                cubic((0.0, 0.0), (1.0, -3.0), (3.0, -4.0), (5.0, 0.0)),
                cubic((5.0, 0.0), (7.0, 4.0), (9.0, 3.0), (10.0, 0.0)),
                cubic((10.0, 0.0), (7.0, -6.0), (2.0, -2.0), (0.0, 0.0)),
            ],
        };
        let data = render(&curved);
        assert!(data.ends_with('Z'));
        assert!(data.contains('C') || data.contains('c'));
    }

    #[test]
    fn discontinuous_input_starts_a_new_subpath() {
        let path = VectorPath {
            color_id: 0,
            is_closed: false,
            segments: vec![
                cubic((0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)),
                cubic((10.0, 10.0), (11.0, 10.0), (12.0, 10.0), (13.0, 10.0)),
            ],
        };
        let data = render(&path);
        assert_eq!(data.matches('M').count(), 2);
        assert!(!data.contains("10 10-"), "no coordinate may bridge the gap");
    }
}
