//! Authored models (`docs/model-pipeline.md`): binary glTF files at
//! `models/<key>/model.glb` under the pack roots (`assets/models/` in a
//! checkout, `packs/models/` in an install), loaded once on first use.
//!
//! What the engine reads from a file: mesh nodes (each one a part, drawn
//! with its node's transform, so a moving part turns about its own pivot),
//! empty nodes (named points such as `grip_r` or `muzzle`), and per
//! material a base color with its texture and an emissive texture. The
//! emissive texture's color is the glow and its alpha the team mask (the
//! engine paints those texels in the team's color, shaded by the base color
//! texture's gray); a material named `team…` is team paint all over. A
//! material may also carry glTF's metallic-roughness texture (roughness in
//! green, metalness in blue), its occlusion texture (red) and a tangent-space
//! normal map, which takes the primitive's tangents (`TANGENT`).
//! Positions, normals, tangents and first texture coordinates must be floats
//! and textures PNG or JPEG. A model may carry one skin (glTF's `skins`):
//! its skinned mesh nodes bend with named bones (the joint nodes' names) by
//! their `JOINTS_0` and `WEIGHTS_0`, from the rest pose the inverse bind
//! matrices undo (`player_model::push_armor` poses an armor's undersuit). A model that's missing or fails to load is drawn the old
//! way, with the built-in parts, and the reason is logged once.
// The browser build draws no models, so most of this goes unused there.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]
use glam::{Mat4, Quat, Vec3};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// One drawable piece: a mesh node's primitive.
pub struct Part {
    pub name: String,
    /// The name of the scene root it hangs under (itself for a root): a map's
    /// model file holds one root per placed model.
    pub group: String,
    /// The node's transform in the model's frame (its pivot and rest pose).
    pub node: Mat4,
    /// Index range in `Model::indices`.
    pub start: u32,
    pub count: u32,
    pub material: usize,
    /// Bent by the model's skin (`Model::skin`), its node's transform unused.
    pub skinned: bool,
}

/// A model's bones: each joint's name (its node's) and the inverse of its
/// rest frame, in the model's frame.
pub struct Skin {
    pub joints: Vec<String>,
    pub inverse_bind: Vec<Mat4>,
}

/// The most bones a skin may have: a draw's palette is this many matrices.
pub const MAX_BONES: usize = 16;

pub struct Material {
    pub color: [f32; 4],
    /// Team paint all over (a material named `team…`).
    pub team: bool,
    pub emissive: [f32; 3],
    /// Image indices: base color (alpha: team mask) and glow.
    pub albedo: Option<usize>,
    pub glow: Option<usize>,
    /// Metalness and roughness factors (times the texture's blue and green
    /// when there is one). A file that gives neither, without a texture, is
    /// drawn as the models before them were: matte, not metal.
    pub metallic: f32,
    pub roughness: f32,
    /// Image indices: metallic-roughness (green, blue), occlusion (red) and
    /// the normal map.
    pub surface: Option<usize>,
    pub occlusion: Option<usize>,
    pub normal: Option<usize>,
}

/// Floats per vertex: position, normal, texture coordinate, tangent (xyz
/// and the bitangent's sign; all zero for a primitive without tangents),
/// and four skin joints (indices into `Skin::joints`) with their weights
/// (all zero for a rigid primitive).
pub const VERTEX_FLOATS: usize = 20;

/// An RGBA8 image.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Model {
    pub key: String,
    /// Interleaved position, normal, texture coordinate and tangent
    /// (`VERTEX_FLOATS`).
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    pub parts: Vec<Part>,
    pub materials: Vec<Material>,
    pub images: Vec<Image>,
    /// Named points (empty nodes) in the model's frame.
    pub points: Vec<(String, Mat4)>,
    pub skin: Option<Skin>,
}

impl Model {
    pub fn point(&self, name: &str) -> Option<Mat4> {
        self.points.iter().find(|(n, _)| n == name).map(|(_, m)| *m)
    }

