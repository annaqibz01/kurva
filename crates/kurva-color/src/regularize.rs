use crate::prefilter::Oklab;
use crate::PaphqConfig;

#[derive(Clone, Copy, Debug)]
pub struct ComponentStats {
    pub area: u32,
    pub min_x: u16,
    pub max_x: u16,
    pub min_y: u16,
    pub max_y: u16,
    pub label: u8,
    pub target_label: u8,
}

pub struct RegularizerScratch {
    pub tmp_indices: Vec<u8>,
    pub parents: Vec<u32>,
    pub comp_id: Vec<u32>,
    pub stats: Vec<ComponentStats>,
    pub adj_counts: Vec<u32>,
}

impl RegularizerScratch {
    pub fn new(max_w: usize, max_h: usize, max_k: usize) -> Self {
        let cap = max_w * max_h;
        Self {
            tmp_indices: vec![0; cap],
            parents: vec![0; cap],
            comp_id: vec![0; cap],
            stats: Vec::with_capacity(cap / 4),
            adj_counts: vec![0; max_k],
        }
    }
}

pub fn regularize_indexed_raster(
    indices: &mut [u8],
    width: usize,
    height: usize,
    palette: &[Oklab],
    cfg: &PaphqConfig,
    scratch: &mut RegularizerScratch,
) {
    let n = width * height;
    if n == 0 || palette.is_empty() {
        return;
    }
    scratch.tmp_indices.resize(n, 0);

    // Pass 1: Corner-Safe & Thin-Line-Safe Spur Detangling
    detangle_spurs_corner_safe(indices, &mut scratch.tmp_indices, width, height);

    // Pass 2: 2-Sweep Elongation-Guarded Connected Component Micro-Island Absorption
    for _ in 0..2 {
        absorb_micro_islands(
            indices,
            width,
            height,
            palette,
            cfg.min_island_area,
            cfg.min_stroke_area,
            cfg.min_stroke_elongation,
            scratch,
        );
    }
}

/// Removes 1-px single-pixel spurs and isolated salt-and-pepper noise while strictly
/// preserving 90-degree corners (N4 >= 2) and 1-px continuous hatching lines (collinear N8).
fn detangle_spurs_corner_safe(indices: &mut [u8], tmp: &mut [u8], width: usize, height: usize) {
    if width < 3 || height < 3 {
        return;
    }
    tmp.copy_from_slice(indices);

    for y in 1..height.saturating_sub(1) {
        let row = y * width;
        for x in 1..width.saturating_sub(1) {
            let idx = row + x;
            let c = tmp[idx];

            let n = tmp[idx - width];
            let s = tmp[idx + width];
            let e = tmp[idx + 1];
            let w = tmp[idx - 1];

            let n4_same = (n == c) as u8 + (s == c) as u8 + (e == c) as u8 + (w == c) as u8;

            // Preserves interior pixels, straight edges, and 90-degree L-corner vertices!
            if n4_same >= 2 {
                continue;
            }

            let ne = tmp[idx - width + 1];
            let nw = tmp[idx - width - 1];
            let se = tmp[idx + width + 1];
            let sw = tmp[idx + width - 1];

            // Preserves 1-px-wide diagonal or orthogonal continuous hatching strokes!
            let is_collinear_line = (n == c && s == c)
                || (e == c && w == c)
                || (ne == c && sw == c)
                || (nw == c && se == c);
            if is_collinear_line {
                continue;
            }

            // Relabel spur/speckle to the dominant 4-connected neighbor
            let candidates = [n, s, e, w];
            let mut best_label = n;
            let mut best_count = 0u8;
            for &cand in &candidates {
                let cnt = (n == cand) as u8
                    + (s == cand) as u8
                    + (e == cand) as u8
                    + (w == cand) as u8;
                if cnt > best_count {
                    best_count = cnt;
                    best_label = cand;
                }
            }
            indices[idx] = best_label;
        }
    }
}

#[inline]
fn dsu_find(parents: &mut [u32], mut x: u32) -> u32 {
    while parents[x as usize] != x {
        let p = parents[x as usize];
        parents[x as usize] = parents[p as usize]; // Path halving
        x = p;
    }
    x
}

#[inline]
fn dsu_union(parents: &mut [u32], a: u32, b: u32) {
    let ra = dsu_find(parents, a);
    let rb = dsu_find(parents, b);
    if ra != rb {
        if ra < rb {
            parents[rb as usize] = ra;
        } else {
            parents[ra as usize] = rb;
        }
    }
}

