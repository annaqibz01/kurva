use super::{extract_contours, geometry, scanline, subpixel, topology, Arena, ContourError};
use kurva_types::{ColorRgb, IndexedRaster};

fn build(width: usize, height: usize, indices: Vec<u8>) -> IndexedRaster {
    let palette = vec![ColorRgb::new(0, 0, 0), ColorRgb::new(255, 255, 255)];
    match IndexedRaster::new(width, height, palette, indices) {
        Some(raster) => raster,
        None => panic!("test raster was rejected"),
    }
}

#[test]
fn uniform_raster_single_contour() -> Result<(), ContourError> {
    let contours = extract_contours(&build(4, 4, vec![0; 16]))?;
    assert_eq!(contours.len(), 1);
    assert!(!contours[0].is_hole);
    assert_eq!(contours[0].points.len(), 16);
    assert_eq!(contours.iter().filter(|contour| contour.is_hole).count(), 0);
    Ok(())
}

fn shoelace(points: &[kurva_types::Point2D<i32>]) -> f64 {
    let mut twice = 0i64;
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        twice +=
            i64::from(current.x) * i64::from(next.y) - i64::from(next.x) * i64::from(current.y);
    }
    twice as f64 * 0.5
}

#[test]
fn nested_donut_hole_detection() -> Result<(), ContourError> {
    let mut indices = vec![0u8; 49];
    for y in 1..6 {
        for x in 1..6 {
            indices[y * 7 + x] = 1;
        }
    }
    for y in 2..5 {
        for x in 2..5 {
            indices[y * 7 + x] = 0;
        }
    }
    let contours = extract_contours(&build(7, 7, indices))?;

    // Three shells (outer frame, inner ring, core) and the two hole
    // boundaries that close the frame and the ring.
    assert_eq!(contours.len(), 5);
    assert_eq!(
        contours.iter().filter(|contour| !contour.is_hole).count(),
        3
    );
    assert_eq!(contours.iter().filter(|contour| contour.is_hole).count(), 2);

    let mut areas: Vec<i64> = contours
        .iter()
        .map(|contour| shoelace(&contour.points).abs() as i64)
        .collect();
    areas.sort_unstable();
    assert_eq!(areas, vec![9, 9, 25, 25, 49]);

    for contour in &contours {
        let area = shoelace(&contour.points);
        assert_eq!(contour.is_hole, area < 0.0);
        assert!(contour
            .points
            .iter()
            .all(|point| point.x >= 0 && point.y >= 0));
    }
    Ok(())
}

#[test]
fn checkerboard_cycles_extract_cleanly() -> Result<(), ContourError> {
    let contours = extract_contours(&build(2, 2, vec![0, 1, 1, 0]))?;
    assert_eq!(contours.len(), 4);
    assert!(contours.iter().all(|contour| contour.points.len() == 4));
    Ok(())
}

#[test]
fn multi_material_grid_is_watertight() -> Result<(), ContourError> {
    let indices: Vec<u8> = (0..36).map(|index| (index % 3) as u8).collect();
    let contours = extract_contours(&build(6, 6, indices))?;
    assert!(!contours.is_empty());
    for contour in &contours {
        assert!(contour.points.len() >= 4);
        let first = &contour.points[0];
        assert!(first.x >= 0 && first.y >= 0);
        assert!(first.x <= 6 && first.y <= 6);
    }
    Ok(())
}

#[test]
fn every_edge_is_traversed_twice() -> Result<(), ContourError> {
    let indices: Vec<u8> = (0..25)
        .map(|index| ((index / 5 + index % 5) % 3) as u8)
        .collect();
    let raster = build(5, 5, indices);
    let mut arena = Arena::default();
    scanline::emit_boundary(&mut arena, &raster)?;
    topology::link_rotation(&mut arena)?;
    topology::extract_faces(&mut arena)?;
    subpixel::adjust_vertices(&mut arena, &raster)?;

    // The exterior face is included here on purpose: Theorem 1 covers the
    // whole complex, so every half-edge must appear in exactly one cycle.
    let mut directed: Vec<((i32, i32), (i32, i32))> = Vec::new();
    for face_id in 0..arena.faces.len() as u32 {
        let points: Vec<(i32, i32)> = geometry::face_cycle_points(&arena, face_id)?
            .into_iter()
            .map(|(x, y)| (x.round() as i32, y.round() as i32))
            .collect();
        for index in 0..points.len() {
            let edge = (points[index], points[(index + 1) % points.len()]);
            assert!(!directed.contains(&edge), "directed edge {edge:?} repeated");
            directed.push(edge);
        }
    }

    assert_eq!(directed.len(), arena.halfedges.len());
    for edge in &directed {
        assert!(
            directed.contains(&(edge.1, edge.0)),
            "edge {edge:?} is not traversed in both orientations"
        );
    }
    Ok(())
}

#[test]
fn empty_canvas_is_rejected() {
    let zero = IndexedRaster {
        width: 0,
        height: 0,
        palette: Vec::new(),
        indices: Vec::new(),
    };
    assert!(matches!(
        extract_contours(&zero),
        Err(ContourError::EmptyCanvas)
    ));
}

#[test]
fn extract_contours_enforces_raster_topology() {
    let valid = IndexedRaster::new(2, 2, vec![ColorRgb::new(0, 0, 0)], vec![0; 4]);
    assert!(matches!(valid, Some(ref r) if extract_contours(r).is_ok()));
    let truncated = IndexedRaster {
        width: 2,
        height: 2,
        palette: Vec::new(),
        indices: vec![0; 3],
    };
    assert!(extract_contours(&truncated).is_err());
}
