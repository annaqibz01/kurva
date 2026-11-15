use super::*;

#[test]
fn solid_color_quantizes_to_single_entry() {
    let mut buf = Vec::with_capacity(16 * 16 * 3);
    for _ in 0..(16 * 16) {
        buf.extend_from_slice(&[42, 137, 210]);
    }
    let raster = quantize_rgb(&buf, 16, 16, 16).expect("solid quantization should succeed");
    assert_eq!(raster.palette.len(), 1);
    let p = raster.palette[0];
    assert!(p.r.abs_diff(42) <= 1);
    assert!(p.g.abs_diff(137) <= 1);
    assert!(p.b.abs_diff(210) <= 1);
    assert!(raster.indices.iter().all(|&idx| idx == 0));
}

#[test]
fn checkerboard_alternates_two_entries() {
    let mut buf = Vec::with_capacity(8 * 8 * 3);
    for y in 0..8 {
        for x in 0..8 {
            if (x + y) % 2 == 0 {
                buf.extend_from_slice(&[10, 10, 10]);
            } else {
                buf.extend_from_slice(&[245, 245, 245]);
            }
        }
    }
    let raster = quantize_rgb(&buf, 8, 8, 8).expect("checkerboard should succeed");
    assert!(raster.palette.len() >= 2);
    let top_left = raster.indices[0];
    let neighbor = raster.indices[1];
    assert_ne!(top_left, neighbor);
    for y in 0..8 {
        for x in 0..8 {
            let expected = if (x + y) % 2 == 0 { top_left } else { neighbor };
            assert_eq!(raster.indices[y * 8 + x], expected);
        }
    }
}

#[test]
fn anti_aliased_edge_unmixes_without_gray_halo() {
    let width = 12;
    let height = 6;
    let mut buf = Vec::with_capacity(width * height * 3);
    for _y in 0..height {
        for x in 0..width {
            if x < 5 {
                buf.extend_from_slice(&[0, 0, 0]);
            } else if x == 5 {
                buf.extend_from_slice(&[128, 128, 128]);
            } else {
                buf.extend_from_slice(&[255, 255, 255]);
            }
        }
    }
    let raster = quantize_rgb(&buf, width, height, 8).expect("AA edge should succeed");
    assert_eq!(raster.palette.len(), 2);
    let left_idx = raster.indices[0];
    let right_idx = raster.indices[width - 1];
    assert_ne!(left_idx, right_idx);
    for y in 0..height {
        let mid = raster.indices[y * width + 5];
        assert!(mid == left_idx || mid == right_idx);
    }
}

#[test]
fn tiny_accent_dot_survives_on_large_background() {
    let width = 24;
    let height = 24;
    let mut buf = vec![0_u8; width * height * 3];
    for px in buf.as_chunks_mut::<3>().0 {
        px.copy_from_slice(&[20, 40, 180]);
    }
    for dy in 11..=12 {
        for dx in 11..=12 {
            let base = (dy * width + dx) * 3;
            buf[base] = 240;
            buf[base + 1] = 220;
            buf[base + 2] = 30;
        }
    }
    let raster = quantize_rgb(&buf, width, height, 4).expect("dot survival should succeed");
    let bg_idx = raster.indices[0];
    let dot_idx = raster.indices[11 * width + 11];
    assert_ne!(bg_idx, dot_idx);
    let dot_rgb = raster.palette[usize::from(dot_idx)];
    assert!(dot_rgb.r > 200 && dot_rgb.b < 80);
}

#[test]
fn smooth_gradient_triggers_adaptive_fallback() {
    let width = 32;
    let height = 16;
    let mut buf = Vec::with_capacity(width * height * 3);
    for _y in 0..height {
        for x in 0..width {
            let v = ((x * 255) / (width - 1)) as u8;
            buf.extend_from_slice(&[v, 255 - v, 128]);
        }
    }
    let raster = quantize_rgb(&buf, width, height, 6).expect("smooth gradient should succeed");
    assert!(!raster.palette.is_empty());
    assert!(raster.palette.len() <= 6);
}

