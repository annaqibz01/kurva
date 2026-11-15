use super::super::{extract_contours, extract_contours_with_rgb, ContourError};
use super::build;
use kurva_types::{ColorRgb, IndexedRaster};

#[test]
fn subpixel_coordinates_retain_fractional_precision() -> Result<(), ContourError> {
    let mut indices = vec![0u8; 16];
    indices[5] = 1;
    indices[6] = 1;
    indices[9] = 1;
    indices[10] = 1;
    let contours = extract_contours(&build(4, 4, indices))?;
    let has_fractional = contours.iter().any(|c| {
        c.points
            .iter()
            .any(|p| (p.x - p.x.round()).abs() > 1.0e-3 || (p.y - p.y.round()).abs() > 1.0e-3)
    });
    assert!(has_fractional, "expected subpixel fractional coordinates");
    Ok(())
}

#[test]
fn rgb_guided_shift_and_watertight_triple_junction() -> Result<(), ContourError> {
    let palette = vec![
        ColorRgb::new(0, 0, 0),
        ColorRgb::new(255, 255, 255),
        ColorRgb::new(255, 0, 0),
    ];
    let mut indices = vec![0u8; 16];
    for y in 0..4 {
        for x in 0..4 {
            indices[y * 4 + x] = if y >= 2 {
                2
            } else if x < 2 {
                0
            } else {
                1
            };
        }
    }
    let raster = IndexedRaster::new(4, 4, palette.clone(), indices)
        .ok_or_else(|| ContourError::Topology("invalid test raster".into()))?;

    let mut rgb = vec![0u8; 16 * 3];
    for y in 0..4 {
        for x in 0..4 {
            let color = if y >= 2 {
                palette[2]
            } else if x < 2 {
                palette[0]
            } else if x == 2 && y == 1 {
                ColorRgb::new(64, 64, 64)
            } else {
                palette[1]
            };
            let base = (y * 4 + x) * 3;
            rgb[base] = color.r;
            rgb[base + 1] = color.g;
            rgb[base + 2] = color.b;
        }
    }

    let baseline = extract_contours(&raster)?;
    let shifted = extract_contours_with_rgb(&raster, &rgb)?;
    assert_eq!(baseline.len(), 3);
    assert_eq!(shifted.len(), 3);

    let black_shifted = shifted
        .iter()
        .find(|c| c.color_id == 0)
        .ok_or_else(|| ContourError::Topology("missing black contour".into()))?;
    let white_shifted = shifted
        .iter()
        .find(|c| c.color_id == 1)
        .ok_or_else(|| ContourError::Topology("missing white contour".into()))?;
    let shifted_pt = black_shifted
        .points
        .iter()
        .find(|p| (p.x - 2.0).abs() < 0.5 && (p.y - 1.0).abs() < 0.25)
        .ok_or_else(|| ContourError::Topology("missing (2,1) vertex".into()))?;
    assert!(
        shifted_pt.x > 2.04,
        "expected black boundary to shift right toward anti-aliased gray pixel, got {}",
        shifted_pt.x
    );
    assert!(
        white_shifted
            .points
            .iter()
            .any(|p| (p.x - shifted_pt.x).abs() < 1.0e-12 && (p.y - shifted_pt.y).abs() < 1.0e-12),
        "adjacent white contour must share exact shifted vertex"
    );

    let mut junction_matches = 0;
    for c in &shifted {
        if c.points
            .iter()
            .any(|p| (p.x - 2.0).abs() < 0.3 && (p.y - 2.0).abs() < 0.3)
        {
            junction_matches += 1;
        }
    }
    assert_eq!(junction_matches, 3);
    let j0 = shifted[0]
        .points
        .iter()
        .find(|p| (p.x - 2.0).abs() < 0.3 && (p.y - 2.0).abs() < 0.3)
        .copied();
    let j1 = shifted[1]
        .points
        .iter()
        .find(|p| (p.x - 2.0).abs() < 0.3 && (p.y - 2.0).abs() < 0.3)
        .copied();
    let j2 = shifted[2]
        .points
        .iter()
        .find(|p| (p.x - 2.0).abs() < 0.3 && (p.y - 2.0).abs() < 0.3)
        .copied();
    assert_eq!(j0, j1);
    assert_eq!(j1, j2);
    Ok(())
}

#[test]
fn rgb_length_mismatch_returns_inconsistent_raster() {
    let raster = build(2, 2, vec![0; 4]);
    let bad_rgb = vec![0u8; 4 * 3 - 1];
    assert!(matches!(
        extract_contours_with_rgb(&raster, &bad_rgb),
        Err(ContourError::InconsistentRaster {
            expected: 12,
            actual: 11
        })
    ));
}

#[test]
fn sharp_right_angle_corner_is_preserved_while_diagonal_smooths() -> Result<(), ContourError> {
    let w = 12;
    let h = 12;
    let palette = vec![ColorRgb::new(0, 0, 0), ColorRgb::new(255, 255, 255)];
    let mut indices = vec![0u8; w * h];
    for y in 2..=9 {
        for x in 2..=9 {
            // Sharp 90-degree corner at (2, 2); diagonal staircase along x + y = 14.
            if x + y <= 14 {
                indices[y * w + x] = 1;
            }
        }
    }
    let mut rgb = vec![0u8; w * h * 3];
    for (idx, &cid) in indices.iter().enumerate() {
        let c = palette[cid as usize];
        rgb[idx * 3] = c.r;
        rgb[idx * 3 + 1] = c.g;
        rgb[idx * 3 + 2] = c.b;
    }
    let raster = IndexedRaster::new(w, h, palette, indices)
        .ok_or_else(|| ContourError::Topology("invalid test raster".into()))?;

    let baseline = extract_contours(&raster)?;
    let smoothed = extract_contours_with_rgb(&raster, &rgb)?;
    let base_fg = baseline
        .iter()
        .find(|c| c.color_id == 1)
        .ok_or_else(|| ContourError::Topology("missing baseline fg contour".into()))?;
    let smooth_fg = smoothed
        .iter()
        .find(|c| c.color_id == 1)
        .ok_or_else(|| ContourError::Topology("missing smoothed fg contour".into()))?;

    let base_corner = base_fg
        .points
        .iter()
        .find(|p| (p.x - 2.0).abs() < 0.4 && (p.y - 2.0).abs() < 0.4)
        .ok_or_else(|| ContourError::Topology("missing baseline corner".into()))?;
    let smooth_corner = smooth_fg
        .points
        .iter()
        .find(|p| (p.x - 2.0).abs() < 0.4 && (p.y - 2.0).abs() < 0.4)
        .ok_or_else(|| ContourError::Topology("missing smoothed corner".into()))?;

    assert!(
        (smooth_corner.x - base_corner.x).abs() < 1.0e-9
            && (smooth_corner.y - base_corner.y).abs() < 1.0e-9,
        "90-degree corner moved from ({}, {}) to ({}, {})",
        base_corner.x,
        base_corner.y,
        smooth_corner.x,
        smooth_corner.y
    );

    let diag_moved = base_fg
        .points
        .iter()
        .zip(smooth_fg.points.iter())
        .any(|(b, s)| (b.x - s.x).hypot(b.y - s.y) > 1.0e-3);
    assert!(diag_moved, "expected diagonal staircase vertices to smooth");
    Ok(())
}
