//! Generated meshes for the people (`player_model`): a sculpted human head,
//! a dome (helmet shells, pauldrons) and a capsule (the undersuit's limbs).
//! Vertex layout matches `scene`'s meshes: position then normal, unit scale.
use glam::{Vec2, Vec3};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

fn gauss(x: f32, y: f32, sx: f32, sy: f32) -> f32 { (-(x / sx).powi(2) - (y / sy).powi(2)).exp() }
fn smooth(a: f32, b: f32, x: f32) -> f32 { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// Face features in the head's front plane (x right, y up; unit head is 1
/// tall, 0.8 wide), as how far each point stands forward of the skull.
fn relief(x: f32, y: f32) -> f32 {
    // Nose: a bridge between the eyes that grows to a tip, with a wider base.
    let t = ((0.06 - y) / 0.21).clamp(0.0, 1.0);
    let along = if y > 0.06 { gauss(0.0, y - 0.06, 1.0, 0.03) } else if y < -0.15 { gauss(0.0, y + 0.15, 1.0, 0.022) } else { 1.0 };
    let nose = (0.015 + 0.085 * t * t) * along * gauss(x, 0.0, 0.03 + 0.035 * t, 1.0)
        + 0.022 * gauss(x, y + 0.155, 0.075, 0.03);
    let brow = 0.035 * gauss(x, y - 0.10, 0.25, 0.035);
    let sockets = 0.05 * (gauss(x - 0.14, y - 0.025, 0.065, 0.045) + gauss(x + 0.14, y - 0.025, 0.065, 0.045));
    let cheeks = 0.022 * (gauss(x - 0.19, y + 0.09, 0.07, 0.06) + gauss(x + 0.19, y + 0.09, 0.07, 0.06));
    let lips = 0.03 * gauss(x, y + 0.265, 0.085, 0.022) + 0.012 * gauss(x, y + 0.24, 0.06, 0.012);
    let mouth = 0.014 * gauss(x, y + 0.272, 0.07, 0.006);
    let chin = 0.03 * gauss(x, y + 0.40, 0.09, 0.05);
    nose + brow - sockets + cheeks + lips - mouth + chin
}

/// How far forward the face reaches at height `y` on its midline: a human
/// profile from forehead to under the chin (0 above the hairline, where the
/// skull's curve takes over).
fn profile(y: f32) -> f32 {
    const KEYS: [(f32, f32); 9] = [(0.34, 0.0), (0.26, 0.37), (0.10, 0.405), (-0.05, 0.40), (-0.20, 0.39),
        (-0.30, 0.375), (-0.40, 0.355), (-0.44, 0.31), (-0.48, 0.0)];
    for w in KEYS.windows(2) {
        let ((y0, d0), (y1, d1)) = (w[0], w[1]);
        if y <= y0 && y >= y1 { return d0 + (d1 - d0) * smooth(y0, y1, y); }
    }
    0.0
}

/// A point of the unit head in direction `d` from its centre. -Z is the face.
fn head_point(d: Vec3) -> Vec3 {
    let mut p = Vec3::new(d.x * 0.40, d.y * 0.5, d.z * if d.z > 0.0 { 0.48 } else { 0.43 });
    // The jaw narrows to the chin, and the back of the lower head tucks in
    // above the neck.
    let low = smooth(0.1, -0.9, d.y);
    p.x *= 1.0 - 0.34 * low;
    if d.z > 0.0 { p.z *= 1.0 - 0.45 * low; }
    if d.z < 0.0 {
        // The face: forward to the profile, curving back toward the cheeks,
        // then the features on it.
        let mask = profile(p.y) - 1.1 * p.x * p.x;
        p.z = p.z.min(-mask);
        p.z -= relief(p.x, p.y) * smooth(0.2, 0.6, -d.z);
    }
    // Ears on the sides.
    let side = smooth(0.55, 0.85, d.x.abs());
    p.x += d.x.signum() * 0.07 * side * gauss(p.y + 0.01, p.z - 0.03, 0.08, 0.05);
    p
}

/// Where the face's surface is at front-plane coordinates (x, y) of the unit
/// head, for placing eyes, brows and lips on it.
pub fn face_point(x: f32, y: f32) -> Vec3 {
    // Solve for the direction whose sculpted point lands at (x, y).
    let mut d = Vec3::new(x / 0.40, y / 0.5, -1.0).normalize();
    for _ in 0..12 {
        let p = head_point(d);
        let err = Vec2::new(x - p.x, y - p.y);
        d = (d + Vec3::new(err.x / 0.40, err.y / 0.5, 0.0)).normalize();
        d.z = -d.z.abs();
    }
    head_point(d)
}

/// A grid of rings (each `lon + 1` points round the Y axis) stitched into
/// triangles wound outward: one triangle near the middle decides the winding
/// for all of them. Collapsed triangles at the poles are dropped.
fn stitch(points: &[Vec3], rings: usize, lon: usize) -> Vec<u16> {
    let s = lon + 1;
    let mut indices = Vec::new();
    for y in 0..rings - 1 {
        for x in 0..lon {
            let i = (y * s + x) as u16;
            let s = s as u16;
            indices.extend_from_slice(&[i, i + s, i + 1, i + 1, i + s, i + s + 1]);
        }
    }
    // A triangle on the middle ring decides: its normal must point away
    // from the axis.
    let mid = (rings - 1) / 2 * lon * 2 * 3;
    let tri = |t: &[u16]| (points[t[0] as usize], points[t[1] as usize], points[t[2] as usize]);
    let (a, b, c) = tri(&indices[mid..mid + 3]);
    let centre = (a + b + c) / 3.0;
    let flip = (b - a).cross(c - a).dot(Vec3::new(centre.x, 0.0, centre.z)) < 0.0;
    let mut kept = Vec::new();
    for t in indices.chunks_exact(3) {
        let (a, b, c) = tri(t);
        if (b - a).cross(c - a).length_squared() <= 1e-12 { continue; }
        kept.extend_from_slice(&if flip { [t[0], t[2], t[1]] } else { [t[0], t[1], t[2]] });
    }
    kept
}

/// Smooth normals from the triangles, welding coincident vertices (the
/// seam and the poles) so the shading has no seams.
fn with_normals(points: &[Vec3], indices: &[u16]) -> Vec<f32> {
    let mut acc: HashMap<[i32; 3], Vec3> = HashMap::new();
    let key = |p: Vec3| [(p.x * 1e4).round() as i32, (p.y * 1e4).round() as i32, (p.z * 1e4).round() as i32];
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (points[t[0] as usize], points[t[1] as usize], points[t[2] as usize]);
        let n = (b - a).cross(c - a);
        for p in [a, b, c] { *acc.entry(key(p)).or_default() += n; }
    }
    points.iter().flat_map(|&p| {
        let n = acc.get(&key(p)).copied().unwrap_or(p).normalize_or(Vec3::Y);
        [p.x, p.y, p.z, n.x, n.y, n.z]
    }).collect()
}