    /// Parse a binary glTF file.
    pub fn parse(key: &str, glb: &[u8]) -> Result<Model, String> {
        let (json, bin) = chunks(glb)?;
        let doc: serde_json::Value = serde_json::from_slice(json).map_err(|e| format!("glTF JSON: {e}"))?;
        let arr = |name: &str| doc.get(name).and_then(|v| v.as_array()).map(Vec::as_slice).unwrap_or(&[]);
        let views = arr("bufferViews");
        let accessors = arr("accessors");
        let view_bytes = |i: usize| -> Result<(&[u8], usize), String> {
            let v = views.get(i).ok_or("bad buffer view")?;
            if v.get("buffer").and_then(|b| b.as_u64()).unwrap_or(0) != 0 { return Err("only the GLB buffer".into()); }
            let off = v.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            let len = v.get("byteLength").and_then(|x| x.as_u64()).ok_or("view length")? as usize;
            let stride = v.get("byteStride").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            Ok((bin.get(off..off + len).ok_or("view outside the buffer")?, stride))
        };
        // Float vectors of `n` components.
        let floats = |i: usize, n: usize| -> Result<Vec<f32>, String> {
            let a = accessors.get(i).ok_or("bad accessor")?;
            if a.get("componentType").and_then(|x| x.as_u64()) != Some(5126) { return Err("attributes must be floats".into()); }
            let count = a.get("count").and_then(|x| x.as_u64()).ok_or("accessor count")? as usize;
            let (bytes, stride) = view_bytes(a.get("bufferView").and_then(|x| x.as_u64()).ok_or("sparse accessor")? as usize)?;
            let off = a.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            let stride = if stride == 0 { 4 * n } else { stride };
            let mut out = Vec::with_capacity(count * n);
            for k in 0..count {
                for c in 0..n {
                    let at = off + k * stride + c * 4;
                    let b = bytes.get(at..at + 4).ok_or("accessor outside its view")?;
                    out.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                }
            }
            Ok(out)
        };
        // Unsigned integer vectors of `n` components (skin joints).
        let uints = |i: usize, n: usize| -> Result<Vec<u32>, String> {
            let a = accessors.get(i).ok_or("bad accessor")?;
            let count = a.get("count").and_then(|x| x.as_u64()).ok_or("accessor count")? as usize;
            let (bytes, stride) = view_bytes(a.get("bufferView").and_then(|x| x.as_u64()).ok_or("sparse accessor")? as usize)?;
            let off = a.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            let size = match a.get("componentType").and_then(|x| x.as_u64()) { Some(5121) => 1, Some(5123) => 2, _ => return Err("joints must be bytes or shorts".into()) };
            let stride = if stride == 0 { size * n } else { stride };
            let mut out = Vec::with_capacity(count * n);
            for k in 0..count {
                for c in 0..n {
                    let at = off + k * stride + c * size;
                    let b = bytes.get(at..at + size).ok_or("accessor outside its view")?;
                    out.push(if size == 1 { b[0] as u32 } else { u16::from_le_bytes([b[0], b[1]]) as u32 });
                }
            }
            Ok(out)
        };
        let index_list = |i: usize| -> Result<Vec<u32>, String> {
            let a = accessors.get(i).ok_or("bad accessor")?;
            let count = a.get("count").and_then(|x| x.as_u64()).ok_or("accessor count")? as usize;
            let (bytes, _) = view_bytes(a.get("bufferView").and_then(|x| x.as_u64()).ok_or("sparse accessor")? as usize)?;
            let off = a.get("byteOffset").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
            let size = match a.get("componentType").and_then(|x| x.as_u64()) { Some(5121) => 1, Some(5123) => 2, Some(5125) => 4, _ => return Err("index type".into()) };
            let bytes = bytes.get(off..off + count * size).ok_or("indices outside their view")?;
            Ok(bytes.chunks_exact(size).map(|b| match size { 1 => b[0] as u32, 2 => u16::from_le_bytes([b[0], b[1]]) as u32,
                _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }).collect())
        };

        // Images (PNG only) and the materials that use them.
        let mut images = Vec::new();
        for img in arr("images") {
            let (bytes, _) = view_bytes(img.get("bufferView").and_then(|x| x.as_u64()).ok_or("image outside the GLB")? as usize)?;
            images.push(match img.get("mimeType").and_then(|m| m.as_str()) {
                Some("image/png") => decode_png(bytes)?,
                Some("image/jpeg") => decode_jpeg(bytes)?,
                _ => return Err("textures must be PNG or JPEG".into()),
            });
        }
        let textures = arr("textures");
        let texture = |t: Option<&serde_json::Value>| -> Option<usize> {
            let i = t?.get("index")?.as_u64()? as usize;
            Some(textures.get(i)?.get("source")?.as_u64()? as usize).filter(|&s| s < images.len())
        };
        let mut materials: Vec<Material> = arr("materials").iter().map(|m| {
            let pbr = m.get("pbrMetallicRoughness");
            let color = pbr.and_then(|p| p.get("baseColorFactor")).and_then(|c| c.as_array())
                .map(|c| [0, 1, 2, 3].map(|k| c.get(k).and_then(|x| x.as_f64()).unwrap_or(1.0) as f32)).unwrap_or([1.0; 4]);
            let emissive = m.get("emissiveFactor").and_then(|c| c.as_array())
                .map(|c| [0, 1, 2].map(|k| c.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32)).unwrap_or([0.0; 3]);
            let team = m.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.starts_with("team"));
            let surface = texture(pbr.and_then(|p| p.get("metallicRoughnessTexture")));
            let factor = |k: &str| pbr.and_then(|p| p.get(k)).and_then(|x| x.as_f64()).map(|x| x as f32);
            let (metallic, roughness) = match (factor("metallicFactor"), factor("roughnessFactor"), surface) {
                (None, None, None) => (0.0, 0.8),
                (m, r, _) => (m.unwrap_or(1.0).clamp(0.0, 1.0), r.unwrap_or(1.0).clamp(0.0, 1.0)),
            };
            Material { color, team, emissive, albedo: texture(pbr.and_then(|p| p.get("baseColorTexture"))), glow: texture(m.get("emissiveTexture")),
                metallic, roughness, surface, occlusion: texture(m.get("occlusionTexture")), normal: texture(m.get("normalTexture")) }
        }).collect();
        // Primitives without a material get a plain one.
        let plain = materials.len();
        materials.push(Material { color: [0.6, 0.6, 0.6, 1.0], team: false, emissive: [0.0; 3], albedo: None, glow: None,
            metallic: 0.0, roughness: 0.8, surface: None, occlusion: None, normal: None });

