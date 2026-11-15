//! Corner-anchored, reversal-invariant shared-edge Bezier network.
//!
//! Decomposes polylines at `G0` cusps into open corner-to-corner chains,
//! classifies flat chains as exact collinear cubics, and memoizes fitted
//! chains in canonical orientation so shared boundaries between adjacent
//! regions receive bit-identical opposing control points.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::curve::reverse_segment;
use crate::error::FitError;
use crate::refine;
use crate::vec2::{add, dot, mul, norm, sub};
use kurva_types::{BezierSegment, Point2D, SimplifiedPolyline, VectorPath};

const KEY_SCALE: f64 = 1.0e6;

type ChainKey = Vec<(i64, i64)>;

/// Fit all polylines through a shared-chain cache so adjacent faces share
/// exact reversed twin `BezierSegment`s along common boundaries.
pub fn fit_network(
    polylines: &[SimplifiedPolyline],
    eps: f64,
) -> Result<Vec<VectorPath>, FitError> {
    let mut cache: HashMap<ChainKey, Vec<BezierSegment>> = HashMap::new();
    let mut paths = Vec::with_capacity(polylines.len());

    for polyline in polylines {
        let (chains, periodic) = decompose_chains(polyline);
        let mut segments = Vec::new();
        for chain in chains {
            if chain.len() < 2 {
                continue;
            }
            if periodic {
                segments.extend(refine::fit_arc(&chain, eps, true)?);
                continue;
            }
            let (canonical, reversed) = canonicalize(&chain);
            let key = quantize_chain(&canonical);
            let fitted = if let Some(cached) = cache.get(&key) {
                cached.clone()
            } else {
                let segs = fit_open_chain(&canonical, eps)?;
                cache.insert(key, segs.clone());
                segs
            };
            if reversed {
                segments.extend(fitted.iter().rev().copied().map(reverse_segment));
            } else {
                segments.extend(fitted);
            }
        }
        paths.push(VectorPath {
            color_id: polyline.color_id,
            segments,
            is_closed: polyline.is_closed,
        });
    }
    Ok(paths)
}

fn decompose_chains(polyline: &SimplifiedPolyline) -> (Vec<Vec<Point2D<f64>>>, bool) {
    let points = &polyline.points;
    let count = points.len();
    if count < 2 {
        return (Vec::new(), false);
    }
    if !polyline.is_closed {
        let mut bounds = vec![0];
        bounds.extend(
            polyline
                .corner_indices
                .iter()
                .copied()
                .filter(|&c| c > 0 && c < count - 1),
        );
        bounds.sort_unstable();
        bounds.dedup();
        bounds.push(count - 1);
        let chains = bounds
            .windows(2)
            .filter(|pair| pair[1] > pair[0])
            .map(|pair| points[pair[0]..=pair[1]].to_vec())
            .collect();
        return (chains, false);
    }

    let mut corners: Vec<usize> = polyline
        .corner_indices
        .iter()
        .copied()
        .filter(|&c| c < count)
        .collect();
    corners.sort_unstable();
    corners.dedup();

    if corners.is_empty() {
        let mut ring = points.clone();
        ring.push(points[0]);
        return (vec![ring], true);
    }

    if corners.len() == 1 {
        let c0 = corners[0];
        let c1 = farthest_from(points, c0);
        if c1 != c0 {
            corners.push(c1);
            corners.sort_unstable();
        }
    }

    if corners.len() < 2 {
        let mut ring = points.clone();
        ring.push(points[0]);
        return (vec![ring], false);
    }

    let chains = (0..corners.len())
        .map(|idx| {
            let from = corners[idx];
            let to = corners[(idx + 1) % corners.len()];
            let steps = match (to + count - from) % count {
                0 => count,
                value => value,
            };
            (0..=steps)
                .map(|step| points[(from + step) % count])
                .collect()
        })
        .collect();
    (chains, false)
}

fn farthest_from(points: &[Point2D<f64>], origin: usize) -> usize {
    let p0 = points[origin];
    let mut best_idx = origin;
    let mut best_d2 = -1.0;
    for (idx, &p) in points.iter().enumerate() {
        if idx == origin {
            continue;
        }
        let d = sub(p, p0);
        let d2 = dot(d, d);
        if d2 > best_d2 {
            best_d2 = d2;
            best_idx = idx;
        }
    }
    best_idx
}

fn cmp_point(a: Point2D<f64>, b: Point2D<f64>) -> Ordering {
    a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y))
}

fn canonicalize(chain: &[Point2D<f64>]) -> (Vec<Point2D<f64>>, bool) {
    let last = chain.len() - 1;
    let mut rev = match cmp_point(chain[last], chain[0]) {
        Ordering::Less => true,
        Ordering::Greater => false,
        Ordering::Equal => {
            let mid = chain[chain.len() / 2];
            let mut order = cmp_point(mid, chain[0]);
            if order == Ordering::Equal {
                order = chain
                    .iter()
                    .rev()
                    .zip(chain.iter())
                    .map(|(&a, &b)| cmp_point(a, b))
                    .find(|&o| o != Ordering::Equal)
                    .unwrap_or(Ordering::Equal);
            }
            order == Ordering::Less
        }
    };
    if cmp_point(chain[last], chain[0]) == Ordering::Equal {
        rev = chain
            .iter()
            .rev()
            .zip(chain.iter())
            .map(|(&a, &b)| cmp_point(a, b))
            .find(|&o| o != Ordering::Equal)
            == Some(Ordering::Less);
    }
    if rev {
        (chain.iter().copied().rev().collect(), true)
    } else {
        (chain.to_vec(), false)
    }
}

fn quantize_chain(chain: &[Point2D<f64>]) -> ChainKey {
    chain
        .iter()
        .map(|p| ((p.x * KEY_SCALE).round() as i64, (p.y * KEY_SCALE).round() as i64))
        .collect()
}

fn max_chord_deviation(chain: &[Point2D<f64>]) -> f64 {
    let p0 = chain[0];
    let pm = chain[chain.len() - 1];
    let chord = sub(pm, p0);
    let len_sq = dot(chord, chord);
    let mut max_dev = 0.0_f64;
    for &p in &chain[1..chain.len() - 1] {
        let dev = if len_sq <= 1.0e-18 {
            norm(sub(p, p0))
        } else {
            let t = (dot(sub(p, p0), chord) / len_sq).clamp(0.0, 1.0);
            let proj = add(p0, mul(chord, t));
            norm(sub(p, proj))
        };
        if dev > max_dev {
            max_dev = dev;
        }
    }
    max_dev
}

fn fit_open_chain(chain: &[Point2D<f64>], eps: f64) -> Result<Vec<BezierSegment>, FitError> {
    let p0 = chain[0];
    let pm = chain[chain.len() - 1];
    if chain.len() == 2 || max_chord_deviation(chain) <= eps * 0.5 {
        let chord = sub(pm, p0);
        return Ok(vec![BezierSegment {
            p0,
            p1: add(p0, mul(chord, 1.0 / 3.0)),
            p2: add(p0, mul(chord, 2.0 / 3.0)),
            p3: pm,
        }]);
    }
    refine::fit_arc(chain, eps, false)
}
