//! Truncated-SVD level-set embedding of lattice junctions.
//!
//! Every pixel corner is the sample point of a bilinear indicator field per
//! phase. Requiring each phase field to reach its `0.5` iso-value yields one
//! linearised constraint per phase; the least-squares step of that system is
//! the truncated pseudo-inverse displacement. Rank-deficient saddles clamp to
//! zero instead of producing `NaN`, and every step is bounded so no segment can
//! shrink below half a pixel or fold over its neighbour.

use crate::arena::{is_null, Arena};
use crate::error::ContourError;
use kurva_types::IndexedRaster;

/// Largest displacement allowed on either axis, in pixels.
pub const MAX_DISPLACEMENT: f64 = 0.25;

/// Singular values below this fraction of the largest one are truncated.
const RELATIVE_CUTOFF: f64 = 1e-9;

/// Phase samples of the four quadrants around a junction, ordered
/// top-left, top-right, bottom-left, bottom-right.
type Quadrants = [u8; 4];

fn pixel(raster: &IndexedRaster, x: usize, y: usize) -> Result<u8, ContourError> {
    raster
        .indices
        .get(y * raster.width + x)
        .copied()
        .ok_or_else(|| ContourError::Topology(format!("pixel ({x}, {y}) is outside the buffer")))
}

fn quadrants(raster: &IndexedRaster, x: usize, y: usize) -> Result<Quadrants, ContourError> {
    Ok([
        pixel(raster, x - 1, y - 1)?,
        pixel(raster, x, y - 1)?,
        pixel(raster, x - 1, y)?,
        pixel(raster, x, y)?,
    ])
}

fn distinct(quadrants: &Quadrants) -> usize {
    let mut seen: [u8; 4] = [0; 4];
    let mut count = 0;
    for &phase in quadrants.iter() {
        if !seen[..count].contains(&phase) {
            seen[count] = phase;
            count += 1;
        }
    }
    count
}

fn indicator(phase: u8, sample: u8) -> f64 {
    if phase == sample {
        1.0
    } else {
        0.0
    }
}

/// Unit eigenvector of the symmetric `2 x 2` matrix `[[a, b], [b, c]]`.
fn eigen_vector(a: f64, b: f64, c: f64, lambda: f64) -> [f64; 2] {
    if b.abs() > f64::EPSILON {
        let vector = [b, lambda - a];
        let norm = (vector[0] * vector[0] + vector[1] * vector[1]).sqrt();
        if norm > 0.0 {
            return [vector[0] / norm, vector[1] / norm];
        }
    }
    if (lambda - a).abs() <= (lambda - c).abs() {
        [1.0, 0.0]
    } else {
        [0.0, 1.0]
    }
}

fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// Least-squares junction displacement for one neighbourhood.
///
/// The jacobian rows `(gx, gy)` and residuals `g` of every present phase feed
/// the normal form `J^T J` and `J^T g`; the displacement is then
/// `-sum (v_i . p) / lambda_i * v_i` over the retained singular directions.
fn solve(quadrants: Quadrants) -> (f64, f64) {
    let mut phases: [u8; 4] = [0; 4];
    let mut phase_count = 0;
    for &phase in quadrants.iter() {
        if !phases[..phase_count].contains(&phase) {
            phases[phase_count] = phase;
            phase_count += 1;
        }
    }

    let (mut a, mut b, mut c, mut r, mut s) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for &phase in phases[..phase_count].iter() {
        let tl = indicator(phase, quadrants[0]);
        let tr = indicator(phase, quadrants[1]);
        let bl = indicator(phase, quadrants[2]);
        let br = indicator(phase, quadrants[3]);
        let gx = 0.5 * ((tr + br) - (tl + bl));
        let gy = 0.5 * ((bl + br) - (tl + tr));
        let residual = 0.25 * (tl + tr + bl + br) - 0.5;
        a += gx * gx;
        b += gx * gy;
        c += gy * gy;
        r += gx * residual;
        s += gy * residual;
    }

    let trace = a + c;
    let spread = (0.25 * (a - c) * (a - c) + b * b).sqrt();
    let lambda_major = 0.5 * trace + spread;
    let lambda_minor = (0.5 * trace - spread).max(0.0);
    let cutoff = RELATIVE_CUTOFF * lambda_major.max(0.0).sqrt();

    let mut dx = 0.0;
    let mut dy = 0.0;
    for lambda in [lambda_major.max(0.0), lambda_minor] {
        let sigma = lambda.sqrt();
        if sigma <= cutoff {
            continue;
        }
        let vector = eigen_vector(a, b, c, lambda);
        let projection = vector[0] * r + vector[1] * s;
        dx -= projection / lambda * vector[0];
        dy -= projection / lambda * vector[1];
    }

    let dx = finite(dx).clamp(-MAX_DISPLACEMENT, MAX_DISPLACEMENT);
    let dy = finite(dy).clamp(-MAX_DISPLACEMENT, MAX_DISPLACEMENT);
    (dx, dy)
}

