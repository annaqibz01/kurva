//! Vector Magic RGB-guided subpixel edge localization.
//!
//! Regular 2-material boundary vertices are shifted along the unit edge normal
//! by projecting straddling sRGB pixels onto the linear segment connecting the
//! two adjacent palette colors. High-valence junctions (>= 3 faces or colors)
//! and canvas-border vertices remain locked so multi-material meshes stay
//! 100% watertight.

use crate::arena::{is_null, Arena};
use crate::error::ContourError;
use crate::scanline::EXTERIOR_COLOR;
use kurva_types::{ColorRgb, IndexedRaster};

/// Maximum RGB-guided normal displacement in pixels.
pub const MAX_RGB_SHIFT: f64 = 0.45;
/// Maximum cumulative displacement from initial/RGB-shifted anchor after smoothing.
const MAX_SMOOTH_RADIUS: f64 = 0.65;
/// Number of tangential Laplacian smoothing passes on unlocked 2-material chains.
const SMOOTH_PASSES: usize = 2;

#[derive(Default, Clone)]
struct VertexStar {
    outgoing: Vec<u32>,
    faces: Vec<u32>,
    colors: Vec<u8>,
    touches_exterior: bool,
}

impl VertexStar {
    fn push_face(&mut self, face: u32) {
        if !is_null(face) && !self.faces.contains(&face) {
            self.faces.push(face);
        }
    }

    fn push_color(&mut self, color: u8) {
        if color == EXTERIOR_COLOR {
            self.touches_exterior = true;
        } else if !self.colors.contains(&color) {
            self.colors.push(color);
        }
    }

    fn is_locked(&self) -> bool {
        self.touches_exterior || self.faces.len() >= 3 || self.colors.len() >= 3
    }
}

fn build_stars(arena: &Arena) -> Result<Vec<VertexStar>, ContourError> {
    let mut stars = vec![VertexStar::default(); arena.vertices.len()];
    for (idx, edge) in arena.halfedges.iter().enumerate() {
        let edge_id = u32::try_from(idx)
            .map_err(|_| ContourError::Topology("half-edge arena exhausted 32-bit handles".into()))?;
        let twin = arena.halfedge(edge.twin)?;
        let star = stars.get_mut(edge.origin as usize).ok_or_else(|| {
            ContourError::Topology(format!("half-edge {idx} starts outside the vertex arena"))
        })?;
        star.outgoing.push(edge_id);
        star.push_face(edge.face);
        star.push_face(twin.face);
        star.push_color(edge.color_left);
        star.push_color(twin.color_left);
    }
    Ok(stars)
}

fn sample_rgb(rgb_bytes: &[u8], width: usize, height: usize, fx: f64, fy: f64) -> [f64; 3] {
    let max_x = width.saturating_sub(1) as f64;
    let max_y = height.saturating_sub(1) as f64;
    let px = fx.floor().clamp(0.0, max_x) as usize;
    let py = fy.floor().clamp(0.0, max_y) as usize;
    let base = (py * width + px) * 3;
    let r = rgb_bytes.get(base).copied().unwrap_or(0) as f64;
    let g = rgb_bytes.get(base + 1).copied().unwrap_or(0) as f64;
    let b = rgb_bytes.get(base + 2).copied().unwrap_or(0) as f64;
    [r, g, b]
}

fn color_vec(c: ColorRgb) -> [f64; 3] {
    [f64::from(c.r), f64::from(c.g), f64::from(c.b)]
}

/// Project observed sRGB color `c` onto the segment connecting `b` (0.0) and `a` (1.0).
fn project_alpha(c: [f64; 3], a: [f64; 3], b: [f64; 3]) -> Option<f64> {
    let ab = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let norm_sq = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if norm_sq <= 1.0e-6 {
        return None;
    }
    let cb = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
    let dot = cb[0] * ab[0] + cb[1] * ab[1] + cb[2] * ab[2];
    Some((dot / norm_sq).clamp(0.0, 1.0))
}

fn finite_clamped(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(-MAX_RGB_SHIFT, MAX_RGB_SHIFT)
    } else {
        0.0
    }
}

