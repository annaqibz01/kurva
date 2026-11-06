//! CIELAB color space conversions relative to the D65 standard illuminant.

use kurva_types::ColorRgb;

/// D65 reference white used by the sRGB specification.
const WHITE_X: f64 = 0.950_47;
const WHITE_Y: f64 = 1.0;
const WHITE_Z: f64 = 1.088_83;

/// CIE standard constants for the sRGB transfer curve and XYZ mapping.
const EPSILON: f64 = 216.0 / 24_389.0;
const KAPPA: f64 = 24_389.0 / 27.0;
const SLOPE: f64 = 7.787_037_037_037_037;

/// A color expressed in CIE L*a*b* coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabColor {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

impl LabColor {
    /// Builds a Lab color from its three components.
    pub const fn new(l: f64, a: f64, b: f64) -> Self {
        Self { l, a, b }
    }
}

/// Converts a single sRGB channel value into linear light.
fn srgb_to_linear(value: u8) -> f64 {
    let channel = f64::from(value) / 255.0;
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Converts linear light back into an 8-bit sRGB channel with clamping.
fn linear_to_srgb(value: f64) -> u8 {
    let channel = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    let scaled = (channel * 255.0).round();
    scaled.clamp(0.0, 255.0) as u8
}

/// Nonlinear expansion toward the CIELAB domain.
fn lab_forward(ratio: f64) -> f64 {
    if ratio > EPSILON {
        ratio.cbrt()
    } else {
        SLOPE * ratio + 4.0 / 29.0
    }
}

/// Inverse of [`lab_forward`].
fn lab_inverse(value: f64) -> f64 {
    let cubed = value * value * value;
    if cubed > EPSILON {
        cubed
    } else {
        (116.0 * value - 16.0) / KAPPA
    }
}

/// Converts 8-bit sRGB channels into CIELAB coordinates.
pub fn rgb_to_lab(r: u8, g: u8, b: u8) -> LabColor {
    let red = srgb_to_linear(r);
    let green = srgb_to_linear(g);
    let blue = srgb_to_linear(b);

    let x = 0.412_456_4 * red + 0.357_576_1 * green + 0.180_437_5 * blue;
    let y = 0.212_672_9 * red + 0.715_152_2 * green + 0.072_175 * blue;
    let z = 0.019_333_9 * red + 0.119_192 * green + 0.950_304_1 * blue;

    let fx = lab_forward(x / WHITE_X);
    let fy = lab_forward(y / WHITE_Y);
    let fz = lab_forward(z / WHITE_Z);

    LabColor {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// Converts CIELAB coordinates back into 8-bit sRGB channels.
pub fn lab_to_rgb(lab: LabColor) -> ColorRgb {
    let fy = (lab.l + 16.0) / 116.0;
    let fx = fy + lab.a / 500.0;
    let fz = fy - lab.b / 200.0;

    let x = lab_inverse(fx) * WHITE_X;
    let y = lab_inverse(fy) * WHITE_Y;
    let z = lab_inverse(fz) * WHITE_Z;

    let red = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
    let green = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
    let blue = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;

    ColorRgb::new(
        linear_to_srgb(red),
        linear_to_srgb(green),
        linear_to_srgb(blue),
    )
}

/// Squared Euclidean distance in CIELAB space, the CIE76 metric without the root.
pub fn delta_e_squared(c1: LabColor, c2: LabColor) -> f64 {
    let dl = c1.l - c2.l;
    let da = c1.a - c2.a;
    let db = c1.b - c2.b;
    dl * dl + da * da + db * db
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: u8, expected: u8, tolerance: u8) {
        let diff = actual.abs_diff(expected);
        assert!(
            diff <= tolerance,
            "channel {actual} differs from {expected} by {diff}"
        );
    }

    #[test]
    fn black_maps_to_zero_lightness() {
        let lab = rgb_to_lab(0, 0, 0);
        assert!(lab.l.abs() < 1e-9);
        assert!(lab.a.abs() < 1e-9);
        assert!(lab.b.abs() < 1e-9);
    }

    #[test]
    fn white_maps_to_full_lightness() {
        let lab = rgb_to_lab(255, 255, 255);
        assert!((lab.l - 100.0).abs() < 1e-3);
        assert!(lab.a.abs() < 1e-2);
        assert!(lab.b.abs() < 1e-2);
    }

    #[test]
    fn neutral_gray_has_no_chroma() {
        let lab = rgb_to_lab(128, 128, 128);
        assert!(lab.a.abs() < 1e-2);
        assert!(lab.b.abs() < 1e-2);
        assert!((50.0..60.0).contains(&lab.l));
    }

    #[test]
    fn primaries_round_trip_within_one_step() {
        let samples = [
            (0u8, 0u8, 0u8),
            (255, 255, 255),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (17, 34, 51),
            (200, 130, 64),
        ];
        for (r, g, b) in samples {
            let back = lab_to_rgb(rgb_to_lab(r, g, b));
            assert_close(back.r, r, 1);
            assert_close(back.g, g, 1);
            assert_close(back.b, b, 1);
        }
    }

    #[test]
    fn delta_e_is_zero_for_identical_colors() {
        let color = rgb_to_lab(10, 120, 200);
        assert!(delta_e_squared(color, color) < 1e-12);
    }

    #[test]
    fn delta_e_grows_with_separation() {
        let base = rgb_to_lab(0, 0, 0);
        let near = rgb_to_lab(10, 10, 10);
        let far = rgb_to_lab(255, 255, 255);
        assert!(delta_e_squared(base, near) < delta_e_squared(base, far));
    }
}