/// Unit directions on `rings` latitude rings from the top pole (theta 0) to
/// `theta_end`, seam at the back (+Z).
fn directions(thetas: &[f32], lon: usize) -> Vec<Vec3> {
    thetas.iter().flat_map(|&th| (0..=lon).map(move |x| {
        let ph = x as f32 / lon as f32 * TAU;
        Vec3::new(th.sin() * ph.sin(), th.cos(), th.sin() * ph.cos())
    })).collect()
}

/// The sculpted head: 1 tall (chin to crown), 0.8 wide, centred on the
/// origin, facing -Z.
pub fn head_mesh() -> (Vec<f32>, Vec<u16>) {
    let (lat, lon) = (30, 40);
    let thetas: Vec<f32> = (0..=lat).map(|y| y as f32 / lat as f32 * PI).collect();
    let points: Vec<Vec3> = directions(&thetas, lon).into_iter().map(head_point).collect();
    let indices = stitch(&points, thetas.len(), lon);
    (with_normals(&points, &indices), indices)
}

/// A unit hemisphere dome (radius 1, base on y = 0) with a flat underside.
pub fn dome_mesh() -> (Vec<f32>, Vec<u16>) {
    let (lat, lon) = (8, 24);
    let thetas: Vec<f32> = (0..=lat).map(|y| y as f32 / lat as f32 * PI * 0.5).collect();
    let dirs = directions(&thetas, lon);
    let indices = stitch(&dirs, thetas.len(), lon);
    let mut v: Vec<f32> = dirs.iter().flat_map(|d| [d.x, d.y, d.z, d.x, d.y, d.z]).collect();
    let mut idx = indices;
    let base = (v.len() / 6) as u16;
    v.extend_from_slice(&[0.0, 0.0, 0.0, 0.0, -1.0, 0.0]);
    for x in 0..=lon {
        let ph = x as f32 / lon as f32 * TAU;
        v.extend_from_slice(&[ph.sin(), 0.0, ph.cos(), 0.0, -1.0, 0.0]);
    }
    // Underside, facing down.
    for x in 0..lon as u16 {
        let (a, b) = (base + 1 + x, base + 2 + x);
        let (pa, pb) = (Vec3::from_slice(&v[a as usize * 6..a as usize * 6 + 3]), Vec3::from_slice(&v[b as usize * 6..b as usize * 6 + 3]));
        idx.extend_from_slice(&if pa.cross(pb).y < 0.0 { [base, a, b] } else { [base, b, a] });
    }
    (v, idx)
}

