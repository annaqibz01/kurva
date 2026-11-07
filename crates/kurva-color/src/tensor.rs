//! Spatial structure tensor, coherence gating and the regularised diffusion tensor.

/// Edge-stopping strength along the dominant gradient direction.
pub const GAMMA: f32 = 0.05;
/// Regulariser keeping the coherence ratio finite on flat regions.
const COHERENCE_DELTA: f32 = 1e-5;
/// Standard deviation of the separable smoothing kernel.
const SIGMA: f32 = 1.5;
/// Half width of the smoothing kernel.
const KERNEL_RADIUS: isize = 2;

/// Symmetric 2x2 diffusion tensor stored by its independent components.
#[derive(Clone, Copy, Debug)]
pub struct DTensor {
    pub dxx: f32,
    pub dxy: f32,
    pub dyy: f32,
}

impl DTensor {
    /// Isotropic unit tensor, the degenerate case of a flat patch.
    pub const IDENTITY: Self = Self {
        dxx: 1.0,
        dxy: 0.0,
        dyy: 1.0,
    };

    /// Determinant of the symmetric tensor; positive for an SPD matrix.
    pub fn determinant(self) -> f32 {
        self.dxx * self.dyy - self.dxy * self.dxy
    }
}

/// Coherence of a structure tensor with eigenvalues `l1 >= l2 >= 0`.
pub fn coherence(l1: f32, l2: f32) -> f32 {
    let ratio = (l1 - l2) / (l1 + l2 + COHERENCE_DELTA);
    (ratio * ratio).clamp(0.0, 1.0)
}

/// Mirror-reflected index access so gradients stay defined on the border.
fn mirror(index: isize, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let n = len as isize;
    let mut i = index;
    while i < 0 || i >= n {
        i = if i < 0 { -i } else { 2 * n - 2 - i };
    }
    i as usize
}

/// Central difference gradient of one channel at `(x, y)` with boundary reflection.
fn gradient(channel: &[f32], w: usize, h: usize, x: usize, y: usize) -> (f32, f32) {
    let left = mirror(x as isize - 1, w);
    let right = mirror(x as isize + 1, w);
    let up = mirror(y as isize - 1, h);
    let down = mirror(y as isize + 1, h);
    let gx = 0.5 * (channel[y * w + right] - channel[y * w + left]);
    let gy = 0.5 * (channel[down * w + x] - channel[up * w + x]);
    (gx, gy)
}

/// Normalised separable Gaussian weights of the `5`-tap smoothing kernel.
fn gaussian_kernel() -> [f32; 5] {
    let mut kernel = [0.0f32; 5];
    let mut total = 0.0f32;
    for (tap, weight) in kernel.iter_mut().enumerate() {
        let offset = tap as f32 - KERNEL_RADIUS as f32;
        *weight = (-(offset * offset) / (2.0 * SIGMA * SIGMA)).exp();
        total += *weight;
    }
    for weight in kernel.iter_mut() {
        *weight /= total;
    }
    kernel
}

/// Separable Gaussian smoothing with mirrored borders.
fn smooth(field: &[f32], w: usize, h: usize) -> Vec<f32> {
    let kernel = gaussian_kernel();
    let mut horizontal = vec![0.0f32; field.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (tap, weight) in kernel.iter().enumerate() {
                let source = mirror(x as isize + tap as isize - KERNEL_RADIUS, w);
                acc += weight * field[y * w + source];
            }
            horizontal[y * w + x] = acc;
        }
    }
    let mut smoothed = vec![0.0f32; field.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (tap, weight) in kernel.iter().enumerate() {
                let source = mirror(y as isize + tap as isize - KERNEL_RADIUS, h);
                acc += weight * horizontal[source * w + x];
            }
            smoothed[y * w + x] = acc;
        }
    }
    smoothed
}

