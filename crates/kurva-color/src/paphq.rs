use crate::prefilter::{Oklab, OklabImage};
use crate::PaphqConfig;

#[derive(Clone, Copy, Debug, Default)]
pub struct AnisoCell {
    pub d_xx: f32,
    pub d_yy: f32,
    pub d_xy: f32,
    pub lambda: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LevelDesc {
    pub width: usize,
    pub height: usize,
    pub pixel_offset: usize,
    pub phi_offset: usize,
}

/// Pre-allocated arena holding all multigrid levels in contiguous flat Vecs.
pub struct MultigridArena {
    pub levels: Vec<LevelDesc>,
    pub u_pyramid: Vec<Oklab>,
    pub tensor_pyramid: Vec<AnisoCell>,
    pub phi_pyramid: Vec<f32>,
}

impl MultigridArena {
    pub fn new(max_w: usize, max_h: usize, max_k: usize, max_levels: usize) -> Self {
        let mut cap_pixels = 0;
        let mut w = max_w;
        let mut h = max_h;
        for _ in 0..max_levels {
            cap_pixels += w * h;
            w = (w + 1) / 2;
            h = (h + 1) / 2;
        }
        Self {
            levels: Vec::with_capacity(max_levels),
            u_pyramid: vec![Oklab::default(); cap_pixels],
            tensor_pyramid: vec![AnisoCell::default(); cap_pixels],
            phi_pyramid: vec![0.0; cap_pixels * max_k],
        }
    }

    pub fn prepare_hierarchy(&mut self, width: usize, height: usize, k: usize, max_levels: usize) {
        self.levels.clear();
        let mut w = width;
        let mut h = height;
        let mut pix_off = 0;
        let mut phi_off = 0;

        for _ in 0..max_levels {
            let n = w * h;
            self.levels.push(LevelDesc {
                width: w,
                height: h,
                pixel_offset: pix_off,
                phi_offset: phi_off,
            });
            pix_off += n;
            phi_off += n * k;
            if w <= 16 || h <= 16 {
                break;
            }
            w = (w + 1) / 2;
            h = (h + 1) / 2;
        }

        self.u_pyramid.resize(pix_off, Oklab::default());
        self.tensor_pyramid.resize(pix_off, AnisoCell::default());
        self.phi_pyramid.resize(phi_off, 0.0);
    }
}

pub struct PaphqSolver;

impl PaphqSolver {
    pub fn solve(
        fine_img: &OklabImage,
        palette: &[Oklab],
        cfg: &PaphqConfig,
        arena: &mut MultigridArena,
        out_indices: &mut [u8],
    ) {
        let k = palette.len();
        arena.prepare_hierarchy(fine_img.width, fine_img.height, k, cfg.max_mg_levels);
        let num_levels = arena.levels.len();

        // Populate level 0 Oklab pixels
        let l0 = arena.levels[0];
        let n0 = l0.width * l0.height;
        for i in 0..n0 {
            arena.u_pyramid[i] = Oklab {
                l: fine_img.l[i],
                a: fine_img.a[i],
                b: fine_img.b[i],
            };
        }

        // Restrict Oklab image down the pyramid and compute anisotropic tensors per level
        for lvl in 0..num_levels {
            if lvl > 0 {
                let prev = arena.levels[lvl - 1];
                let curr = arena.levels[lvl];
                let (src_slice, dst_slice) = arena.u_pyramid.split_at_mut(curr.pixel_offset);
                let fine_u =
                    &src_slice[prev.pixel_offset..prev.pixel_offset + prev.width * prev.height];
                let coarse_u = &mut dst_slice[..curr.width * curr.height];
                Self::restrict_oklab(
                    fine_u,
                    prev.width,
                    prev.height,
                    coarse_u,
                    curr.width,
                    curr.height,
                );
            }
            let desc = arena.levels[lvl];
            let u_slice =
                &arena.u_pyramid[desc.pixel_offset..desc.pixel_offset + desc.width * desc.height];
            let t_slice = &mut arena.tensor_pyramid
                [desc.pixel_offset..desc.pixel_offset + desc.width * desc.height];
            Self::build_corner_gated_tensors(u_slice, t_slice, desc.width, desc.height, cfg);
        }

        // Initialize coarsest level via softmax-Boltzmann warm start
        let coarsest = arena.levels[num_levels - 1];
        Self::init_coarsest_phi(
            &arena.u_pyramid
                [coarsest.pixel_offset..coarsest.pixel_offset + coarsest.width * coarsest.height],
            palette,
            &mut arena.phi_pyramid
                [coarsest.phi_offset..coarsest.phi_offset + coarsest.width * coarsest.height * k],
        );

        // Full Multigrid (FMG) coarse-to-fine V-cycle progression
        for lvl in (0..num_levels).rev() {
            let desc = arena.levels[lvl];
            let n = desc.width * desc.height;
            let u_lvl = &arena.u_pyramid[desc.pixel_offset..desc.pixel_offset + n];
            let t_lvl = &arena.tensor_pyramid[desc.pixel_offset..desc.pixel_offset + n];

            {
                let phi_lvl = &mut arena.phi_pyramid[desc.phi_offset..desc.phi_offset + n * k];
                for _ in 0..cfg.v_cycle_sweeps {
                    Self::gauss_seidel_aniso_sweep(
                        phi_lvl,
                        u_lvl,
                        t_lvl,
                        palette,
                        desc.width,
                        desc.height,
                        cfg.epsilon,
                    );
                }
            }

            if lvl > 0 {
                let fine_desc = arena.levels[lvl - 1];
                let (fine_part, coarse_part) = arena.phi_pyramid.split_at_mut(desc.phi_offset);
                let fine_phi = &mut fine_part[fine_desc.phi_offset
                    ..fine_desc.phi_offset + fine_desc.width * fine_desc.height * k];
                let coarse_phi = &coarse_part[..n * k];
                let fine_u = &arena.u_pyramid[fine_desc.pixel_offset
                    ..fine_desc.pixel_offset + fine_desc.width * fine_desc.height];
                let fine_t = &arena.tensor_pyramid[fine_desc.pixel_offset
                    ..fine_desc.pixel_offset + fine_desc.width * fine_desc.height];

                Self::prolongate_with_ridge_anchoring(
                    coarse_phi,
                    desc.width,
                    desc.height,
                    fine_phi,
                    fine_u,
                    fine_t,
                    palette,
                    fine_desc.width,
                    fine_desc.height,
                    cfg.lambda_base,
                );
            }
        }

        // Discretize finest level phi -> u8 index
        let fine_phi = &arena.phi_pyramid[l0.phi_offset..l0.phi_offset + n0 * k];
        for i in 0..n0 {
            let row = &fine_phi[i * k..(i + 1) * k];
            let mut best_k = 0u8;
            let mut best_val = row[0];
            for (idx, &val) in row.iter().enumerate().skip(1) {
                if val > best_val {
                    best_val = val;
                    best_k = idx as u8;
                }
            }
            out_indices[i] = best_k;
        }
    }