/// Relax every interior junction of the arena onto its phase iso-surface.
///
/// Pixels outside the canvas are never sampled: the pass only touches lattice
/// corners with `1 <= x <= W - 1` and `1 <= y <= H - 1`, so the exterior phase
/// never enters the level-set system.
pub fn adjust_vertices(arena: &mut Arena, raster: &IndexedRaster) -> Result<(), ContourError> {
    let max_x = raster.width as f64 - 1.0;
    let max_y = raster.height as f64 - 1.0;
    let mut moves: Vec<(u32, f64, f64)> = Vec::new();

    for (index, vertex) in arena.vertices.iter().enumerate() {
        if is_null(vertex.outgoing_halfedge) {
            continue;
        }
        let x = vertex.x.round();
        let y = vertex.y.round();
        if !(x >= 1.0 && y >= 1.0 && x <= max_x && y <= max_y) {
            continue;
        }
        let samples = quadrants(raster, x as usize, y as usize)?;
        if distinct(&samples) < 2 {
            continue;
        }
        let (dx, dy) = solve(samples);
        if dx != 0.0 || dy != 0.0 {
            moves.push((index as u32, dx, dy));
        }
    }

    for (handle, dx, dy) in moves {
        let vertex = arena.vertex_mut(handle)?;
        vertex.x += dx;
        vertex.y += dy;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{adjust_vertices, solve, MAX_DISPLACEMENT};
    use crate::arena::Arena;
    use crate::error::ContourError;
    use crate::geometry::classify_faces;
    use crate::scanline::emit_boundary;
    use crate::topology::{extract_faces, link_rotation};
    use kurva_types::{ColorRgb, IndexedRaster};

    fn build(width: usize, height: usize, indices: Vec<u8>) -> IndexedRaster {
        let palette = vec![ColorRgb::new(0, 0, 0), ColorRgb::new(255, 255, 255)];
        match IndexedRaster::new(width, height, palette, indices) {
            Some(raster) => raster,
            None => panic!("test raster was rejected"),
        }
    }

    #[test]
    fn checkerboard_saddle_stability() -> Result<(), ContourError> {
        let raster = build(2, 2, vec![0, 1, 1, 0]);
        let mut arena = Arena::default();
        emit_boundary(&mut arena, &raster)?;
        link_rotation(&mut arena)?;
        extract_faces(&mut arena)?;
        adjust_vertices(&mut arena, &raster)?;

        for vertex in &arena.vertices {
            assert!(vertex.x.is_finite() && vertex.y.is_finite());
            assert!((vertex.x - vertex.x.round()).abs() <= MAX_DISPLACEMENT);
            assert!((vertex.y - vertex.y.round()).abs() <= MAX_DISPLACEMENT);
        }

        classify_faces(&mut arena)?;
        let internal = arena.faces.iter().filter(|face| !face.is_exterior).count();
        assert_eq!(internal, 4);
        for face in arena.faces.iter().filter(|face| !face.is_exterior) {
            assert!((face.area.abs() - 1.0).abs() < 1e-9);
            assert!(!face.is_hole);
        }
        Ok(())
    }

    #[test]
    fn rank_deficient_neighbourhood_clamps_to_zero() {
        assert_eq!(solve([0, 1, 1, 0]), (0.0, 0.0));
        assert_eq!(solve([0, 0, 1, 1]), (0.0, 0.0));
        assert_eq!(solve([0, 0, 0, 0]), (0.0, 0.0));
    }
}
