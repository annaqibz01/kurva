//! Frozen intermediate representation shared by every stage of the kurva pipeline.
//!
//! This crate is the single source of truth for geometry and data-exchange types.
//! It must never depend on another crate inside this workspace.

use serde::{Deserialize, Serialize};

/// Generic two-dimensional coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point2D<T> {
    pub x: T,
    pub y: T,
}

impl<T> Point2D<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl Point2D<f64> {
    /// Squared Euclidean distance, used when only distance ordering matters.
    pub fn distance_squared(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    /// Euclidean distance between two coordinates.
    pub fn distance(self, other: Self) -> f64 {
        self.distance_squared(other).sqrt()
    }
}

/// Opaque sRGB entry of an indexed palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl ColorRgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Palette-indexed raster; `indices` is row-major with one `u8` index per pixel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedRaster {
    pub width: usize,
    pub height: usize,
    pub palette: Vec<ColorRgb>,
    pub indices: Vec<u8>,
}

impl IndexedRaster {
    /// Builds a raster only when the index buffer covers exactly `width * height` pixels.
    ///
    /// Returns `None` on dimension overflow as well as on a truncated or padded buffer, so a
    /// raster with unreachable pixel addresses cannot be constructed at all.
    pub fn new(
        width: usize,
        height: usize,
        palette: Vec<ColorRgb>,
        indices: Vec<u8>,
    ) -> Option<Self> {
        let pixels = width.checked_mul(height)?;
        if indices.len() != pixels {
            return None;
        }
        Some(Self {
            width,
            height,
            palette,
            indices,
        })
    }

    /// Number of pixels addressed by the index buffer.
    pub fn pixel_count(&self) -> usize {
        self.indices.len()
    }
}

/// Closed boundary loop of discrete pixel coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawContour {
    pub points: Vec<Point2D<i32>>,
    pub is_hole: bool,
}

/// Output of the border-following stage.
pub type RawContourList = Vec<RawContour>;

/// Polygon after RDP reduction, retaining the vertex indices classified as cusps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimplifiedPolyline {
    pub points: Vec<Point2D<f64>>,
    pub corner_indices: Vec<usize>,
    pub is_closed: bool,
}

/// Cubic Bezier segment in 64-bit coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BezierSegment {
    pub p0: Point2D<f64>,
    pub p1: Point2D<f64>,
    pub p2: Point2D<f64>,
    pub p3: Point2D<f64>,
}

impl BezierSegment {
    /// Evaluates the cubic Bernstein form at parameter `t`.
    ///
    /// `B(t) = (1-t)^3 P0 + 3(1-t)^2 t P1 + 3(1-t) t^2 P2 + t^3 P3`, with `t` expected in
    /// `[0, 1]`; outside that range the polynomial extrapolates along the same tangent.
    pub fn sample(&self, t: f64) -> Point2D<f64> {
        let u = 1.0 - t;
        let b0 = u * u * u;
        let b1 = 3.0 * u * u * t;
        let b2 = 3.0 * u * t * t;
        let b3 = t * t * t;
        Point2D::new(
            b0 * self.p0.x + b1 * self.p1.x + b2 * self.p2.x + b3 * self.p3.x,
            b0 * self.p0.y + b1 * self.p1.y + b2 * self.p2.y + b3 * self.p3.y,
        )
    }
}

/// Single vector outline bound to one palette entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    pub color_id: u8,
    pub segments: Vec<BezierSegment>,
    pub is_closed: bool,
}

/// Output of the curve-fitting stage.
pub type VectorPathList = Vec<VectorPath>;

#[cfg(test)]
mod tests {
    use super::{BezierSegment, ColorRgb, IndexedRaster, Point2D};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1.0e-12
    }

    #[test]
    fn distance_helpers_follow_pythagoras() {
        let origin = Point2D::new(0.0_f64, 0.0_f64);
        let corner = Point2D::new(3.0_f64, 4.0_f64);
        assert!(close(origin.distance_squared(corner), 25.0));
        assert!(close(origin.distance(corner), 5.0));
        assert!(close(corner.distance(origin), 5.0));
    }

    #[test]
    fn point2d_new_preserves_components() {
        let point = Point2D::new(-2_i32, 7_i32);
        assert_eq!(point.x, -2);
        assert_eq!(point.y, 7);
        assert_eq!(point, Point2D::new(-2, 7));
    }

    #[test]
    fn indexed_raster_enforces_pixel_count() {
        let palette = vec![ColorRgb::new(12, 34, 56)];
        assert!(IndexedRaster::new(2, 2, palette.clone(), vec![0, 0, 0]).is_none());
        assert!(IndexedRaster::new(2, 2, palette.clone(), vec![0; 5]).is_none());
        assert!(IndexedRaster::new(usize::MAX, 2, palette.clone(), Vec::new()).is_none());
        let raster = IndexedRaster::new(2, 2, palette, vec![0; 4]);
        assert!(raster.is_some());
        if let Some(raster) = raster {
            assert_eq!(raster.pixel_count(), 4);
            assert_eq!(raster.palette.len(), 1);
            assert_eq!(raster.width * raster.height, raster.indices.len());
        }
    }

    #[test]
    fn bezier_sample_hits_endpoints_and_midpoint() {
        let segment = BezierSegment {
            p0: Point2D::new(0.0, 0.0),
            p1: Point2D::new(0.0, 1.0),
            p2: Point2D::new(1.0, 1.0),
            p3: Point2D::new(1.0, 0.0),
        };
        let start = segment.sample(0.0);
        assert!(close(start.x, 0.0) && close(start.y, 0.0));
        let end = segment.sample(1.0);
        assert!(close(end.x, 1.0) && close(end.y, 0.0));
        let mid = segment.sample(0.5);
        assert!(close(mid.x, 0.5));
        assert!(close(mid.y, 0.75));
    }
}
