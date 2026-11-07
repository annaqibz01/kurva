//! Scanline boundary edge emission on the dual pixel-corner grid.
//!
//! For a raster of `W x H` pixels the corners form the integer lattice
//! `[0, W] x [0, H]`, addressed by `index = y * (W + 1) + x`. Every pair of
//! 4-adjacent cells with distinct phases emits one undirected boundary edge,
//! stored as a pair of opposing half-edges.

use crate::arena::{is_null, Arena};
use crate::error::ContourError;
use kurva_types::IndexedRaster;

/// Phase id reserved for the canvas exterior.
///
/// Palette indices are `u8`, so `u8::MAX` is the only free sentinel; a palette
/// of 256 entries keeps it unused because pixel indices stop at 255.
pub const EXTERIOR_COLOR: u8 = u8::MAX;

fn buffer_error() -> ContourError {
    ContourError::Topology("raster index points outside the flat buffer".to_string())
}

/// Corner-index to vertex-handle map of the dual grid.
pub struct GridMap {
    width: usize,
    corner_vertices: Vec<u32>,
}

impl GridMap {
    /// Allocate a corner map sized for a `width x height` raster.
    pub fn new(width: usize, height: usize) -> Result<Self, ContourError> {
        let corners = (width + 1).checked_mul(height + 1).ok_or_else(|| {
            ContourError::Topology("dual grid dimensions overflow usize".to_string())
        })?;
        if corners > u32::MAX as usize {
            return Err(ContourError::Topology(
                "dual grid exceeds the 32-bit vertex handle range".to_string(),
            ));
        }
        Ok(Self {
            width,
            corner_vertices: vec![crate::arena::NULL_INDEX; corners],
        })
    }

    fn slot(&self, x: usize, y: usize) -> usize {
        y * (self.width + 1) + x
    }

    /// Resolve, allocating on first touch, the vertex at lattice corner `(x, y)`.
    pub fn vertex_at(
        &mut self,
        arena: &mut Arena,
        x: usize,
        y: usize,
    ) -> Result<u32, ContourError> {
        let slot = self.slot(x, y);
        match self.corner_vertices.get(slot) {
            Some(&existing) if !is_null(existing) => Ok(existing),
            Some(_) => {
                let handle = arena.add_vertex(x as f64, y as f64)?;
                if let Some(entry) = self.corner_vertices.get_mut(slot) {
                    *entry = handle;
                }
                Ok(handle)
            }
            None => Err(ContourError::Topology(format!(
                "corner ({x}, {y}) lies outside the dual grid"
            ))),
        }
    }
}

/// Read the phase of the pixel at `(x, y)`, or the exterior phase when outside.
fn phase_at(raster: &IndexedRaster, x: isize, y: isize) -> Result<u8, ContourError> {
    let width = raster.width as isize;
    let height = raster.height as isize;
    if x < 0 || y < 0 || x >= width || y >= height {
        return Ok(EXTERIOR_COLOR);
    }
    let offset = y as usize * raster.width + x as usize;
    raster.indices.get(offset).copied().ok_or_else(buffer_error)
}

/// Phase sitting on the left of the unit step `from -> to`.
///
/// Left of a direction `(dx, dy)` in image coordinates (y down) is `(dy, -dx)`,
/// so the probe point is the edge midpoint pushed half a unit along that normal.
/// Coordinates are doubled to stay in exact integer arithmetic.
fn left_phase(
    raster: &IndexedRaster,
    from: (usize, usize),
    to: (usize, usize),
) -> Result<u8, ContourError> {
    let dx = to.0 as isize - from.0 as isize;
    let dy = to.1 as isize - from.1 as isize;
    let probe_x = 2 * from.0 as isize + dx + dy;
    let probe_y = 2 * from.1 as isize + dy - dx;
    phase_at(raster, probe_x.div_euclid(2), probe_y.div_euclid(2))
}

fn emit_pair(
    arena: &mut Arena,
    grid: &mut GridMap,
    raster: &IndexedRaster,
    from: (usize, usize),
    to: (usize, usize),
) -> Result<(), ContourError> {
    let origin = grid.vertex_at(arena, from.0, from.1)?;
    let target = grid.vertex_at(arena, to.0, to.1)?;
    let forward = arena.add_halfedge(origin, target, left_phase(raster, from, to)?)?;
    let backward = arena.add_halfedge(target, origin, left_phase(raster, to, from)?)?;
    arena.halfedge_mut(forward)?.twin = backward;
    arena.halfedge_mut(backward)?.twin = forward;
    Ok(())
}

/// Emit every boundary edge of `raster` into `arena`.
///
/// Interior interfaces are visited once from each axis; canvas borders compare
/// against the exterior phase, which seals the outer ring of the cell complex.
pub fn emit_boundary(arena: &mut Arena, raster: &IndexedRaster) -> Result<GridMap, ContourError> {
    let width = raster.width;
    let height = raster.height;
    let mut grid = GridMap::new(width, height)?;

    for y in 0..height {
        for x in 0..=width {
            let left = if x == 0 {
                EXTERIOR_COLOR
            } else {
                *raster
                    .indices
                    .get(y * width + x - 1)
                    .ok_or_else(buffer_error)?
            };
            let right = if x == width {
                EXTERIOR_COLOR
            } else {
                *raster.indices.get(y * width + x).ok_or_else(buffer_error)?
            };
            if left != right {
                emit_pair(arena, &mut grid, raster, (x, y), (x, y + 1))?;
            }
        }
    }

    for y in 0..=height {
        for x in 0..width {
            let top = if y == 0 {
                EXTERIOR_COLOR
            } else {
                *raster
                    .indices
                    .get((y - 1) * width + x)
                    .ok_or_else(buffer_error)?
            };
            let bottom = if y == height {
                EXTERIOR_COLOR
            } else {
                *raster.indices.get(y * width + x).ok_or_else(buffer_error)?
            };
            if top != bottom {
                emit_pair(arena, &mut grid, raster, (x, y), (x + 1, y))?;
            }
        }
    }

    Ok(grid)
}