        // Walk the default scene's nodes.
        let nodes = arr("nodes");
        let meshes = arr("meshes");
        let roots: Vec<usize> = doc.get("scenes").and_then(|s| s.as_array())
            .and_then(|s| s.get(doc.get("scene").and_then(|x| x.as_u64()).unwrap_or(0) as usize))
            .and_then(|s| s.get("nodes")).and_then(|n| n.as_array())
            .map(|n| n.iter().filter_map(|x| x.as_u64().map(|x| x as usize)).collect())
            .unwrap_or_else(|| (0..nodes.len()).collect());
        // The first skin: its joints' names and inverse bind matrices.
        let skin = match arr("skins").first() {
            Some(sk) => {
                let joints: Vec<String> = sk.get("joints").and_then(|j| j.as_array()).ok_or("skin joints")?.iter()
                    .map(|j| j.as_u64().and_then(|n| nodes.get(n as usize)).and_then(|n| n.get("name")).and_then(|n| n.as_str())
                        .map(str::to_string).ok_or("skin joint"))
                    .collect::<Result<_, _>>()?;
                if joints.len() > MAX_BONES { return Err(format!("a skin has at most {MAX_BONES} joints")); }
                let inverse_bind = match sk.get("inverseBindMatrices").and_then(|x| x.as_u64()) {
                    Some(i) => floats(i as usize, 16)?.chunks_exact(16).map(Mat4::from_cols_slice).collect(),
                    None => vec![Mat4::IDENTITY; joints.len()],
                };
                if inverse_bind.len() != joints.len() { return Err("inverse bind matrices".into()); }
                Some(Skin { joints, inverse_bind })
            }
            None => None,
        };
        let mut model = Model { key: key.to_string(), vertices: Vec::new(), indices: Vec::new(), parts: Vec::new(),
            materials, images, points: Vec::new(), skin: None };
        let mut stack: Vec<(usize, Mat4, u32, Option<String>)> = roots.into_iter().map(|n| (n, Mat4::IDENTITY, 0, None)).collect();
        while let Some((n, parent, depth, group)) = stack.pop() {
            if depth > 32 { return Err("nodes nested too deep".into()); }
            let node = nodes.get(n).ok_or("bad node")?;
            let world = parent * local_transform(node);
            let name = node.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let group = group.unwrap_or_else(|| name.clone());
            match node.get("mesh").and_then(|m| m.as_u64()) {
                Some(m) => {
                    let mesh = meshes.get(m as usize).ok_or("bad mesh")?;
                    let skinned = node.get("skin").is_some() && skin.is_some();
                    for prim in mesh.get("primitives").and_then(|p| p.as_array()).map(Vec::as_slice).unwrap_or(&[]) {
                        if prim.get("mode").and_then(|x| x.as_u64()).unwrap_or(4) != 4 { continue; }
                        let attrs = prim.get("attributes").ok_or("primitive attributes")?;
                        let attr = |k: &str| attrs.get(k).and_then(|x| x.as_u64()).map(|x| x as usize);
                        let pos = floats(attr("POSITION").ok_or("positions")?, 3)?;
                        let count = pos.len() / 3;
                        let normals = match attr("NORMAL") { Some(i) => floats(i, 3)?, None => return Err("normals".into()) };
                        let uv = match attr("TEXCOORD_0") { Some(i) => floats(i, 2)?, None => vec![0.0; count * 2] };
                        let tangents = match attr("TANGENT") { Some(i) => floats(i, 4)?, None => vec![0.0; count * 4] };
                        let (joints, weights) = match (skinned, attr("JOINTS_0"), attr("WEIGHTS_0")) {
                            (true, Some(j), Some(w)) => (uints(j, 4)?, floats(w, 4)?),
                            _ => (vec![0; count * 4], vec![0.0; count * 4]),
                        };
                        if normals.len() != count * 3 || uv.len() != count * 2 || tangents.len() != count * 4
                            || joints.len() != count * 4 || weights.len() != count * 4 {
                            return Err("attribute counts differ".into());
                        }
                        let bones = skin.as_ref().map_or(0, |s| s.joints.len()) as u32;
                        if joints.iter().zip(&weights).any(|(&j, &w)| w > 0.0 && j >= bones) { return Err("joint out of range".into()); }
                        let base = (model.vertices.len() / VERTEX_FLOATS) as u32;
                        for k in 0..count {
                            model.vertices.extend_from_slice(&pos[k * 3..k * 3 + 3]);
                            model.vertices.extend_from_slice(&normals[k * 3..k * 3 + 3]);
                            model.vertices.extend_from_slice(&uv[k * 2..k * 2 + 2]);
                            model.vertices.extend_from_slice(&tangents[k * 4..k * 4 + 4]);
                            model.vertices.extend(joints[k * 4..k * 4 + 4].iter().map(|&j| j as f32));
                            model.vertices.extend_from_slice(&weights[k * 4..k * 4 + 4]);
                        }
                        let idx = match prim.get("indices").and_then(|x| x.as_u64()) {
                            Some(i) => index_list(i as usize)?,
                            None => (0..count as u32).collect(),
                        };
                        if idx.iter().any(|&i| i as usize >= count) { return Err("index out of range".into()); }
                        let start = model.indices.len() as u32;
                        model.indices.extend(idx.iter().map(|i| i + base));
                        let material = prim.get("material").and_then(|x| x.as_u64()).map_or(plain, |m| m as usize).min(plain);
                        model.parts.push(Part { name: name.clone(), group: group.clone(), node: world, start, count: idx.len() as u32, material, skinned });
                    }
                }
                None => model.points.push((name, world)),
            }
            for c in node.get("children").and_then(|c| c.as_array()).map(Vec::as_slice).unwrap_or(&[]) {
                stack.push((c.as_u64().ok_or("bad child")? as usize, world, depth + 1, Some(group.clone())));
            }
        }
        if model.parts.is_empty() { return Err("no meshes".into()); }
        model.skin = skin;
        // Draw order follows the file.
        model.parts.reverse();
        Ok(model)
    }
}

