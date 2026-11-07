use crate::ColorRgb;

#[derive(Clone, Copy, Debug, Default)]
pub struct Oklab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

impl Oklab {
    #[inline]
    pub fn dist_sq(self, other: Self) -> f32 {
        let dl = self.l - other.l;
        let da = self.a - other.a;
        let db = self.b - other.b;
        dl * dl + da * da + db * db
    }
}

pub struct OklabImage {
    pub width: usize,
    pub height: usize,
    pub l: Vec<f32>,
    pub a: Vec<f32>,
    pub b: Vec<f32>,
}

impl OklabImage {
    pub fn new(max_w: usize, max_h: usize) -> Self {
        let cap = max_w * max_h;
        Self {
            width: max_w,
            height: max_h,
            l: vec![0.0; cap],
            a: vec![0.0; cap],
            b: vec![0.0; cap],
        }
    }

    pub fn resize_and_convert_srgb(&mut self, rgb: &[u8], width: usize, height: usize) {
        let n = width * height;
        self.width = width;
        self.height = height;
        self.l.resize(n, 0.0);
        self.a.resize(n, 0.0);
        self.b.resize(n, 0.0);

        for i in 0..n {
            let c = srgb_to_oklab(rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]);
            self.l[i] = c.l;
            self.a[i] = c.a;
            self.b[i] = c.b;
        }
    }
}

pub struct PreFilterScratch {
    pub tmp_h: Vec<f32>,
    pub mean_l: Vec<f32>,
    pub mean_a: Vec<f32>,
    pub mean_b: Vec<f32>,
    pub var_coupled: Vec<f32>,
    pub coeff_a: Vec<f32>,
    pub coeff_b_l: Vec<f32>,
    pub coeff_b_a: Vec<f32>,
    pub coeff_b_b: Vec<f32>,
}

impl PreFilterScratch {
    pub fn new(max_w: usize, max_h: usize) -> Self {
        let cap = max_w * max_h;
        Self {
            tmp_h: vec![0.0; cap],
            mean_l: vec![0.0; cap],
            mean_a: vec![0.0; cap],
            mean_b: vec![0.0; cap],
            var_coupled: vec![0.0; cap],
            coeff_a: vec![0.0; cap],
            coeff_b_l: vec![0.0; cap],
            coeff_b_a: vec![0.0; cap],
            coeff_b_b: vec![0.0; cap],
        }
    }

    fn ensure_len(&mut self, n: usize) {
        self.tmp_h.resize(n, 0.0);
        self.mean_l.resize(n, 0.0);
        self.mean_a.resize(n, 0.0);
        self.mean_b.resize(n, 0.0);
        self.var_coupled.resize(n, 0.0);
        self.coeff_a.resize(n, 0.0);
        self.coeff_b_l.resize(n, 0.0);
        self.coeff_b_a.resize(n, 0.0);
        self.coeff_b_b.resize(n, 0.0);
    }
}

#[inline]
pub fn srgb_to_linear(u: u8) -> f32 {
    let x = (u as f32) * (1.0 / 255.0);
    if x <= 0.04045 {
        x * (1.0 / 12.92)
    } else {
        ((x + 0.055) * (1.0 / 1.055)).powf(2.4)
    }
}

#[inline]
pub fn srgb_to_oklab(r: u8, g: u8, b: u8) -> Oklab {
    let lr = srgb_to_linear(r);
    let lg = srgb_to_linear(g);
    let lb = srgb_to_linear(b);

    let l = (0.41222147 * lr + 0.53633254 * lg + 0.05144599 * lb).cbrt();
    let m = (0.21190350 * lr + 0.68069955 * lg + 0.10739696 * lb).cbrt();
    let s = (0.08830246 * lr + 0.28171884 * lg + 0.62997870 * lb).cbrt();

    Oklab {
        l: 0.21045426 * l + 0.79361779 * m - 0.00407205 * s,
        a: 1.9779985 * l - 2.4285922 * m + 0.4505937 * s,
        b: 0.02590404 * l + 0.78277177 * m - 0.80867577 * s,
    }
}

pub fn palette_to_oklab(palette: &[ColorRgb]) -> Vec<Oklab> {
    palette
        .iter()
        .map(|c| srgb_to_oklab(c.r, c.g, c.b))
        .collect()
}

/// Strictly O(N) separable sliding-window box filter without Integral Image precision loss.
pub fn box_filter_separable(
    src: &[f32],
    dst: &mut [f32],
    tmp: &mut [f32],
    width: usize,
    height: usize,
    radius: usize,
) {
    if radius == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let scale = 1.0 / ((2 * radius + 1) as f32);

    // Pass 1: Horizontal sliding window
    for y in 0..height {
        let row_off = y * width;
        let row_src = &src[row_off..row_off + width];
        let row_tmp = &mut tmp[row_off..row_off + width];

        let mut sum = row_src[0] * ((radius + 1) as f32);
        for dx in 1..=radius {
            sum += row_src[dx.min(width - 1)];
        }
        for x in 0..width {
            row_tmp[x] = sum * scale;
            let out_idx = x.saturating_sub(radius);
            let in_idx = (x + radius + 1).min(width - 1);
            sum += row_src[in_idx] - row_src[out_idx];
        }
    }

    // Pass 2: Vertical sliding window
    for x in 0..width {
        let mut sum = tmp[x] * ((radius + 1) as f32);
        for dy in 1..=radius {
            sum += tmp[dy.min(height - 1) * width + x];
        }
        for y in 0..height {
            dst[y * width + x] = sum * scale;
            let out_y = y.saturating_sub(radius);
            let in_y = (y + radius + 1).min(height - 1);
            sum += tmp[in_y * width + x] - tmp[out_y * width + x];
        }
    }
}

