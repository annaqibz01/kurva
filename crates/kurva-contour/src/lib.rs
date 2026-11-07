//! KARST-v2: watertight multi-material contours from indexed rasters.
//!
//! The pipeline turns a raster into a pointer-free cell complex and returns one
//! closed cycle per boundary component:
//!
//! 1. [`scanline`] emits unit boundary edges on the dual pixel-corner lattice.
//! 2. [`topology`] assembles twins, the rotation system and the face cycles.
//! 3. [`subpixel`] relaxes junctions onto their bilinear level-set iso-value.
//! 4. [`geometry`] measures signed area and classifies shells against holes.
//!
//! Every material interface is shared by exactly two half-edges, so the output
//! is watertight by construction: no gap, no overlap, no duplicated border.

mod arena;
mod error;
mod geometry;
mod scanline;
mod subpixel;
mod topology;

pub use arena::{Arena, EdgeId, Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId, NULL_INDEX};
pub use error::ContourError;
pub use scanline::EXTERIOR_COLOR;

use kurva_types::{IndexedRaster, Point2D, RawContour};

/// Upper bound on the pre-allocated arena slots, so a huge canvas grows on
/// demand instead of reserving the worst case up front.
const CAPACITY_CEILING: usize = 1 << 18;

/// Validate the raster header and its flat index buffer.
fn validate(raster: &IndexedRaster) -> Result<usize, ContourError> {
    if raster.width == 0 || raster.height == 0 {
        return Err(ContourError::EmptyCanvas);
    }
    let expected = raster.width.saturating_mul(raster.height);
    if raster.indices.len() != expected {
        return Err(ContourError::InconsistentRaster {
            expected,
            actual: raster.indices.len(),
        });
    }
    Ok(expected)
}

/// Extract every material contour of `raster`.
///
/// The result holds one entry per boundary component, each flagged by
/// [`RawContour::is_hole`], with the unbounded canvas exterior excluded.
/// Vertices are mapped back onto the integer pixel-corner lattice after the
/// subpixel pass.
pub fn extract_contours(raster: &IndexedRaster) -> Result<Vec<RawContour>, ContourError> {
    let expected = validate(raster)?;
    let estimate = expected.min(CAPACITY_CEILING);
    let mut arena = Arena::with_capacity(estimate, estimate.saturating_mul(4), estimate);

    scanline::emit_boundary(&mut arena, raster)?;
    topology::link_rotation(&mut arena)?;
    topology::extract_faces(&mut arena)?;
    subpixel::adjust_vertices(&mut arena, raster)?;
    geometry::classify_faces(&mut arena)?;

    let mut contours = Vec::with_capacity(arena.faces.len());
    for face_id in 0..arena.faces.len() as u32 {
        let face = *arena.face(face_id)?;
        if face.is_exterior {
            continue;
        }
        let points = geometry::face_cycle_points(&arena, face_id)?
            .into_iter()
            .map(|(x, y)| Point2D::new(x.round() as i32, y.round() as i32))
            .collect();
        contours.push(RawContour {
            color_id: face.color_id,
            points,
            is_hole: face.is_hole,
        });
    }
    Ok(contours)
}

#[cfg(test)]
mod tests;