fn chunks(glb: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let word = |at: usize| glb.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    if glb.get(0..4) != Some(b"glTF") || word(4) != Some(2) { return Err("not a binary glTF 2 file".into()); }
    let json_len = word(12).ok_or("truncated")?;
    if word(16) != Some(0x4E4F534A) { return Err("first chunk must be JSON".into()); }
    let json = glb.get(20..20 + json_len).ok_or("truncated JSON")?;
    let at = 20 + json_len;
    let bin = match (word(at), word(at + 4)) {
        (Some(len), Some(0x004E4942)) => glb.get(at + 8..at + 8 + len).ok_or("truncated buffer")?,
        _ => &[],
    };
    Ok((json, bin))
}

fn local_transform(node: &serde_json::Value) -> Mat4 {
    let nums = |k: &str, n: usize| node.get(k).and_then(|v| v.as_array())
        .filter(|a| a.len() == n).map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect::<Vec<_>>());
    if let Some(m) = nums("matrix", 16) { return Mat4::from_cols_slice(&m); }
    let t = nums("translation", 3).map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
    let r = nums("rotation", 4).map_or(Quat::IDENTITY, |v| Quat::from_xyzw(v[0], v[1], v[2], v[3]).normalize());
    let s = nums("scale", 3).map_or(Vec3::ONE, |v| Vec3::new(v[0], v[1], v[2]));
    Mat4::from_scale_rotation_translation(s, r, t)
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_png(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("PNG: {e}"))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("PNG: {e}"))?;
    let (w, h) = (info.width, info.height);
    if w > 4096 || h > 4096 { return Err("textures are at most 4096 pixels a side".into()); }
    let px = (w * h) as usize;
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3].chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..px * 2].chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf[..px].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG left unexpanded".into()),
    };
    Ok(Image { width: w, height: h, rgba })
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_jpeg(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(bytes));
    let pixels = decoder.decode().map_err(|e| format!("JPEG: {e:?}"))?;
    let (w, h) = decoder.dimensions().ok_or("JPEG size")?;
    if w > 4096 || h > 4096 { return Err("textures are at most 4096 pixels a side".into()); }
    let rgba = match pixels.len() / (w * h).max(1) {
        3 => pixels.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        1 => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        4 => pixels,
        _ => return Err("JPEG channels".into()),
    };
    Ok(Image { width: w as u32, height: h as u32, rgba })
}

