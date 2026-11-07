//! Periodic closed-loop topology: anchor selection, anchor arcs and circular primitives.

use std::f64::consts::{PI, TAU};

use crate::curvature::{Class, CurvatureProfile, KAPPA_MIN};
use crate::geometry::{LoopGeometry, EPS, V2};
use crate::view::LoopView;

/// Coefficient of curvature variation below which a loop is treated as a circle.
pub(crate) const KAPPA_VARIATION_LIMIT: f64 = 0.02;
/// Loops below this vertex count are never treated as circular primitives.
const MIN_PRIMITIVE_VERTICES: usize = 8;

/// Rotation offset of the seam: the sharpest pinned cusp, else the peak curvature vertex.
///
/// Both candidates are maximizers with a smallest-index tie-break, which makes the seam
/// a deterministic function of the canonical vertex order.
pub(crate) fn anchor_offset(
    geom: &LoopGeometry,
    prof: &CurvatureProfile,
    class: &[Class],
) -> usize {
    let n = geom.pts.len();
    let mut sharpest: Option<(f64, usize)> = None;
    for (index, kind) in class.iter().enumerate() {
        if *kind != Class::Cusp {
            continue;
        }
        if sharpest.is_none_or(|(best, _)| prof.score[index] > best) {
            sharpest = Some((prof.score[index], index));
        }
    }
    if let Some((_, index)) = sharpest {
        return index;
    }
    let mut peak: Option<(f64, usize)> = None;
    for index in 0..n {
        let response = prof.smoothed[index].abs();
        if peak.is_none_or(|(best, _)| response > best) {
            peak = Some((response, index));
        }
    }
    peak.map_or(0, |(_, index)| index)
}

/// Index ranges delimited by pinned cusps; a cusp-free loop yields the single arc `(0, n)`.
pub(crate) fn arcs(view: &LoopView) -> Vec<(usize, usize)> {
    let mut anchors = vec![0usize];
    for index in 1..view.n {
        if view.cusp[index] {
            anchors.push(index);
        }
    }
    let mut out: Vec<(usize, usize)> = anchors.windows(2).map(|pair| (pair[0], pair[1])).collect();
    let last = anchors[anchors.len() - 1];
    out.push((last, view.n));
    out
}

fn centroid(pts: &[V2]) -> V2 {
    let count = pts.len() as f64;
    let mut sum = V2::new(0.0, 0.0);
    for point in pts {
        sum = sum.add(*point);
    }
    sum.scale(1.0 / count)
}

/// Regular polygon sampling of a loop whose curvature is constant within 2 percent.
///
/// The side count follows from the sagitta bound `R * (1 - cos(pi / k)) <= epsilon`.
pub(crate) fn circular_primitive(
    geom: &LoopGeometry,
    offset: usize,
    epsilon: f64,
) -> Option<Vec<V2>> {
    let n = geom.pts.len();
    if n < MIN_PRIMITIVE_VERTICES {
        return None;
    }
    let mean = geom.kappa.iter().map(|k| k.abs()).sum::<f64>() / n as f64;
    if mean < KAPPA_MIN {
        return None;
    }
    let deviation = geom
        .kappa
        .iter()
        .map(|k| (k.abs() - mean).powi(2))
        .sum::<f64>()
        / n as f64;
    if deviation.sqrt() / mean >= KAPPA_VARIATION_LIMIT {
        return None;
    }
    let center = centroid(&geom.pts);
    let radius = geom
        .pts
        .iter()
        .map(|point| point.distance(center))
        .sum::<f64>()
        / n as f64;
    if radius <= EPS {
        return None;
    }
    let sagitta = (epsilon / radius).clamp(0.0, 1.0 - 1e-6);
    let step = (1.0 - sagitta).clamp(-1.0, 1.0).acos();
    if step <= EPS {
        return None;
    }
    let sides = (PI / step).ceil() as usize;
    if sides >= n {
        return None;
    }
    let sides = sides.max(3);
    let phase = geom.pts[offset].sub(center).angle();
    let mut out = Vec::with_capacity(sides);
    for corner in 0..sides {
        let angle = phase + TAU * corner as f64 / sides as f64;
        out.push(V2::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::analyze_closed;

    fn disc(sides: usize, radius: f64) -> LoopGeometry {
        let pts: Vec<V2> = (0..sides)
            .map(|k| {
                let angle = TAU * k as f64 / sides as f64;
                V2::new(radius * angle.cos(), radius * angle.sin())
            })
            .collect();
        match analyze_closed(&pts) {
            Ok(geom) => geom,
            Err(error) => panic!("expected closed loop, got {error}"),
        }
    }

    #[test]
    fn a_circle_samples_a_regular_polygon_inside_the_sagitta_bound() {
        let geom = disc(32, 10.0);
        let polygon = match circular_primitive(&geom, 0, 1.0) {
            Some(polygon) => polygon,
            None => panic!("expected a circular primitive"),
        };
        assert!((3..32).contains(&polygon.len()));
        for point in &polygon {
            let radius = point.norm();
            assert!((radius - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn tiny_epsilon_keeps_the_original_resolution() {
        let geom = disc(32, 10.0);
        assert!(circular_primitive(&geom, 0, 1e-4).is_none());
    }
}
