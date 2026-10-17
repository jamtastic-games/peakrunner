//! Shared terrain-detail baker. No vegetation geometry or painted blades.
//! Macro material coverage belongs to each map's SurfaceStyle; this tile only
//! supplies isotropic soil/gravel/turf grain, with wrapping normal-map edges.

pub const TEX_SIZE: u32 = 256;

#[derive(Clone, Copy)]
pub struct SurfaceStyle {
    /// RGB + texture repeat in meters.
    pub cover: [f32; 4],
    /// RGB + macro patch size in meters.
    pub soil: [f32; 4],
    /// RGB + detail contrast.
    pub rock: [f32; 4],
    /// Rock slope start/end, normal strength, dry coverage threshold.
    pub rules: [f32; 4],
}

pub const HIGHLAND: SurfaceStyle = SurfaceStyle {
    cover: [0.30, 0.38, 0.13, 5.0],
    soil: [0.44, 0.31, 0.19, 45.0],
    rock: [0.43, 0.42, 0.36, 0.85],
    rules: [0.14, 0.46, 0.12, 0.48],
};

pub fn bake_textures() -> (Vec<u8>, Vec<u8>) { bake_sized(TEX_SIZE) }

pub fn bake_sized(size: u32) -> (Vec<u8>, Vec<u8>) {
    assert!(size.is_power_of_two() && size >= 16);
    let n = size as usize;
    let mut albedo = vec![0; n * n * 4];
    let mut heights = vec![0.0; n * n];
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 / n as f32;
            let v = y as f32 / n as f32;
            let coarse = tile_noise(u * 12.0, v * 12.0, 12);
            let grit = tile_noise(u * 64.0, v * 64.0, 64);
            let grain = hash(x as i32, y as i32);
            let value = (0.5 + (coarse - 0.5) * 0.42
                + (grit - 0.5) * 0.5 + (grain - 0.5) * 0.24).clamp(0.0, 1.0);
            let i = (y * n + x) * 4;
            let c = (value * 255.0) as u8;
            albedo[i..i + 4].copy_from_slice(&[c, c, c, 255]);
            heights[y * n + x] = value;
        }
    }
    let mut normals = vec![0; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let dx = heights[y * n + (x + 1) % n] - heights[y * n + (x + n - 1) % n];
            let dz = heights[((y + 1) % n) * n + x] - heights[((y + n - 1) % n) * n + x];
            let normal = glam::Vec3::new(-dx * 0.5, 1.0, -dz * 0.5).normalize();
            let i = (y * n + x) * 4;
            normals[i..i + 4].copy_from_slice(&[
                ((normal.x * 0.5 + 0.5) * 255.0) as u8,
                ((normal.y * 0.5 + 0.5) * 255.0) as u8,
                ((normal.z * 0.5 + 0.5) * 255.0) as u8, 255]);
        }
    }
    (albedo, normals)
}

fn hash(x: i32, y: i32) -> f32 {
    let mut n = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    n as u32 as f32 / u32::MAX as f32
}

fn tile_noise(x: f32, y: f32, period: i32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let fx = smooth(x - ix as f32);
    let fy = smooth(y - iy as f32);
    let h = |x: i32, y: i32| hash(x.rem_euclid(period), y.rem_euclid(period));
    let a = h(ix, iy);
    let b = h(ix + 1, iy);
    let c = h(ix, iy + 1);
    let d = h(ix + 1, iy + 1);
    a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detail_is_deterministic_gritty_and_tileable() {
        let (a, n) = bake_sized(64);
        assert_eq!(a, bake_sized(64).0);
        assert_eq!(a.len(), n.len());
        let mean = a.chunks_exact(4).map(|p| p[0] as f32).sum::<f32>() / 4096.0;
        let variance = a.chunks_exact(4).map(|p| (p[0] as f32 - mean).powi(2)).sum::<f32>() / 4096.0;
        assert!(variance > 100.0);
        assert!(n.chunks_exact(4).all(|p| p[1] > 220 && p[3] == 255));
        for y in [0.13, 2.4, 8.9] {
            assert!((tile_noise(0.0, y, 12) - tile_noise(12.0, y, 12)).abs() < 1e-6);
        }
    }
}