#[cfg(target_arch = "wasm32")]
fn decode_png(_bytes: &[u8]) -> Result<Image, String> { Err("no models in the browser build".into()) }

#[cfg(target_arch = "wasm32")]
fn decode_jpeg(_bytes: &[u8]) -> Result<Image, String> { Err("no models in the browser build".into()) }

/// Loaded models: a key's number is its place here, fixed once loaded.
struct Table {
    keys: HashMap<String, Option<u16>>,
    models: Vec<&'static Model>,
}

fn table() -> &'static Mutex<Table> {
    static TABLE: OnceLock<Mutex<Table>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(Table { keys: HashMap::new(), models: Vec::new() }))
}

/// Whether authored models are drawn: on unless `PEAKRUNNER_MODELS=0`
/// (comparisons with the built-in parts), never in the browser build.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| !cfg!(target_arch = "wasm32") && std::env::var("PEAKRUNNER_MODELS").map_or(true, |v| v != "0"))
}

/// The model for `key` (`weapons/hunter`, `armor/large/chest`), loading it
/// on first use; `None` when there isn't one.
pub fn get(key: &str) -> Option<(u16, &'static Model)> {
    if !enabled() { return None; }
    cached(key, &|| load(key))
}

/// A map's authored models (its `models.glb`, `map_pack::ModelPlacement`),
/// loaded on first use; `None` for a map without them or whose file fails
/// to load (logged once), which then draws its stand-in geometry.
pub fn of_map(pack: &peakrunner_core::map_pack::MapPack) -> Option<(u16, &'static Model)> {
    if !enabled() || pack.manifest.models.is_empty() { return None; }
    let key = format!("maps/{}", pack.fingerprint);
    cached(&key, &|| {
        let bytes = pack.asset("models.glb").map_err(|e| log::warn!("map models {}: {e}", pack.manifest.name)).ok()?;
        Model::parse(&key, &bytes).map_err(|e| log::warn!("map models {}: {e}", pack.manifest.name)).ok()
    })
}

fn cached(key: &str, load: &dyn Fn() -> Option<Model>) -> Option<(u16, &'static Model)> {
    let mut t = table().lock().ok()?;
    if let Some(found) = t.keys.get(key) {
        return found.map(|id| (id, t.models[id as usize]));
    }
    let found = load().and_then(|m| {
        if t.models.len() >= u16::MAX as usize { return None; }
        t.models.push(Box::leak(Box::new(m)));
        Some((t.models.len() - 1) as u16)
    });
    t.keys.insert(key.to_string(), found);
    found.map(|id| (id, t.models[id as usize]))
}

/// A loaded model by number (`get` gave it out).
pub fn by_id(id: u16) -> Option<&'static Model> {
    table().lock().ok()?.models.get(id as usize).copied()
}

#[cfg(not(target_arch = "wasm32"))]
fn load(key: &str) -> Option<Model> {
    if key.split('/').any(|s| s.is_empty() || s == ".." || s.starts_with('.')) { return None; }
    let path = peakrunner_core::catalog::roots().into_iter().map(|r| r.join("models").join(key).join("model.glb")).find(|p| p.is_file())?;
    let bytes = std::fs::read(&path).map_err(|e| log::warn!("model {key}: {e}")).ok()?;
    Model::parse(key, &bytes).map_err(|e| log::warn!("model {key} ({}): {e}", path.display())).ok()
}