fn absorb_micro_islands(
    indices: &mut [u8],
    width: usize,
    height: usize,
    palette: &[Oklab],
    min_island_area: u32,
    min_stroke_area: u32,
    min_stroke_elongation: f32,
    scratch: &mut RegularizerScratch,
) {
    let n = width * height;
    if n == 0 || palette.is_empty() {
        return;
    }
    let k = palette.len();
    scratch.parents.resize(n, 0);
    scratch.comp_id.resize(n, u32::MAX);
    scratch.comp_id.fill(u32::MAX); // Reset all IDs for multi-sweep safety
    scratch.stats.clear();

    for i in 0..n {
        scratch.parents[i] = i as u32;
    }

    // 1. 8-Connected Disjoint-Set Union so diagonal 1-px hatching lines form single components
    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let idx = row + x;
            let c = indices[idx];
            if x + 1 < width && indices[idx + 1] == c {
                dsu_union(&mut scratch.parents, idx as u32, (idx + 1) as u32);
            }
            if y + 1 < height {
                let next_row = idx + width;
                if indices[next_row] == c {
                    dsu_union(&mut scratch.parents, idx as u32, next_row as u32);
                }
                if x > 0 && indices[next_row - 1] == c {
                    dsu_union(&mut scratch.parents, idx as u32, (next_row - 1) as u32);
                }
                if x + 1 < width && indices[next_row + 1] == c {
                    dsu_union(&mut scratch.parents, idx as u32, (next_row + 1) as u32);
                }
            }
        }
    }

    // 2. Flatten roots and accumulate component Area + Bounding Box
    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let idx = row + x;
            let root = dsu_find(&mut scratch.parents, idx as u32) as usize;
            let cid = if scratch.comp_id[root] == u32::MAX {
                let new_id = scratch.stats.len() as u32;
                scratch.comp_id[root] = new_id;
                scratch.stats.push(ComponentStats {
                    area: 0,
                    min_x: x as u16,
                    max_x: x as u16,
                    min_y: y as u16,
                    max_y: y as u16,
                    label: indices[idx],
                    target_label: indices[idx],
                });
                new_id
            } else {
                scratch.comp_id[root]
            };

            scratch.comp_id[idx] = cid;
            let st = &mut scratch.stats[cid as usize];
            st.area += 1;
            st.min_x = st.min_x.min(x as u16);
            st.max_x = st.max_x.max(x as u16);
            st.min_y = st.min_y.min(y as u16);
            st.max_y = st.max_y.max(y as u16);
        }
    }

    // 3. Identify micro-islands vs protected elongated hatching strokes
    let num_comps = scratch.stats.len();
    scratch.adj_counts.resize(num_comps * k, 0);
    scratch.adj_counts.fill(0);

    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let idx = row + x;
            let cid = scratch.comp_id[idx] as usize;
            if scratch.stats[cid].area >= min_island_area {
                continue;
            }
            let c_label = indices[idx];
            let neighbors = [
                y.saturating_sub(1) * width + x,
                (y + 1).min(height - 1) * width + x,
                row + x.saturating_sub(1),
                row + (x + 1).min(width - 1),
            ];
            for &n_idx in &neighbors {
                let n_label = indices[n_idx];
                if n_label != c_label && (n_label as usize) < k {
                    scratch.adj_counts[cid * k + (n_label as usize)] += 1;
                }
            }
        }
    }

    // 4. Resolve absorbing target label for non-exempt micro-islands
    for (cid, st) in scratch.stats.iter_mut().enumerate() {
        if st.area >= min_island_area {
            continue;
        }
        let dx = (st.max_x - st.min_x + 1) as f32;
        let dy = (st.max_y - st.min_y + 1) as f32;
        let max_span = dx.max(dy);
        let elongation = (max_span * max_span) / (st.area as f32);

        // Exempt legitimate 1-2 px hatching dashes from absorption
        if st.area >= min_stroke_area && elongation >= min_stroke_elongation {
            continue;
        }

        let adj = &scratch.adj_counts[cid * k..(cid + 1) * k];
        let own_color = palette.get(st.label as usize).copied().unwrap_or_default();
        let mut best_score = -1.0f32;
        let mut best_k = st.label;

        for (cand_k, &perim) in adj.iter().enumerate() {
            if perim == 0 || cand_k as u8 == st.label {
                continue;
            }
            let cand_color = palette.get(cand_k).copied().unwrap_or_default();
            let dist_sq = own_color.dist_sq(cand_color);
            let score = (perim as f32) / (dist_sq + 1e-3);
            if score > best_score {
                best_score = score;
                best_k = cand_k as u8;
            }
        }
        st.target_label = best_k;
    }

    // 5. Commit relabeling in-place
    for i in 0..n {
        let cid = scratch.comp_id[i] as usize;
        indices[i] = scratch.stats[cid].target_label;
    }
}