/// Displace regular 2-material boundary vertices using straddling RGB pixel intensities.
pub fn adjust_vertices_rgb(
    arena: &mut Arena,
    raster: &IndexedRaster,
    rgb_bytes: &[u8],
) -> Result<(), ContourError> {
    let stars = build_stars(arena)?;
    let max_x = raster.width as f64;
    let max_y = raster.height as f64;
    let mut moves: Vec<(u32, f64, f64)> = Vec::new();

    for (v_idx, (vertex, star)) in arena.vertices.iter().zip(stars.iter()).enumerate() {
        if is_null(vertex.outgoing_halfedge) || star.is_locked() || star.outgoing.is_empty() {
            continue;
        }
        if vertex.x <= 0.0 || vertex.x >= max_x || vertex.y <= 0.0 || vertex.y >= max_y {
            continue;
        }

        let mut sum_dx = 0.0;
        let mut sum_dy = 0.0;
        let mut count = 0usize;

        for &edge_id in &star.outgoing {
            let edge = arena.halfedge(edge_id)?;
            let twin = arena.halfedge(edge.twin)?;
            let (Some(&pal_a), Some(&pal_b)) = (
                raster.palette.get(edge.color_left as usize),
                raster.palette.get(twin.color_left as usize),
            ) else {
                continue;
            };
            let target = arena.vertex(edge.target)?;
            let dx = target.x - vertex.x;
            let dy = target.y - vertex.y;
            let len = dx.hypot(dy);
            if len <= 1.0e-9 {
                continue;
            }
            // Left normal points into `color_left` (A); outward normal points toward B.
            let nx = dy / len;
            let ny = -dx / len;
            let mx = 0.5 * (vertex.x + target.x);
            let my = 0.5 * (vertex.y + target.y);

            let sample_a = sample_rgb(rgb_bytes, raster.width, raster.height, mx + 0.5 * nx, my + 0.5 * ny);
            let sample_b = sample_rgb(rgb_bytes, raster.width, raster.height, mx - 0.5 * nx, my - 0.5 * ny);
            let a_vec = color_vec(pal_a);
            let b_vec = color_vec(pal_b);

            let (Some(alpha_a), Some(alpha_b)) = (
                project_alpha(sample_a, a_vec, b_vec),
                project_alpha(sample_b, a_vec, b_vec),
            ) else {
                continue;
            };
            let alpha = 0.5 * (alpha_a + alpha_b);
            let shift = (alpha - 0.5).clamp(-MAX_RGB_SHIFT, MAX_RGB_SHIFT);
            // Expand A toward B (-nx, -ny) when alpha > 0.5.
            sum_dx += shift * (-nx);
            sum_dy += shift * (-ny);
            count += 1;
        }

        if count > 0 {
            let dx = finite_clamped(sum_dx / count as f64);
            let dy = finite_clamped(sum_dy / count as f64);
            if dx != 0.0 || dy != 0.0 {
                let handle = u32::try_from(v_idx).map_err(|_| {
                    ContourError::Topology("vertex arena exhausted 32-bit handles".into())
                })?;
                moves.push((handle, dx, dy));
            }
        }
    }

    for (handle, dx, dy) in moves {
        let vertex = arena.vertex_mut(handle)?;
        vertex.x += dx;
        vertex.y += dy;
    }

    let anchors: Vec<(f64, f64)> = arena.vertices.iter().map(|v| (v.x, v.y)).collect();
    smooth_chains(arena, &stars, &anchors, max_x, max_y)?;
    Ok(())
}

fn smooth_chains(
    arena: &mut Arena,
    stars: &[VertexStar],
    anchors: &[(f64, f64)],
    max_x: f64,
    max_y: f64,
) -> Result<(), ContourError> {
    let mut updates: Vec<(u32, f64, f64)> = Vec::new();
    for _ in 0..SMOOTH_PASSES {
        updates.clear();
        for (v_idx, (vertex, star)) in arena.vertices.iter().zip(stars.iter()).enumerate() {
            if is_null(vertex.outgoing_halfedge) || star.is_locked() || star.outgoing.len() != 2 {
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