    fn build_corner_gated_tensors(
        u: &[Oklab],
        tensors: &mut [AnisoCell],
        w: usize,
        h: usize,
        cfg: &PaphqConfig,
    ) {
        let inv_k2 = 1.0 / (cfg.kappa_normal * cfg.kappa_normal);
        let inv_kc2 = 1.0 / (cfg.kappa_corner * cfg.kappa_corner);
        let k2 = cfg.kappa_normal * cfg.kappa_normal;

        for y in 0..h {
            let ym1 = y.saturating_sub(1) * w;
            let y0 = y * w;
            let yp1 = (y + 1).min(h - 1) * w;

            for x in 0..w {
                let xm1 = x.saturating_sub(1);
                let xp1 = (x + 1).min(w - 1);

                // 3x3 smoothed Structure Tensor J_rho in Oklab
                let mut j_xx = 0.0f32;
                let mut j_yy = 0.0f32;
                let mut j_xy = 0.0f32;

                for &ry in &[ym1, y0, yp1] {
                    let u_e = u[ry + xp1];
                    let u_w = u[ry + xm1];
                    let dl_x = 0.5 * (u_e.l - u_w.l);
                    let da_x = 0.5 * (u_e.a - u_w.a);
                    let db_x = 0.5 * (u_e.b - u_w.b);

                    let u_s = u[yp1 + x];
                    let u_n = u[ym1 + x];
                    let dl_y = 0.5 * (u_s.l - u_n.l);
                    let da_y = 0.5 * (u_s.a - u_n.a);
                    let db_y = 0.5 * (u_s.b - u_n.b);

                    j_xx += dl_x * dl_x + 2.5 * (da_x * da_x + db_x * db_x);
                    j_yy += dl_y * dl_y + 2.5 * (da_y * da_y + db_y * db_y);
                    j_xy += dl_x * dl_y + 2.5 * (da_x * da_y + db_x * db_y);
                }
                j_xx *= 1.0 / 3.0;
                j_yy *= 1.0 / 3.0;
                j_xy *= 1.0 / 3.0;

                // Eigenvalues mu1 >= mu2 >= 0
                let diff = j_xx - j_yy;
                let disc = (diff * diff + 4.0 * j_xy * j_xy).sqrt();
                let mu1 = 0.5 * (j_xx + j_yy + disc);
                let mu2 = 0.5 * (j_xx + j_yy - disc).max(0.0);

                // Eigenvector v1 (across-edge normal)
                let mut v1x = j_xy;
                let mut v1y = mu1 - j_xx;
                let norm = (v1x * v1x + v1y * v1y).sqrt();
                if norm > 1e-6 {
                    v1x /= norm;
                    v1y /= norm;
                } else {
                    v1x = 1.0;
                    v1y = 0.0;
                }

                // Across-edge diffusivity d1 & Corner-Gated Along-edge diffusivity d2
                let r1 = mu1 * inv_k2;
                let r2 = mu2 * inv_kc2;
                let d1 = (1.0 / (1.0 + r1 * r1)).max(1e-3);
                let d2 = (1.0 / (1.0 + r2 * r2)).max(1e-3);

                let d_xx = d1 * v1x * v1x + d2 * v1y * v1y;
                let d_yy = d1 * v1y * v1y + d2 * v1x * v1x;
                let d_xy = (d1 - d2) * v1x * v1y;

                // Ridge-amplified fidelity weight lambda(x)
                let ridge_aniso = (mu1 - mu2) / (mu1 + mu2 + k2);
                let lambda = cfg.lambda_base * (1.0 + cfg.beta_ridge * ridge_aniso);

                tensors[y0 + x] = AnisoCell {
                    d_xx,
                    d_yy,
                    d_xy,
                    lambda,
                };
            }
        }
    }

