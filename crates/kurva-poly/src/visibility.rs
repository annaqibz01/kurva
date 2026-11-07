//! Dual angular visibility cones and the tube-bounded shortcut graph.

use std::f64::consts::PI;

use crate::geometry::{wrap, EPS, V2};
use crate::view::LoopView;

/// Angular slack absorbing floating-point error at cone boundaries.
const ANGULAR_SLACK: f64 = 1e-12;

/// Admissible shortcut graph: `successors[i]` holds every `j > i` reachable from `i`.
pub(crate) struct ShortcutGraph {
    pub successors: Vec<Vec<usize>>,
}

/// An angular interval on the circle, stored as center plus half width.
struct Cone {
    center: f64,
    half: f64,
    empty: bool,
}

impl Cone {
    /// A fully open cone anchored on `bearing`, so the interval never straddles the
    /// `-pi` branch cut of `atan2` for edges that point exactly backwards.
    fn full(bearing: f64) -> Self {
        Self {
            center: bearing,
            half: PI,
            empty: false,
        }
    }

    /// Intersect the cone with the visibility wedge `[theta - half_angle, theta + half_angle]`.
    fn narrow(&mut self, theta: f64, half_angle: f64) {
        if self.empty {
            return;
        }
        let directed = self.center + wrap(theta - self.center);
        let low = (self.center - self.half).max(directed - half_angle);
        let high = (self.center + self.half).min(directed + half_angle);
        if low > high {
            self.empty = true;
            return;
        }
        self.center = 0.5 * (low + high);
        self.half = 0.5 * (high - low);
    }

    fn admits(&self, theta: f64) -> bool {
        !self.empty && wrap(theta - self.center).abs() <= self.half + ANGULAR_SLACK
    }
}

fn wedge_half_angle(reach: f64, tolerance: f64) -> f64 {
    (tolerance / reach).clamp(0.0, 1.0).asin()
}

/// Build `G_wedge` intersected with `G_tube`, plus the chords forced by pre-collapse.
///
/// Forward cones are swept with `j` ascending and backward cones with `i` descending,
/// so both intersections are maintained incrementally and the graph costs
/// `O(n * span)` cone work plus `O(n * span^2)` tube verification.
pub(crate) fn build(
    view: &LoopView,
    keep: &[bool],
    forced: &[(usize, usize)],
    span: usize,
) -> ShortcutGraph {
    let total = view.pts.len();
    let span = span.clamp(1, total.saturating_sub(1));
    let cusp_prefix = cusp_prefix_sums(view);
    let mut forward = vec![false; total * span];
    for i in 0..total {
        if !keep[i] || i + 1 >= total {
            continue;
        }
        let mut cone = Cone::full(view.pts[i + 1].sub(view.pts[i]).angle());
        for step in 1..=span {
            let j = i + step;
            if j >= total {
                break;
            }
            if step >= 2 {
                let k = j - 1;
                let reach = view.pts[k].distance(view.pts[i]);
                if reach > EPS {
                    let bearing = view.pts[k].sub(view.pts[i]).angle();
                    cone.narrow(bearing, wedge_half_angle(reach, view.eps[k]));
                }
            }
            if !keep[j] || cone.empty || spans_cusp(&cusp_prefix, i, j) {
                continue;
            }
            let chord = view.pts[j].sub(view.pts[i]);
            if chord.norm() <= EPS {
                continue;
            }
            if cone.admits(chord.angle()) {
                forward[i * span + step - 1] = true;
            }
        }
    }
    let mut successors = vec![Vec::new(); total];
    for j in 0..total {
        if !keep[j] || j == 0 {
            continue;
        }
        let mut cone = Cone::full(view.pts[j - 1].sub(view.pts[j]).angle());
        for step in 1..=span {
            if j < step {
                break;
            }
            let i = j - step;
            if step >= 2 {
                let k = i + 1;
                let reach = view.pts[k].distance(view.pts[j]);
                if reach > EPS {
                    let bearing = view.pts[k].sub(view.pts[j]).angle();
                    cone.narrow(bearing, wedge_half_angle(reach, view.eps[k]));
                }
            }
            if !keep[i] || !forward[i * span + step - 1] {
                continue;
            }
            let bearing = view.pts[i].sub(view.pts[j]).angle();
            if !cone.admits(bearing) || !tube_fits(view, i, j) {
                continue;
            }
            successors[i].push(j);
        }
    }
    for &(i, j) in forced {
        if i < j && j < total {
            successors[i].push(j);
        }
    }
    for list in &mut successors {
        list.sort_unstable();
        list.dedup();
    }
    ShortcutGraph { successors }
}

fn cusp_prefix_sums(view: &LoopView) -> Vec<usize> {
    let mut prefix = vec![0usize; view.pts.len() + 1];
    for index in 0..view.pts.len() {
        prefix[index + 1] = prefix[index] + usize::from(view.cusp[index]);
    }
    prefix
}

/// A shortcut may not bypass a pinned G0 cusp.
fn spans_cusp(prefix: &[usize], left: usize, right: usize) -> bool {
    prefix[right] - prefix[left + 1] > 0
}

fn tube_fits(view: &LoopView, left: usize, right: usize) -> bool {
    let a = view.pts[left];
    let b = view.pts[right];
    (left + 1..right).all(|k| V2::segment_distance(view.pts[k], a, b) <= view.eps[k])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cones_reject_sharply_off_axis_shortcuts() {
        let mut cone = Cone::full(0.0);
        cone.narrow(0.0, 0.1);
        assert!(cone.admits(0.05));
        assert!(!cone.admits(0.4));
        cone.narrow(1.0, 0.1);
        assert!(cone.empty);
    }

    #[test]
    fn wedge_half_angle_saturates_when_the_tolerance_exceeds_the_reach() {
        assert!((wedge_half_angle(1.0, 2.0) - PI * 0.5).abs() < 1e-12);
        assert!(wedge_half_angle(10.0, 1.0) > 0.09);
    }
}
