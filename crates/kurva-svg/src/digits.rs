//! Decimal digit primitives shared by the emitter and the cost model.

use crate::lattice::Lattice;

/// Appends `value` in base ten, most significant digit first.
pub(crate) fn write_unsigned(buffer: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0_u8; 20];
    let mut length = 0;
    loop {
        digits[length] = b'0' + (value % 10) as u8;
        value /= 10;
        length += 1;
        if value == 0 {
            break;
        }
    }
    for digit in digits[..length].iter().rev() {
        buffer.push(*digit);
    }
}

/// Decimal width of `value`.
pub(crate) fn unsigned_len(value: u64) -> usize {
    let mut length = 1;
    let mut rest = value;
    while rest >= 10 {
        rest /= 10;
        length += 1;
    }
    length
}

/// Byte width of one fixed-point coordinate once emitted.
pub(crate) fn number_len(value: i64, lattice: Lattice) -> usize {
    let magnitude = value.unsigned_abs();
    let divisor = lattice.divisor() as u64;
    let quotient = magnitude / divisor;
    let remainder = magnitude % divisor;
    let mut length = usize::from(value < 0);
    if quotient > 0 {
        length += unsigned_len(quotient);
    } else if remainder == 0 {
        length += 1;
    }
    if remainder == 0 {
        return length;
    }
    length += 1;
    let mut rest = remainder;
    let mut digits = lattice.scale();
    while rest.is_multiple_of(10) {
        rest /= 10;
        digits = digits.saturating_sub(1);
    }
    length + digits as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lattice::DEFAULT_SCALE;

    #[test]
    fn decimal_widths_are_exact() {
        assert_eq!(unsigned_len(0), 1);
        assert_eq!(unsigned_len(9), 1);
        assert_eq!(unsigned_len(10), 2);
        assert_eq!(unsigned_len(99_999), 5);

        let lattice = Lattice::new(DEFAULT_SCALE);
        assert_eq!(number_len(0, lattice), 1);
        assert_eq!(number_len(-50, lattice), 3);
        assert_eq!(number_len(75, lattice), 3);
        assert_eq!(number_len(1250, lattice), 4);
        assert_eq!(number_len(-1250, lattice), 5);
    }
}
