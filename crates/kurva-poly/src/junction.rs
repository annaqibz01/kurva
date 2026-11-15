//! Topological triple-junction detection across multi-region contours.
//!
//! Vertices shared by >= 3 contour boundary occurrences or >= 3 distinct
//! material colors on a `1.0e-4` grid are locked as `G0` cusps so adjacent
//! regions never drift apart during polygon simplification.

use std::collections::{HashMap, HashSet};

use crate::geometry::V2;

const GRID_SCALE: f64 = 1.0e4;

#[derive(Default)]
struct JunctionStats {
    occurrences: usize,
    colors: Vec<u8>,
}

impl JunctionStats {
    fn record(&mut self, color_id: u8) {
        self.occurrences += 1;
        if self.colors.len() < 3 && !self.colors.contains(&color_id) {
            self.colors.push(color_id);
        }
    }

    fn is_triple(&self) -> bool {
        self.occurrences >= 3 || self.colors.len() >= 3
    }
}

pub(crate) fn quantize(point: V2) -> (i64, i64) {
    (
        (point.x * GRID_SCALE).round() as i64,
        (point.y * GRID_SCALE).round() as i64,
    )
}

/// Detect all topological triple-junction coordinates across a set of loops.
pub(crate) fn detect_triple_junctions<'a, I>(loops: I) -> HashSet<(i64, i64)>
where
    I: IntoIterator<Item = (u8, &'a [V2])>,
{
    let mut stats: HashMap<(i64, i64), JunctionStats> = HashMap::new();
    for (color_id, pts) in loops {
        for &point in pts {
            stats.entry(quantize(point)).or_default().record(color_id);
        }
    }
    stats
        .into_iter()
        .filter_map(|(key, entry)| entry.is_triple().then_some(key))
        .collect()
}

/// Build a per-vertex boolean mask of locked triple-junction anchors.
pub(crate) fn locked_mask(pts: &[V2], junctions: &HashSet<(i64, i64)>) -> Vec<bool> {
    pts.iter()
        .map(|&point| junctions.contains(&quantize(point)))
        .collect()
}
