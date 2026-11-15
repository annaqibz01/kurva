use crate::{quantize_rgb, refine};

#[test]
fn adaptive_palette_merges_near_duplicates_and_preserves_multi_hue_colors() {
    let w = 24;
    let h = 16;
    let mut buf = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let rgb = match (x / 6, y / 8) {
                (0, 0) => [15, 15, 20],
                (1, 0) => [18, 18, 24],
                (2, 0) => [246, 246, 246],
                (3, 0) => [254, 254, 254],
                (0, 1) => [120, 120, 120],
                (1, 1) => [126, 126, 126],
                (2, 1) => [210, 40, 40],
                _ => [30, 160, 70],
            };
            buf.extend_from_slice(&rgb);
        }
    }

    let raster = quantize_rgb(&buf, w, h, 16).expect("quantize_rgb should succeed");
    assert!(
        raster.palette.len() <= 5,
        "near-duplicate shades should collapse under max_colors=16, got {:?}",
        raster.palette
    );
    assert!(raster.palette.iter().any(|c| c.r > 180 && c.g < 80));
    assert!(raster.palette.iter().any(|c| c.g > 130 && c.r < 70));
}

#[test]
fn connected_component_despeckle_absorbs_tiny_noise_and_keeps_thin_strokes() {
    let w = 8;
    let h = 8;
    let palette_lab = [[0.9, 0.0, 0.0], [0.2, 0.0, 0.0]];
    let mut lab = vec![palette_lab[0]; w * h];
    let mut indices = vec![0_u8; w * h];
    let mut thin_strokes = vec![false; w * h];

    // Isolated 2-px non-stroke speckle at (1,1) and (2,1).
    for idx in [w + 1, w + 2] {
        lab[idx] = palette_lab[1];
        indices[idx] = 1;
    }

    // Isolated 2-px thin-stroke feature at (5,5) and (5,6) flagged in thin_strokes.
    for idx in [5 * w + 5, 6 * w + 5] {
        lab[idx] = palette_lab[1];
        indices[idx] = 1;
        thin_strokes[idx] = true;
    }

    refine::icm_cleanup(&lab, &palette_lab, &thin_strokes, &mut indices, w, h);

    assert_eq!(indices[w + 1], 0, "1-2 px non-stroke speckle should be absorbed");
    assert_eq!(indices[w + 2], 0, "1-2 px non-stroke speckle should be absorbed");
    assert_eq!(indices[5 * w + 5], 1, "thin_strokes component must survive despeckle");
    assert_eq!(indices[6 * w + 5], 1, "thin_strokes component must survive despeckle");
}
