//! Flat, pointer-free topology arena for the KARST-v2 cell complex.
//!
//! Every topological entity lives in a `Vec` and is addressed through a 32-bit
//! handle, so the planar map is pure index space: no raw pointers, no `unsafe`,
//! and no lifetime coupling between the map and its storage.

use crate::error::ContourError;

/// Handle to a [`Vertex`].
pub type VertexId = u32;
/// Handle to an undirected edge, stored as the lower of its two half-edges.
pub type EdgeId = u32;
/// Handle to a directed [`HalfEdge`].
pub type HalfEdgeId = u32;
/// Handle to a [`Face`].
pub type FaceId = u32;

/// Sentinel handle meaning "no entity".
pub const NULL_INDEX: u32 = u32::MAX;

/// True when `handle` carries the [`NULL_INDEX`] sentinel.
#[inline]
pub fn is_null(handle: u32) -> bool {
    handle == NULL_INDEX
}

/// Coordinate embedding plus one topology anchor of a 0-cell.
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    pub outgoing_halfedge: u32,
}

/// A directed 1-cell; `twin` is the same edge traversed the other way.
#[derive(Debug, Clone, Copy)]
pub struct HalfEdge {
    pub origin: u32,
    pub target: u32,
    pub twin: u32,
    pub next: u32,
    pub face: u32,
    pub color_left: u8,
}

/// A 2-cell: exactly one boundary cycle of the planar map.
#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub halfedge: u32,
    pub color_id: u8,
    pub is_hole: bool,
    pub is_exterior: bool,
    pub area: f64,
}

/// Flat arena owning every vertex, half-edge and face of one map.
#[derive(Debug, Default)]
pub struct Arena {
    pub vertices: Vec<Vertex>,
    pub halfedges: Vec<HalfEdge>,
    pub faces: Vec<Face>,
}

fn exhausted(kind: &str) -> ContourError {
    ContourError::Topology(format!("{kind} arena exhausted 32-bit handles"))
}

fn missing(kind: &str, handle: u32) -> ContourError {
    ContourError::Topology(format!("{kind} handle {handle} is outside the arena"))
}

impl Arena {
    /// Allocate an empty arena with room for the given entity counts.
    pub fn with_capacity(vertices: usize, halfedges: usize, faces: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(vertices),
            halfedges: Vec::with_capacity(halfedges),
            faces: Vec::with_capacity(faces),
        }
    }

    /// Append a vertex anchored at `(x, y)` and return its handle.
    pub fn add_vertex(&mut self, x: f64, y: f64) -> Result<VertexId, ContourError> {
        let handle = u32::try_from(self.vertices.len()).map_err(|_| exhausted("vertex"))?;
        self.vertices.push(Vertex {
            x,
            y,
            outgoing_halfedge: NULL_INDEX,
        });
        Ok(handle)
    }

    /// Append a half-edge without twins or links and return its handle.
    pub fn add_halfedge(
        &mut self,
        origin: u32,
        target: u32,
        color_left: u8,
    ) -> Result<HalfEdgeId, ContourError> {
        let handle = u32::try_from(self.halfedges.len()).map_err(|_| exhausted("half-edge"))?;
        self.halfedges.push(HalfEdge {
            origin,
            target,
            twin: NULL_INDEX,
            next: NULL_INDEX,
            face: NULL_INDEX,
            color_left,
        });
        Ok(handle)
    }

    /// Append a face seeded with `halfedge` and return its handle.
    pub fn add_face(&mut self, halfedge: u32, color_id: u8) -> Result<FaceId, ContourError> {
        let handle = u32::try_from(self.faces.len()).map_err(|_| exhausted("face"))?;
        self.faces.push(Face {
            halfedge,
            color_id,
            is_hole: false,
            is_exterior: false,
            area: 0.0,
        });
        Ok(handle)
    }

    /// Borrow a vertex.
    pub fn vertex(&self, id: VertexId) -> Result<&Vertex, ContourError> {
        self.vertices
            .get(id as usize)
            .ok_or_else(|| missing("vertex", id))
    }

    /// Borrow a vertex mutably.
    pub fn vertex_mut(&mut self, id: VertexId) -> Result<&mut Vertex, ContourError> {
        self.vertices
            .get_mut(id as usize)
            .ok_or_else(|| missing("vertex", id))
    }

    /// Borrow a half-edge.
    pub fn halfedge(&self, id: HalfEdgeId) -> Result<&HalfEdge, ContourError> {
        self.halfedges
            .get(id as usize)
            .ok_or_else(|| missing("half-edge", id))
    }

    /// Borrow a half-edge mutably.
    pub fn halfedge_mut(&mut self, id: HalfEdgeId) -> Result<&mut HalfEdge, ContourError> {
        self.halfedges
            .get_mut(id as usize)
            .ok_or_else(|| missing("half-edge", id))
    }

    /// Borrow a face.
    pub fn face(&self, id: FaceId) -> Result<&Face, ContourError> {
        self.faces
            .get(id as usize)
            .ok_or_else(|| missing("face", id))
    }

    /// Borrow a face mutably.
    pub fn face_mut(&mut self, id: FaceId) -> Result<&mut Face, ContourError> {
        self.faces
            .get_mut(id as usize)
            .ok_or_else(|| missing("face", id))
    }
}