/// Eigenvalues `(l1, l2)` in descending order and the unit eigenvector of `l1`.
fn eigen(sxx: f32, sxy: f32, syy: f32) -> (f32, f32, (f32, f32)) {
    let trace = sxx + syy;
    let determinant = sxx * syy - sxy * sxy;
    let disc = (0.25 * trace * trace - determinant).max(0.0).sqrt();
    let l1 = 0.5 * trace + disc;
    let l2 = (0.5 * trace - disc).max(0.0);
    let v1 = if sxy.abs() > f32::EPSILON {
        let (x, y) = (sxy, l1 - sxx);
        let norm = (x * x + y * y).sqrt();
        if norm > f32::EPSILON {
            (x / norm, y / norm)
        } else {
            (1.0, 0.0)
        }
    } else if sxx >= syy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    (l1, l2, v1)
}

/// Regularised diffusion tensor of every pixel of an Oklab raster.
///
/// The tensor is symmetric positive definite by construction: its eigenvalues are
/// `1 - c + c * gamma` and `1 - c + c`, both bounded below by `gamma`.
pub fn diffusion_field(raster: &[[f32; 3]], w: usize, h: usize) -> Vec<DTensor> {
    let count = w * h;
    let mut planes = [
        vec![0.0f32; count],
        vec![0.0f32; count],
        vec![0.0f32; count],
    ];
    for (pixel, colour) in raster.iter().enumerate() {
        for (channel, value) in planes.iter_mut().zip(colour.iter()) {
            channel[pixel] = *value;
        }
    }
    let mut sxx = vec![0.0f32; count];
    let mut sxy = vec![0.0f32; count];
    let mut syy = vec![0.0f32; count];
    for y in 0..h {
        for x in 0..w {
            let pixel = y * w + x;
            for channel in planes.iter() {
                let (gx, gy) = gradient(channel, w, h, x, y);
                sxx[pixel] += gx * gx;
                sxy[pixel] += gx * gy;
                syy[pixel] += gy * gy;
            }
        }
    }
    let sxx = smooth(&sxx, w, h);
    let sxy = smooth(&sxy, w, h);
    let syy = smooth(&syy, w, h);
    let mut field = Vec::with_capacity(count);
    for pixel in 0..count {
        let (l1, l2, (v1x, v1y)) = eigen(sxx[pixel], sxy[pixel], syy[pixel]);
        let weight = coherence(l1, l2);
        let (v2x, v2y) = (-v1y, v1x);
        let dxx = (1.0 - weight) + weight * (GAMMA * v1x * v1x + v2x * v2x);
        let dyy = (1.0 - weight) + weight * (GAMMA * v1y * v1y + v2y * v2y);
        let dxy = weight * (GAMMA * v1x * v1y + v2x * v2y);
        let bound = dxx.min(dyy);
        field.push(DTensor {
            dxx,
            dxy: dxy.clamp(-bound, bound),
            dyy,
        });
    }
    field
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: usize, h: usize) -> Vec<[f32; 3]> {
        (0..w * h)
            .map(|pixel| {
                let level = (pixel % w) as f32 / w as f32;
                [level, level, level]
            })
            .collect()
    }

    #[test]
    fn tensor_spd_and_coherence() {
        let magnitude = coherence(3.0, 0.5);
        assert!((0.0..1.0).contains(&magnitude));
        assert_eq!(coherence(0.0, 0.0), 0.0);

        let field = diffusion_field(&ramp(12, 9), 12, 9);
        assert_eq!(field.len(), 108);
        for tensor in field {
            assert!(tensor.dxx > 0.0);
            assert!(tensor.dyy > 0.0);
            assert!(tensor.determinant() > 0.0);
            assert!(tensor.dxy.abs() <= tensor.dxx.min(tensor.dyy));
        }
    }

    #[test]
    fn flat_patch_stays_isotropic() {
        let flat = vec![[0.4f32, 0.1, -0.1]; 25];
        for tensor in diffusion_field(&flat, 5, 5) {
            assert!((tensor.dxx - 1.0).abs() < 1e-3);
            assert!((tensor.dyy - 1.0).abs() < 1e-3);
            assert!(tensor.dxy.abs() < 1e-3);
        }
    }
}
