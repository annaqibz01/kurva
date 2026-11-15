//! Corner-preserving bounded tangential Laplacian smoothing for 2-material chains.

use crate::arena::{is_null, Arena};
use crate::error::ContourError;
use crate::rgb_subpixel::VertexStar;

/// Maximum cumulative displacement from initial/RGB-shifted anchor after smoothing.
const MAX_SMOOTH_RADIUS: f64 = 0.95;
/// Number of tangential Laplacian smoothing passes on unlocked 2-material chains.
const SMOOTH_PASSES: usize = 6;
/// Cosine threshold for a geometric corner anchor (`turning angle >= 50 deg`, `cos <= 0.6428`).
const CORNER_COS_THRESHOLD: f64 = 0.6428;

#[inline]
fn is_turning_corner(p0: (f64, f64), v: (f64, f64), p1: (f64, f64)) -> bool {
    let in_x = v.0 - p0.0;
    let in_y = v.1 - p0.1;
    let out_x = p1.0 - v.0;
    let out_y = p1.1 - v.1;
    let in_sq = in_x * in_x + in_y * in_y;
    let out_sq = out_x * out_x + out_y * out_y;
    if in_sq <= 1.0e-12 || out_sq <= 1.0e-12 {
        return true;
    }
    let cos_theta = (in_x * out_x + in_y * out_y) / (in_sq * out_sq).sqrt();
    cos_theta <= CORNER_COS_THRESHOLD
}

pub(crate) fn smooth_chains(
    arena: &mut Arena,
    stars: &[VertexStar],
    anchors: &[(f64, f64)],
    max_x: f64,
    max_y: f64,
) -> Result<(), ContourError> {
    let mut corner_locked = vec![false; arena.vertices.len()];
    for (v_idx, (vertex, star)) in arena.vertices.iter().zip(stars.iter()).enumerate() {
        if is_null(vertex.outgoing_halfedge) || star.is_locked() || star.outgoing.len() != 2 {
            continue;
        }
        if vertex.x <= 0.0 || vertex.x >= max_x || vertex.y <= 0.0 || vertex.y >= max_y {
            continue;
        }
        let n0 = arena.halfedge(star.outgoing[0])?.target as usize;
        let n1 = arena.halfedge(star.outgoing[1])?.target as usize;
        corner_locked[v_idx] = is_turning_corner(anchors[n0], anchors[v_idx], anchors[n1]);
    }

    let mut updates: Vec<(u32, f64, f64)> = Vec::new();
    for _ in 0..SMOOTH_PASSES {
        updates.clear();
        for (v_idx, (vertex, star)) in arena.vertices.iter().zip(stars.iter()).enumerate() {
            if corner_locked[v_idx]
                || is_null(vertex.outgoing_halfedge)
                || star.is_locked()
                || star.outgoing.len() != 2
            {
                continue;
            }
            if vertex.x <= 0.0 || vertex.x >= max_x || vertex.y <= 0.0 || vertex.y >= max_y {
                continue;
            }
            let n0 = arena.halfedge(star.outgoing[0])?.target;
            let n1 = arena.halfedge(star.outgoing[1])?.target;
            let p0 = arena.vertex(n0)?;
            let p1 = arena.vertex(n1)?;
            let sx = 0.5 * vertex.x + 0.25 * (p0.x + p1.x);
            let sy = 0.5 * vertex.y + 0.25 * (p0.y + p1.y);
            let (ax, ay) = anchors[v_idx];
            let dx = sx - ax;
            let dy = sy - ay;
            let dist = dx.hypot(dy);
            let (cx, cy) = if dist > MAX_SMOOTH_RADIUS {
                let scale = MAX_SMOOTH_RADIUS / dist;
                (ax + dx * scale, ay + dy * scale)
            } else {
                (sx, sy)
            };
            let handle = u32::try_from(v_idx).map_err(|_| {
                ContourError::Topology("vertex arena exhausted 32-bit handles".into())
            })?;
            updates.push((handle, cx, cy));
        }
        for &(handle, cx, cy) in &updates {
            let vertex = arena.vertex_mut(handle)?;
            vertex.x = cx;
            vertex.y = cy;
        }
    }
    Ok(())
}
