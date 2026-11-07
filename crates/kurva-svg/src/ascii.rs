//! Zero-allocation fixed-point byte emitter (ALTO-v3 stage 2).
//!
//! Coordinates are streamed straight from their integer lattice form into the
//! document buffer: no float formatting, no temporary `String`, and no
//! allocation once the buffer capacity is reserved.

use kurva_types::ColorRgb;

use crate::digits::write_unsigned;
use crate::lattice::{Lattice, LatticePoint, MAX_FRACTION_DIGITS};

/// Streams SVG bytes into a pre-reserved buffer.
pub(crate) struct IntToFixedASCII {
    buffer: Vec<u8>,
    scale: u32,
    divisor: u64,
    after_command: bool,
}

impl IntToFixedASCII {
    pub(crate) fn with_capacity(capacity: usize, lattice: Lattice) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            scale: lattice.scale(),
            divisor: lattice.divisor() as u64,
            after_command: true,
        }
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }

    /// Appends literal markup. Nothing after it needs a separator.
    pub(crate) fn push_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
        self.after_command = true;
    }

    /// Appends a command letter; the first coordinate needs no separator.
    pub(crate) fn push_command(&mut self, letter: u8) {
        self.buffer.push(letter);
        self.after_command = true;
    }

    /// Appends one fixed-point coordinate with SVG 1.1 token separation.
    pub(crate) fn push_coord(&mut self, value: i64) {
        if !self.after_command && value >= 0 {
            self.buffer.push(b' ');
        }
        self.after_command = false;
        self.push_number(value);
    }

    pub(crate) fn push_point(&mut self, point: LatticePoint) {
        self.push_coord(point.x);
        self.push_coord(point.y);
    }

    /// Appends an unsigned decimal integer, used for canvas dimensions.
    pub(crate) fn push_unsigned(&mut self, value: u64) {
        write_unsigned(&mut self.buffer, value);
    }

    /// Appends `#rrggbb` with lowercase hex digits.
    pub(crate) fn push_hex(&mut self, color: ColorRgb) {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        self.buffer.push(b'#');
        for channel in [color.r, color.g, color.b] {
            self.buffer.push(DIGITS[usize::from(channel >> 4)]);
            self.buffer.push(DIGITS[usize::from(channel & 0x0f)]);
        }
        self.after_command = true;
    }

    /// Writes `value / 10^k`, dropping trailing zeros and a redundant `0`.
    fn push_number(&mut self, value: i64) {
        if value < 0 {
            self.buffer.push(b'-');
        }
        let magnitude = value.unsigned_abs();
        let quotient = magnitude / self.divisor;
        let remainder = magnitude % self.divisor;
        if quotient > 0 {
            write_unsigned(&mut self.buffer, quotient);
        } else if remainder == 0 {
            self.buffer.push(b'0');
        }
        if remainder == 0 {
            return;
        }
        self.buffer.push(b'.');
        let mut digits = [b'0'; MAX_FRACTION_DIGITS];
        let mut rest = remainder;
        let mut index = self.scale as usize;
        while index > 0 {
            index -= 1;
            digits[index] = b'0' + (rest % 10) as u8;
            rest /= 10;
        }
        let mut end = self.scale as usize;
        while end > 0 && digits[end - 1] == b'0' {
            end -= 1;
        }
        self.buffer.extend_from_slice(&digits[..end]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digits::number_len;

    fn emit(value: i64, scale: u32) -> String {
        let mut out = IntToFixedASCII::with_capacity(24, Lattice::new(scale));
        out.push_coord(value);
        String::from_utf8(out.into_bytes()).expect("ascii only")
    }

    #[test]
    fn int_to_fixed_ascii_formatting() {
        assert_eq!(emit(0, 2), "0");
        assert_eq!(emit(1250, 2), "12.5");
        assert_eq!(emit(75, 2), ".75");
        assert_eq!(emit(-50, 2), "-.5");
        assert_eq!(emit(1000, 2), "10");
        assert_eq!(emit(1005, 2), "10.05");
        assert_eq!(emit(-1250, 3), "-1.25");
        assert_eq!(emit(1, 3), ".001");
        assert_eq!(emit(10000, 3), "10");
    }

    #[test]
    fn token_separators_follow_svg_grammar() {
        let mut out = IntToFixedASCII::with_capacity(64, Lattice::new(2));
        out.push_command(b'M');
        out.push_coord(1000);
        out.push_coord(-2000);
        out.push_coord(3000);
        let text = String::from_utf8(out.into_bytes()).expect("ascii only");
        assert_eq!(text, "M10-20 30");
    }

    #[test]
    fn measured_width_matches_emitted_width() {
        let lattice = Lattice::new(crate::lattice::DEFAULT_SCALE);
        for value in [0_i64, 7, -7, 1250, -1250, 1005, 1000, 99_999] {
            assert_eq!(number_len(value, lattice), emit(value, 2).len());
        }
    }
}
