//! sRGB to Oklab conversion and the perceptual `Delta E_OK` metric.

use kurva_types::ColorRgb;

/// Decodes one eight-bit sRGB channel into linear light.
fn linearise(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Encodes linear light back into eight-bit sRGB.
fn encode(linear: f32) -> u8 {
    let value = linear.clamp(0.0, 1.0);
    let srgb = if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Converts an sRGB triple into Oklab `(L, a, b)` through LMS cone responses.
pub fn from_srgb(rgb: ColorRgb) -> [f32; 3] {
    let r = linearise(rgb.r);
    let g = linearise(rgb.g);
    let b = linearise(rgb.b);
    let l = 0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_84 * g + 0.629_978_7 * b;
    let l = l.cbrt();
    let m = m.cbrt();
    let s = s.cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Converts Oklab `(L, a, b)` back into clamped sRGB.
pub fn to_srgb(lab: [f32; 3]) -> ColorRgb {
    let [l, a, b] = lab;
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    let red = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let green = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let blue = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    ColorRgb::new(encode(red), encode(green), encode(blue))
}

/// Euclidean metric in Oklab, `Delta E_OK`.
pub fn delta_e_ok(left: [f32; 3], right: [f32; 3]) -> f32 {
    let dl = left[0] - right[0];
    let da = left[1] - right[1];
    let db = left[2] - right[2];
    (dl * dl + da * da + db * db).sqrt()
}

/// Decodes an interleaved RGB8 buffer into an Oklab raster.
pub fn raster(bytes: &[u8]) -> Vec<[f32; 3]> {
    (0..bytes.len() / 3)
        .map(|pixel| {
            let base = pixel * 3;
            from_srgb(ColorRgb::new(bytes[base], bytes[base + 1], bytes[base + 2]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklab_roundtrip_stays_within_one_lsb() {
        for red in (0..=255).step_by(17) {
            for green in (0..=255).step_by(51) {
                for blue in (0..=255).step_by(17) {
                    let original = ColorRgb::new(red as u8, green as u8, blue as u8);
                    let restored = to_srgb(from_srgb(original));
                    assert!(restored.r.abs_diff(original.r) <= 1);
                    assert!(restored.g.abs_diff(original.g) <= 1);
                    assert!(restored.b.abs_diff(original.b) <= 1);
                }
            }
        }
    }

    #[test]
    fn delta_e_ok_is_zero_for_identical_colours() {
        let lab = from_srgb(ColorRgb::new(12, 34, 56));
        assert!(delta_e_ok(lab, lab) < 1e-6);
        assert!((delta_e_ok([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]) - 1.0).abs() < 1e-6);
    }
}