    fn gauss_seidel_aniso_sweep(
        phi: &mut [f32],
        u: &[Oklab],
        t: &[AnisoCell],
        palette: &[Oklab],
        w: usize,
        h: usize,
        eps: f32,
    ) {
        let k = palette.len();
        let inv_eps = 1.0 / eps;

        for y in 0..h {
            let ym1 = y.saturating_sub(1);
            let yp1 = (y + 1).min(h - 1);

            for x in 0..w {
                let xm1 = x.saturating_sub(1);
                let xp1 = (x + 1).min(w - 1);

                let idx = y * w + x;
                let idx_e = y * w + xp1;
                let idx_w = y * w + xm1;
                let idx_s = yp1 * w + x;
                let idx_n = ym1 * w + x;

                let idx_se = yp1 * w + xp1;
                let idx_ne = ym1 * w + xp1;
                let idx_sw = yp1 * w + xm1;
                let idx_nw = ym1 * w + xm1;

                let tc = t[idx];
                let d_e = 0.5 * (tc.d_xx + t[idx_e].d_xx);
                let d_w = 0.5 * (tc.d_xx + t[idx_w].d_xx);
                let d_s = 0.5 * (tc.d_yy + t[idx_s].d_yy);
                let d_n = 0.5 * (tc.d_yy + t[idx_n].d_yy);
                let sigma = d_e + d_w + d_s + d_n;

                let c_se = 0.25 * (t[idx_e].d_xy + t[idx_s].d_xy);
                let c_ne = 0.25 * (t[idx_e].d_xy + t[idx_n].d_xy);
                let c_sw = 0.25 * (t[idx_w].d_xy + t[idx_s].d_xy);
                let c_nw = 0.25 * (t[idx_w].d_xy + t[idx_n].d_xy);

                let u_px = u[idx];
                let lam = tc.lambda;

                for c in 0..k {
                    let p_old = phi[idx * k + c];
                    let ortho = d_e * phi[idx_e * k + c]
                        + d_w * phi[idx_w * k + c]
                        + d_s * phi[idx_s * k + c]
                        + d_n * phi[idx_n * k + c];

                    let cross = c_se * phi[idx_se * k + c]
                        - c_ne * phi[idx_ne * k + c]
                        - c_sw * phi[idx_sw * k + c]
                        + c_nw * phi[idx_nw * k + c];

                    let well_rhs = inv_eps * (6.0 * p_old * p_old - 4.0 * p_old * p_old * p_old);
                    let numer = (eps * (ortho + cross) + well_rhs).max(0.0);
                    let denom = eps * sigma + 2.0 * inv_eps + 2.0 * lam * u_px.dist_sq(palette[c]);
                    phi[idx * k + c] = numer / denom;
                }

                // Euclidean projection onto probability simplex Delta^K
                Self::project_simplex(&mut phi[idx * k..(idx + 1) * k]);
            }
        }
    }

