//! Signed shoelace measures and shell / hole classification.

use crate::arena::{Arena, FaceId};
use crate::error::ContourError;

/// Shoelace area of a closed cycle in image coordinates (`x` right, `y` down).
///
/// A positive value marks a cycle that winds around its own region, so the
/// region sits on the right of every traversal: an outer shell. A negative
/// value marks a cycle whose region lies outside it: an inner hole boundary.
pub fn signed_area(points: &[(f64, f64)]) -> f64 {
    let count = points.len();
    if count < 3 {
        return 0.0;
    }
    let mut twice_area = 0.0;
    for index in 0..count {
        let (x0, y0) = points[index];
        let (x1, y1) = points[(index + 1) % count];
        twice_area += x0 * y1 - x1 * y0;
    }
    twice_area * 0.5
}

/// Collect the origin coordinates of one face cycle, in traversal order.
///
/// Pinch vertices are revisited as often as the cycle passes through them; the
/// loop stays a single closed contour, which is exactly what the SVG `nonzero`
/// and `evenodd` fill rules consume.
pub fn face_cycle_points(arena: &Arena, face_id: FaceId) -> Result<Vec<(f64, f64)>, ContourError> {
    let start = arena.face(face_id)?.halfedge;
    if crate::arena::is_null(start) {
        return Err(ContourError::Topology(format!(
            "face {face_id} has no seed half-edge"
        )));
    }
    let limit = arena.halfedges.len();
    let mut points = Vec::new();
    let mut cursor = start;
    loop {
        let edge = arena.halfedge(cursor)?;
        let origin = arena.vertex(edge.origin)?;
        points.push((origin.x, origin.y));
        cursor = edge.next;
        if cursor == start {
            break;
        }
        if points.len() > limit {
            return Err(ContourError::Topology(format!(
                "face {face_id} cycle never closed"
            )));
        }
    }
    Ok(points)
}

/// Measure every face and derive its `is_hole` flag from the shoelace sign.
pub fn classify_faces(arena: &mut Arena) -> Result<(), ContourError> {
    let mut measured: Vec<(f64, bool)> = Vec::with_capacity(arena.faces.len());
    for face_id in 0..arena.faces.len() as u32 {
        let area = signed_area(&face_cycle_points(arena, face_id)?);
        let exterior = arena.face(face_id)?.is_exterior;
        measured.push((area, !exterior && area < 0.0));
    }
    for (face_id, (area, is_hole)) in measured.into_iter().enumerate() {
        let face = arena.face_mut(face_id as u32)?;
        face.area = area;
        face.is_hole = is_hole;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::signed_area;

    #[test]
    fn shoelace_sign_follows_image_orientation() {
        let shell = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
        assert!((signed_area(&shell) - 4.0).abs() < 1e-12);
        let mut hole = shell;
        hole.reverse();
        assert!((signed_area(&hole) + 4.0).abs() < 1e-12);
        assert!((signed_area(&shell[..2])).abs() < 1e-12);
    }
}
