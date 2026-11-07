//! Half-edge planar map assembly: rotation permutation, face permutation, cycles.

use crate::arena::{is_null, Arena, HalfEdgeId};
use crate::error::ContourError;
use crate::scanline::EXTERIOR_COLOR;

/// Smallest star size handled by insertion sort.
const INSERTION_SORT_LIMIT: usize = 8;

fn polar_angle(arena: &Arena, halfedge: HalfEdgeId) -> Result<f64, ContourError> {
    let edge = arena.halfedge(halfedge)?;
    let origin = arena.vertex(edge.origin)?;
    let target = arena.vertex(edge.target)?;
    Ok((target.y - origin.y).atan2(target.x - origin.x))
}

/// Sort the outgoing star of a vertex into ascending image-space polar order.
fn sort_star(arena: &Arena, star: &mut [u32]) -> Result<(), ContourError> {
    if star.len() < 2 {
        return Ok(());
    }
    if star.len() <= INSERTION_SORT_LIMIT {
        for index in 1..star.len() {
            let mut cursor = index;
            while cursor > 0 {
                let left = polar_angle(arena, star[cursor - 1])?;
                let right = polar_angle(arena, star[cursor])?;
                if right >= left {
                    break;
                }
                star.swap(cursor - 1, cursor);
                cursor -= 1;
            }
        }
        return Ok(());
    }
    let mut keyed: Vec<(f64, u32)> = Vec::with_capacity(star.len());
    for &halfedge in star.iter() {
        keyed.push((polar_angle(arena, halfedge)?, halfedge));
    }
    keyed.sort_by(|left, right| left.0.total_cmp(&right.0));
    for (slot, entry) in star.iter_mut().zip(keyed) {
        *slot = entry.1;
    }
    Ok(())
}

/// Build the rotation system and link `next = rho(alpha(h))` on every half-edge.
pub fn link_rotation(arena: &mut Arena) -> Result<(), ContourError> {
    let halfedge_count = arena.halfedges.len();
    if halfedge_count == 0 {
        return Ok(());
    }

    let mut stars: Vec<Vec<u32>> = vec![Vec::new(); arena.vertices.len()];
    for (index, edge) in arena.halfedges.iter().enumerate() {
        let star = stars.get_mut(edge.origin as usize).ok_or_else(|| {
            ContourError::Topology(format!("half-edge {index} starts outside the vertex arena"))
        })?;
        star.push(index as u32);
    }

    let mut rho = vec![crate::arena::NULL_INDEX; halfedge_count];
    for (vertex_id, star) in stars.iter_mut().enumerate() {
        if star.is_empty() {
            continue;
        }
        sort_star(arena, star)?;
        let degree = star.len();
        let first = star.first().copied().unwrap_or(crate::arena::NULL_INDEX);
        arena.vertex_mut(vertex_id as u32)?.outgoing_halfedge = first;
        // A face cycle keeps its owning region on the right, so the cyclic
        // successor of `rho` is the next edge clockwise from the incoming one:
        // the angular predecessor inside the ascending star.
        for position in 0..degree {
            let successor = star[(position + degree - 1) % degree];
            if let Some(entry) = rho.get_mut(star[position] as usize) {
                *entry = successor;
            }
        }
    }

    for index in 0..halfedge_count {
        let twin = arena.halfedge(index as u32)?.twin;
        if is_null(twin) {
            return Err(ContourError::Topology(format!(
                "half-edge {index} has no twin"
            )));
        }
        let successor = rho.get(twin as usize).copied().unwrap_or_default();
        if is_null(successor) {
            return Err(ContourError::Topology(format!(
                "rotation of half-edge {index} is undefined"
            )));
        }
        arena.halfedge_mut(index as u32)?.next = successor;
    }
    Ok(())
}