pub fn run_prefilter(
    img: &mut OklabImage,
    scratch: &mut PreFilterScratch,
    radius: usize,
    eps: f32,
    shock_iters: usize,
    shock_threshold: f32,
) {
    let w = img.width;
    let h = img.height;
    let n = w * h;
    scratch.ensure_len(n);

    // 1. Compute local means for L, a, b
    box_filter_separable(
        &img.l,
        &mut scratch.mean_l,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );
    box_filter_separable(
        &img.a,
        &mut scratch.mean_a,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );
    box_filter_separable(
        &img.b,
        &mut scratch.mean_b,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );

    // 2. Compute coupled chroma-luminance squared energy E[L^2 + 2.5*(a^2 + b^2)]
    for i in 0..n {
        scratch.coeff_a[i] =
            img.l[i] * img.l[i] + 2.5 * (img.a[i] * img.a[i] + img.b[i] * img.b[i]);
    }
    box_filter_separable(
        &scratch.coeff_a,
        &mut scratch.var_coupled,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );

    // 3. Linear Guided Filter coefficients a_k, b_k
    for i in 0..n {
        let mu_energy = scratch.mean_l[i] * scratch.mean_l[i]
            + 2.5 * (scratch.mean_a[i] * scratch.mean_a[i] + scratch.mean_b[i] * scratch.mean_b[i]);
        let var = (scratch.var_coupled[i] - mu_energy).max(0.0);
        let ak = var / (var + eps);
        scratch.coeff_a[i] = ak;
        let one_minus_a = 1.0 - ak;
        scratch.coeff_b_l[i] = one_minus_a * scratch.mean_l[i];
        scratch.coeff_b_a[i] = one_minus_a * scratch.mean_a[i];
        scratch.coeff_b_b[i] = one_minus_a * scratch.mean_b[i];
    }

    // 4. Average linear coefficients over window omega_k
    box_filter_separable(
        &scratch.coeff_a,
        &mut scratch.var_coupled,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );
    box_filter_separable(
        &scratch.coeff_b_l,
        &mut scratch.mean_l,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );
    box_filter_separable(
        &scratch.coeff_b_a,
        &mut scratch.mean_a,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );
    box_filter_separable(
        &scratch.coeff_b_b,
        &mut scratch.mean_b,
        &mut scratch.tmp_h,
        w,
        h,
        radius,
    );

    for i in 0..n {
        let mean_a = scratch.var_coupled[i];
        img.l[i] = mean_a * img.l[i] + scratch.mean_l[i];
        img.a[i] = mean_a * img.a[i] + scratch.mean_a[i];
        img.b[i] = mean_a * img.b[i] + scratch.mean_b[i];
    }

    // 5. Coupled Upwind Morphological Shock Filter (Collapses 1-3 px AA halos)
    for _ in 0..shock_iters {
        scratch.mean_l.copy_from_slice(&img.l);
        scratch.mean_a.copy_from_slice(&img.a);
        scratch.mean_b.copy_from_slice(&img.b);

        for y in 1..h.saturating_sub(1) {
            let row = y * w;
            for x in 1..w.saturating_sub(1) {
                let idx = row + x;
                let l_c = scratch.mean_l[idx];
                let l_e = scratch.mean_l[idx + 1];
                let l_w = scratch.mean_l[idx - 1];
                let l_s = scratch.mean_l[idx + w];
                let l_n = scratch.mean_l[idx - w];

                let lx = 0.5 * (l_e - l_w);
                let ly = 0.5 * (l_s - l_n);
                let grad_sq = lx * lx + ly * ly;
                if grad_sq < 1e-5 {
                    continue;
                }

                let lxx = l_e - 2.0 * l_c + l_w;
                let lyy = l_s - 2.0 * l_c + l_n;
                let lxy = 0.25
                    * (scratch.mean_l[idx + w + 1]
                        - scratch.mean_l[idx + w - 1]
                        - scratch.mean_l[idx - w + 1]
                        + scratch.mean_l[idx - w - 1]);

                let l_eta_eta = (lx * lx * lxx + 2.0 * lx * ly * lxy + ly * ly * lyy) / grad_sq;

                if l_eta_eta.abs() > shock_threshold {
                    let pull_min = l_eta_eta > 0.0;
                    let mut best_idx = idx;
                    let mut best_l = l_c;

                    for dy in -1isize..=1 {
                        let ny = (y as isize + dy) as usize * w;
                        for dx in -1isize..=1 {
                            let n_idx = ny + (x as isize + dx) as usize;
                            let nl = scratch.mean_l[n_idx];
                            if (pull_min && nl < best_l) || (!pull_min && nl > best_l) {
                                best_l = nl;
                                best_idx = n_idx;
                            }
                        }
                    }
                    img.l[idx] = scratch.mean_l[best_idx];
                    img.a[idx] = scratch.mean_a[best_idx];
                    img.b[idx] = scratch.mean_b[best_idx];
                }
            }
        }
    }
}