    #[inline]
    fn project_simplex(v: &mut [f32]) {
        let mut sum = 0.0f32;
        for x in v.iter_mut() {
            *x = x.max(0.0);
            sum += *x;
        }
        if sum > 1e-6 {
            let inv = 1.0 / sum;
            for x in v.iter_mut() {
                *x *= inv;
            }
        } else {
            let unif = 1.0 / (v.len() as f32);
            v.fill(unif);
        }
    }

    fn restrict_oklab(
        fine: &[Oklab],
        fw: usize,
        fh: usize,
        coarse: &mut [Oklab],
        cw: usize,
        ch: usize,
    ) {
        for cy in 0..ch {
            let fy0 = (cy * 2).min(fh - 1);
            let fy1 = (cy * 2 + 1).min(fh - 1);
            for cx in 0..cw {
                let fx0 = (cx * 2).min(fw - 1);
                let fx1 = (cx * 2 + 1).min(fw - 1);
                let p00 = fine[fy0 * fw + fx0];
                let p10 = fine[fy0 * fw + fx1];
                let p01 = fine[fy1 * fw + fx0];
                let p11 = fine[fy1 * fw + fx1];
                coarse[cy * cw + cx] = Oklab {
                    l: 0.25 * (p00.l + p10.l + p01.l + p11.l),
                    a: 0.25 * (p00.a + p10.a + p01.a + p11.a),
                    b: 0.25 * (p00.b + p10.b + p01.b + p11.b),
                };
            }
        }
    }

    fn init_coarsest_phi(u: &[Oklab], palette: &[Oklab], phi: &mut [f32]) {
        let k = palette.len();
        for (i, &px) in u.iter().enumerate() {
            let row = &mut phi[i * k..(i + 1) * k];
            for (c, &pal) in palette.iter().enumerate() {
                row[c] = (-40.0 * px.dist_sq(pal)).exp();
            }
            Self::project_simplex(row);
        }
    }

    /// Prolongates coarse phi to fine grid, blending in fine-grid color fidelity on thin ridges
    /// so 1-2 px hatching lines lost on coarse grids immediately re-nucleate.
    fn prolongate_with_ridge_anchoring(
        coarse_phi: &[f32],
        cw: usize,
        ch: usize,
        fine_phi: &mut [f32],
        fine_u: &[Oklab],
        fine_t: &[AnisoCell],
        palette: &[Oklab],
        fw: usize,
        fh: usize,
        lambda_base: f32,
    ) {
        let k = palette.len();
        for fy in 0..fh {
            let cy = (fy / 2).min(ch - 1);
            for fx in 0..fw {
                let cx = (fx / 2).min(cw - 1);
                let f_idx = fy * fw + fx;
                let c_idx = cy * cw + cx;

                let ridge_strength = ((fine_t[f_idx].lambda / lambda_base) - 1.0).clamp(0.0, 1.0);
                let alpha_ridge = 0.35 * ridge_strength;
                let px = fine_u[f_idx];

                let f_row = &mut fine_phi[f_idx * k..(f_idx + 1) * k];
                let c_row = &coarse_phi[c_idx * k..(c_idx + 1) * k];

                for c in 0..k {
                    let local_likelihood = (-35.0 * px.dist_sq(palette[c])).exp();
                    f_row[c] = (1.0 - alpha_ridge) * c_row[c] + alpha_ridge * local_likelihood;
                }
                Self::project_simplex(f_row);
            }
        }
    }
}