#[cfg(target_arch = "wasm32")]
fn load(_key: &str) -> Option<Model> { None }

/// Draws for every part of `model` placed by `frame`, painted `team` where
/// the texture's team mask says so, with glow scaled by `glow`. `pose` may
/// move named parts: it gets a part's name and returns a transform applied
/// about that part's pivot.
pub fn push(out: &mut Vec<crate::drawlist::ModelDraw>, id: u16, model: &Model, frame: Mat4, team: Vec3, glow: f32,
    pose: &dyn Fn(&str) -> Option<Mat4>) {
    for (k, part) in model.parts.iter().enumerate() {
        let node = match pose(&part.name) { Some(m) => part.node * m, None => part.node };
        out.push(crate::drawlist::ModelDraw { id, part: k as u16, model: frame * node, team, glow, paint: false, plates: 0.0, shine: 0.0, skin: crate::drawlist::RIGID });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal binary glTF: one triangle in a node named `body` under a
    /// translated root, an empty `muzzle`, one material.
    fn triangle_glb() -> Vec<u8> {
        let mut bin = Vec::new();
        for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] { for x in v { bin.extend_from_slice(&x.to_le_bytes()); } }
        for _ in 0..3 { for x in [0.0f32, 0.0, 1.0] { bin.extend_from_slice(&x.to_le_bytes()); } }
        for i in [0u16, 1, 2, 0] { bin.extend_from_slice(&i.to_le_bytes()); }
        let json = serde_json::json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "root", "translation": [0.0, 2.0, 0.0], "children": [1, 2]},
                {"name": "body", "mesh": 0}, {"name": "muzzle", "translation": [0.0, 0.0, -0.7]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2, "material": 0}]}],
            "materials": [{"name": "metal", "pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.25, 1.0, 1.0]}}],
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 36},
                {"buffer": 0, "byteOffset": 72, "byteLength": 6}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"}],
        });
        let mut json = serde_json::to_vec(&json).unwrap();
        while json.len() % 4 != 0 { json.push(b' '); }
        let mut glb = Vec::new();
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&((12 + 8 + json.len() + 8 + bin.len()) as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E4942u32.to_le_bytes());
        glb.extend_from_slice(&bin);
        glb
    }

    #[test]
    fn parses_parts_points_and_materials() {
        let m = Model::parse("test", &triangle_glb()).unwrap();
        assert_eq!(m.parts.len(), 1);
        assert_eq!(m.parts[0].name, "body");
        assert_eq!(m.parts[0].count, 3);
        assert_eq!(m.vertices.len(), 3 * VERTEX_FLOATS);
        assert_eq!(m.parts[0].node.transform_point3(Vec3::ZERO), Vec3::new(0.0, 2.0, 0.0));
        let muzzle = m.point("muzzle").unwrap().transform_point3(Vec3::ZERO);
        assert_eq!(muzzle, Vec3::new(0.0, 2.0, -0.7));
        let material = &m.materials[m.parts[0].material];
        assert_eq!(material.color, [0.5, 0.25, 1.0, 1.0]);
        // No factors and no texture: matte, as models were drawn before
        // they carried any.
        assert_eq!((material.metallic, material.roughness), (0.0, 0.8));
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        assert!(Model::parse("x", b"nope").is_err());
        let mut glb = triangle_glb();
        glb.truncate(40);
        assert!(Model::parse("x", &glb).is_err());
    }

    /// Every model shipped with the game loads and has what the engine
    /// places it by.
    #[test]
    fn shipped_models_load() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/models");
        let mut seen = 0;
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() { stack.push(p); continue; }
                if p.file_name().and_then(|n| n.to_str()) != Some("model.glb") { continue; }
                let key = p.parent().unwrap().strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                let m = Model::parse(&key, &std::fs::read(&p).unwrap()).unwrap_or_else(|e| panic!("{key}: {e}"));
                // A map structure (drawn from its map's models.glb) is ship-sized.
                let budget = if key.starts_with("structures/") { 80_000 } else { 40_000 };
                assert!(m.indices.len() / 3 <= budget, "{key}: {} triangles", m.indices.len() / 3);
                if key.starts_with("weapons/") {
                    assert!(m.point("grip_r").is_some() && m.point("muzzle").is_some(), "{key}: grip_r and muzzle");
                }
                seen += 1;
            }
        }
        let _ = seen;
    }
}