#[test]
fn thin_white_stroke_survives_in_gray_ribbon_and_aa_border_has_no_gray_halo() {
    let width = 24;
    let height = 16;
    let mut buf = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let rgb = if x < 6 {
                [0, 0, 0]
            } else if x == 6 {
                [128, 128, 128]
            } else if x < 10 || (x == 17 && (2..14).contains(&y)) {
                [255, 255, 255]
            } else {
                [120, 120, 120]
            };
            buf.extend_from_slice(&rgb);
        }
    }

    let raster = quantize_rgb(&buf, width, height, 4).expect("quantization should succeed");
    let black_idx = raster.indices[0];
    let white_idx = raster.indices[8];
    let gray_idx = raster.indices[13];
    assert_ne!(black_idx, white_idx);
    assert_ne!(white_idx, gray_idx);
    assert_ne!(black_idx, gray_idx);

    for y in 0..height {
        let edge_idx = raster.indices[y * width + 6];
        assert!(
            edge_idx == black_idx || edge_idx == white_idx,
            "black/white AA edge at y={y} spawned gray halo"
        );
    }
    for y in 3..13 {
        assert_eq!(
            raster.indices[y * width + 17],
            white_idx,
            "1px white stroke at y={y} was erased"
        );
    }
}

#[test]
fn ringing_and_invalid_inputs_handled() {
    let mut ring = Vec::with_capacity(12 * 6 * 3);
    for _y in 0..6 {
        for x in 0..12 {
            let v = match x {
                0..=3 => 0,
                4 => 15,
                5 => 240,
                _ => 255,
            };
            ring.extend_from_slice(&[v, v, v]);
        }
    }
    let raster = quantize_rgb(&ring, 12, 6, 4).expect("ringing buffer should succeed");
    assert!(raster.palette.len() <= 4);

    let valid = [0_u8; 12];
    assert!(quantize_rgb(&valid, 2, 2, 0).is_err());
    assert!(quantize_rgb(&valid, 2, 2, 257).is_err());
    assert!(matches!(
        quantize_rgb(&valid[..11], 2, 2, 4),
        Err(InvalidBufferSize)
    ));
    assert!(matches!(
        quantize_rgb(&valid, usize::MAX, 2, 4),
        Err(InvalidBufferSize)
    ));
    assert!(matches!(quantize_rgb(&[], 0, 0, 4), Err(InvalidBufferSize)));
}

#[test]
fn jpeg_ringing_is_suppressed_while_high_contrast_stroke_is_preserved() {
    let width = 24;
    let height = 16;
    let mut buf = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let rgb = if x < 5 {
                [0, 0, 0]
            } else if x == 7 && (2..14).contains(&y) {
                [215, 215, 215]
            } else if x < 14 {
                [245, 245, 245]
            } else if x == 19 && (2..14).contains(&y) {
                [250, 250, 250]
            } else {
                [85, 85, 85]
            };
            buf.extend_from_slice(&rgb);
        }
    }

    let raster = quantize_rgb(&buf, width, height, 4).expect("quantization should succeed");
    let white_idx = raster.indices[10];
    let gray_idx = raster.indices[16];
    assert_ne!(white_idx, gray_idx);

    for y in 3..13 {
        assert_eq!(
            raster.indices[y * width + 7],
            white_idx,
            "low-contrast JPEG ringing at y={y} spawned a stray gray region"
        );
        let stroke_idx = raster.indices[y * width + 19];
        assert_ne!(
            stroke_idx, gray_idx,
            "high-contrast 1px white stroke at y={y} was erased into ribbon gray"
        );
        assert!(
            raster.palette[stroke_idx as usize].r >= 245,
            "high-contrast 1px stroke at y={y} lost white luminance: {:?}",
            raster.palette[stroke_idx as usize]
        );
    }
    assert!(
        raster
            .palette
            .iter()
            .all(|c| !(180..=230).contains(&c.r)),
        "stray JPEG ringing gray entry spawned in palette: {:?}",
        raster.palette
    );
}
