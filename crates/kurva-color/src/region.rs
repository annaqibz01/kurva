//! Connected-component pure interior region extraction over `FlatInterior` pixels.

use std::collections::VecDeque;

use kurva_types::ColorRgb;

use crate::gradient::PixelKind;

/// A connected component of perceptually uniform `FlatInterior` pixels.
#[derive(Debug, Clone)]
pub struct PureRegion {
    #[allow(dead_code)]
    pub id: usize,
    pub pixel_count: usize,
    pub mean_lab: [f32; 3],
    #[allow(dead_code)]
    pub mean_rgb: ColorRgb,
}

/// Extracted pure regions and per-pixel region membership.
pub struct RegionMap {
    pub regions: Vec<PureRegion>,
    #[allow(dead_code)]
    pub labels: Vec<Option<usize>>,
}

/// Tight Oklab perceptual distance squared tolerance for region growing.
const REGION_TOL_SQ: f32 = 0.035 * 0.035;

#[inline]
pub(crate) fn dist_sq_oklab(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dl = a[0] - b[0];
    let da = a[1] - b[1];
    let db = a[2] - b[2];
    dl * dl + da * da + db * db
}

/// Performs deterministic 4-connected flood-fill strictly over `FlatInterior` pixels.
///
/// `Transition` pixels are masked out so blurry anti-aliased edges never bridge
/// or contaminate pure interior regions.
pub fn extract(
    lab: &[[f32; 3]],
    rgb_bytes: &[u8],
    kinds: &[PixelKind],
    width: usize,
    height: usize,
) -> RegionMap {
    let total = width * height;
    let mut labels = vec![None; total];
    let mut regions = Vec::new();
    let mut queue = VecDeque::new();

    for start in 0..total {
        if kinds[start] != PixelKind::FlatInterior || labels[start].is_some() {
            continue;
        }

        let region_id = regions.len();
        let seed_lab = lab[start];
        labels[start] = Some(region_id);
        queue.push_back(start);

        let mut count = 0_usize;
        let mut sum_lab = [0.0_f64; 3];
        let mut sum_rgb = [0_u64; 3];

        while let Some(curr) = queue.pop_front() {
            count += 1;
            let c_lab = lab[curr];
            sum_lab[0] += f64::from(c_lab[0]);
            sum_lab[1] += f64::from(c_lab[1]);
            sum_lab[2] += f64::from(c_lab[2]);

            let base = curr * 3;
            sum_rgb[0] += u64::from(rgb_bytes[base]);
            sum_rgb[1] += u64::from(rgb_bytes[base + 1]);
            sum_rgb[2] += u64::from(rgb_bytes[base + 2]);

            let x = curr % width;
            let y = curr / width;
            let neighbors = [
                x.checked_sub(1).map(|nx| y * width + nx),
                (x + 1 < width).then(|| y * width + x + 1),
                y.checked_sub(1).map(|ny| ny * width + x),
                (y + 1 < height).then(|| (y + 1) * width + x),
            ];

            for n_opt in neighbors.into_iter().flatten() {
                if kinds[n_opt] == PixelKind::FlatInterior
                    && labels[n_opt].is_none()
                    && dist_sq_oklab(lab[n_opt], seed_lab) <= REGION_TOL_SQ
                    && dist_sq_oklab(lab[n_opt], c_lab) <= REGION_TOL_SQ
                {
                    labels[n_opt] = Some(region_id);
                    queue.push_back(n_opt);
                }
            }
        }

        let inv = 1.0 / count as f64;
        regions.push(PureRegion {
            id: region_id,
            pixel_count: count,
            mean_lab: [
                (sum_lab[0] * inv) as f32,
                (sum_lab[1] * inv) as f32,
                (sum_lab[2] * inv) as f32,
            ],
            mean_rgb: ColorRgb::new(
                (sum_rgb[0] / count as u64) as u8,
                (sum_rgb[1] / count as u64) as u8,
                (sum_rgb[2] / count as u64) as u8,
            ),
        });
    }

    RegionMap { regions, labels }
}