/// A unit capsule: 1 tall and 1 wide, rounded ends, along Y.
pub fn capsule_mesh() -> (Vec<f32>, Vec<u16>) {
    let (half, lon) = (6, 16);
    // Top cap's rings, then the bottom cap's: the two equator rings make the
    // straight middle.
    let top: Vec<f32> = (0..=half).map(|k| k as f32 / half as f32 * PI * 0.5).collect();
    let bottom: Vec<f32> = (0..=half).map(|k| PI * 0.5 + k as f32 / half as f32 * PI * 0.5).collect();
    let thetas: Vec<f32> = top.iter().chain(&bottom).copied().collect();
    let dirs = directions(&thetas, lon);
    let points: Vec<Vec3> = dirs.iter().enumerate().map(|(i, d)| {
        let shift = if i / (lon + 1) <= half { 0.25 } else { -0.25 };
        Vec3::new(d.x * 0.5, d.y * 0.25 + shift, d.z * 0.5)
    }).collect();
    let indices = stitch(&points, thetas.len(), lon);
    // Normals of the squashed caps: the ellipsoid's, (x, 2y, z).
    let v = points.iter().zip(&dirs).flat_map(|(p, d)| {
        let n = Vec3::new(d.x, d.y * 2.0, d.z).normalize_or(Vec3::Y);
        [p.x, p.y, p.z, n.x, n.y, n.z]
    }).collect();
    (v, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, (v, idx): &(Vec<f32>, Vec<u16>), outward_from: Vec3) {
        assert!(idx.iter().all(|&i| (i as usize) < v.len() / 6), "{name} indices");
        for t in idx.chunks_exact(3) {
            let p = |i: u16| Vec3::from_slice(&v[i as usize * 6..i as usize * 6 + 3]);
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let n = (b - a).cross(c - a);
            if n.length_squared() < 1e-12 { continue; }
            let centre = (a + b + c) / 3.0;
            assert!(n.dot(centre - outward_from) > -1e-6 || centre.y.abs() < 1e-6, "{name}: inward triangle at {centre}");
        }
        for vertex in v.chunks_exact(6) {
            assert!((Vec3::from_slice(&vertex[3..]).length() - 1.0).abs() < 1e-3, "{name}: unit normals");
        }
    }

    #[test]
    fn generated_meshes_are_closed_outward_and_unit_normal() {
        check("head", &head_mesh(), Vec3::ZERO);
        check("capsule", &capsule_mesh(), Vec3::ZERO);
        let (v, _) = dome_mesh();
        assert!(v.chunks_exact(6).all(|p| p[1] >= -1e-6 && p[1] <= 1.0 + 1e-6), "dome from 0 to 1");
        let (v, _) = capsule_mesh();
        let ys: Vec<f32> = v.chunks_exact(6).map(|p| p[1]).collect();
        assert!((ys.iter().cloned().fold(f32::MIN, f32::max) - 0.5).abs() < 1e-4);
        assert!((ys.iter().cloned().fold(f32::MAX, f32::min) + 0.5).abs() < 1e-4);
    }

    #[test]
    fn the_head_has_a_face() {
        // The nose stands out past the eyes and cheeks, the eyes sit in
        // sockets behind the brow, and the chin juts past the mouth line.
        let nose = face_point(0.0, -0.14);
        let eye = face_point(0.14, 0.025);
        let brow = face_point(0.14, 0.10);
        let cheek = face_point(0.2, -0.09);
        let mouth = face_point(0.0, -0.272);
        let chin = face_point(0.0, -0.40);
        assert!(nose.z < cheek.z - 0.08, "nose {nose} cheek {cheek}");
        assert!(eye.z > brow.z + 0.02, "eye {eye} brow {brow}");
        // In profile the chin sits just behind the lips, ahead of the hollow
        // under the lower lip.
        let hollow = face_point(0.0, -0.33);
        assert!(chin.z < mouth.z + 0.04 && chin.z < hollow.z, "chin {chin} mouth {mouth} hollow {hollow}");
        assert!((face_point(0.1, -0.2).x - 0.1).abs() < 0.01, "face_point lands on its coordinates");
    }
}
