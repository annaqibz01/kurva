//! Bilateral dynamic programming over the admissible shortcut graph.

use crate::geometry::EPS;
use crate::view::LoopView;
use crate::visibility::ShortcutGraph;

/// Weight of the accumulated curvature term in the MDL objective.
pub(crate) const LAMBDA: f64 = 0.5;

/// Prefix sums of `|kappa_sigma| / eps * ds`, so an edge cost is one subtraction.
pub(crate) fn prefix_bend(view: &LoopView) -> Vec<f64> {
    let total = view.pts.len();
    let mut out = vec![0.0; total];
    for m in 0..total.saturating_sub(1) {
        let step = view.arc[m + 1] - view.arc[m];
        out[m + 1] = out[m] + view.smoothed[m].abs() / view.eps[m].max(EPS) * step;
    }
    out
}

/// Bellman recurrence `d(j) = min over (i, j) of d(i) + w(i, j)` restricted to `start..=end`.
///
/// Edges only ever point forward, so a single ascending sweep visits nodes in
/// topological order and the backtrack yields the optimal index sequence.
pub(crate) fn solve_range(
    graph: &ShortcutGraph,
    bend: &[f64],
    start: usize,
    end: usize,
) -> Option<Vec<usize>> {
    if start >= end || end >= bend.len() {
        return None;
    }
    let mut dist = vec![f64::INFINITY; end + 1];
    let mut back = vec![usize::MAX; end + 1];
    dist[start] = 0.0;
    for i in start..end {
        if !dist[i].is_finite() {
            continue;
        }
        for &j in &graph.successors[i] {
            if j <= i {
                continue;
            }
            if j > end {
                break;
            }
            let weight = 1.0 + LAMBDA * (bend[j] - bend[i]);
            let candidate = dist[i] + weight;
            if candidate < dist[j] {
                dist[j] = candidate;
                back[j] = i;
            }
        }
    }
    if !dist[end].is_finite() {
        return None;
    }
    let mut path = vec![end];
    let mut cursor = end;
    while cursor != start {
        let previous = back[cursor];
        if previous == usize::MAX {
            return None;
        }
        path.push(previous);
        cursor = previous;
    }
    path.reverse();
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_of(n: usize, edges: &[(usize, usize)]) -> ShortcutGraph {
        let mut successors = vec![Vec::new(); n];
        for &(i, j) in edges {
            successors[i].push(j);
        }
        for list in &mut successors {
            list.sort_unstable();
        }
        ShortcutGraph { successors }
    }

    #[test]
    fn the_cheapest_path_prefers_a_single_hop() {
        // 0 -> 1 -> 2 -> 3 costs 3.0, while 0 -> 3 costs 1.0.
        let graph = graph_of(4, &[(0, 1), (1, 2), (2, 3), (0, 3)]);
        let bend = vec![0.0; 4];
        assert_eq!(solve_range(&graph, &bend, 0, 3), Some(vec![0, 3]));
    }

    #[test]
    fn a_range_never_leaves_its_endpoints() {
        // The 0 -> 3 shortcut exists but the requested range ends at 2.
        let graph = graph_of(4, &[(0, 1), (1, 2), (2, 3), (0, 3)]);
        let bend = vec![0.0; 4];
        assert_eq!(solve_range(&graph, &bend, 0, 2), Some(vec![0, 1, 2]));
    }

    #[test]
    fn unreachable_ranges_report_no_path() {
        let graph = graph_of(3, &[(0, 1)]);
        assert_eq!(solve_range(&graph, &[0.0; 3], 0, 2), None);
    }
}