/// Trace every cycle of the face permutation and register it as a 2-cell.
///
/// A face cycle keeps its owning region on the right in image coordinates, so
/// the phase of that region is read from the left flank of the twin half-edge.
pub fn extract_faces(arena: &mut Arena) -> Result<(), ContourError> {
    let halfedge_count = arena.halfedges.len();
    if halfedge_count == 0 {
        return Ok(());
    }
    let mut visited = vec![false; halfedge_count];
    let mut members: Vec<u32> = Vec::new();

    for start in 0..halfedge_count as u32 {
        if visited.get(start as usize).copied().unwrap_or(true) {
            continue;
        }
        members.clear();
        let mut cursor = start;
        loop {
            let slot = visited.get_mut(cursor as usize).ok_or_else(|| {
                ContourError::Topology(format!("cycle reached half-edge {cursor}"))
            })?;
            if *slot {
                return Err(ContourError::Topology(format!(
                    "face cycle re-entered half-edge {cursor}"
                )));
            }
            *slot = true;
            members.push(cursor);
            if members.len() > halfedge_count {
                return Err(ContourError::Topology(
                    "face cycle never closed".to_string(),
                ));
            }
            cursor = arena.halfedge(cursor)?.next;
            if is_null(cursor) {
                return Err(ContourError::Topology(
                    "face successor is unlinked".to_string(),
                ));
            }
            if cursor == start {
                break;
            }
        }
        let twin = arena.halfedge(start)?.twin;
        let color = arena.halfedge(twin)?.color_left;
        let face_id = arena.add_face(start, color)?;
        arena.face_mut(face_id)?.is_exterior = color == EXTERIOR_COLOR;
        for &member in members.iter() {
            arena.halfedge_mut(member)?.face = face_id;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{extract_faces, link_rotation};
    use crate::arena::{is_null, Arena};
    use crate::error::ContourError;
    use crate::scanline::emit_boundary;
    use kurva_types::{ColorRgb, IndexedRaster};

    fn uniform(width: usize, height: usize) -> IndexedRaster {
        let indices = vec![0u8; width * height];
        match IndexedRaster::new(width, height, vec![ColorRgb::new(0, 0, 0)], indices) {
            Some(raster) => raster,
            None => panic!("uniform raster was rejected"),
        }
    }

    fn assembled(raster: &IndexedRaster) -> Result<Arena, ContourError> {
        let mut arena = Arena::default();
        emit_boundary(&mut arena, raster)?;
        link_rotation(&mut arena)?;
        extract_faces(&mut arena)?;
        Ok(arena)
    }

    fn cycle_length(arena: &Arena, start: u32) -> Result<usize, ContourError> {
        let mut length = 0;
        let mut cursor = start;
        loop {
            length += 1;
            cursor = arena.halfedge(cursor)?.next;
            if cursor == start {
                return Ok(length);
            }
            if length > arena.halfedges.len() {
                return Err(ContourError::Topology("cycle did not close".to_string()));
            }
        }
    }

    #[test]
    fn topology_invariants() -> Result<(), ContourError> {
        let raster = uniform(4, 4);
        let arena = assembled(&raster)?;

        for (index, edge) in arena.halfedges.iter().enumerate() {
            let twin = arena.halfedge(edge.twin)?;
            assert_eq!(twin.twin as usize, index);
            assert_eq!(twin.origin, edge.target);
            assert_eq!(twin.target, edge.origin);
            assert!(!is_null(edge.face));
        }

        let mut covered = 0;
        for face in &arena.faces {
            covered += cycle_length(&arena, face.halfedge)?;
        }
        assert_eq!(covered, arena.halfedges.len());

        let vertices = arena
            .vertices
            .iter()
            .filter(|vertex| !is_null(vertex.outgoing_halfedge))
            .count();
        let edges = arena.halfedges.len() / 2;
        let faces = arena.faces.len();
        assert_eq!(vertices as i64 - edges as i64 + faces as i64, 2);
        Ok(())
    }
}
