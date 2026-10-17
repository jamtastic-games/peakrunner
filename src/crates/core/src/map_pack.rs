//! Map packs: explicit data only, no script execution or downloads.
//! The same immutable, checked triangle data serves client and dedicated server.
use glam::Vec3;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, HashMap}, path::{Path, PathBuf}};

fn default_step()->f32 {8.0}
fn default_water()->bool {true}

#[derive(Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub name: String,
    #[serde(default="default_step")]
    pub terrain_step: f32,
    #[serde(default="default_water")]
    pub water_enabled: bool,
    #[serde(default)]
    pub exact_spawns: bool,
    /// A duel server's lobby (Tenfold Tower): it hosts the duel lobby and
    /// no other mode, so menus, rotations and votes never offer it.
    #[serde(default)]
    pub lobby: bool,
    pub version: u32,
    pub flags: [[f32; 3]; 2],
    pub spawns: [[f32; 3]; 2],
    pub holes: Vec<usize>,
    pub texture_count: u32,
    pub terrain_layers: [u32; 4],
    pub sky_layers: Vec<u32>,
    pub water_layer: u32,
    pub ambient_emitters: Vec<[f32; 6]>,
    pub sky: BTreeMap<String, String>,
    pub water: BTreeMap<String, String>,
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub entities: Vec<crate::equipment::Definition>,
    /// Optional per-team spawn list, `[x, y, z, yaw]` with the player's
    /// centre and world yaw. Absent: the single `spawns` entry per team.
    #[serde(default)]
    pub spawn_points: Vec<Vec<[f32; 4]>>,
    /// Optional render look (sun, ambient, exposure, height fog, procedural
    /// sky); see `crate::look`. Absent: the historical look.
    #[serde(default)]
    pub look: crate::look::Look,
    /// Optional Capture & Hold points; see `crate::control`.
    #[serde(default)]
    pub control_points: Vec<crate::control::Definition>,
    /// Optional water bodies that slow movement; see `crate::water`. The
    /// legacy `water` plane above stays render-only.
    #[serde(default)]
    pub water_volumes: Vec<crate::water::Volume>,
    /// Optional football field; see `crate::sim::football`. Only maps that
    /// declare one offer the Football mode.
    #[serde(default)]
    pub football: Option<crate::sim::football::Field>,
    /// Optional moving parts, zones, items and units; see
    /// `crate::sim::features`. Absent on the base game's maps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moveables: Vec<crate::sim::features::Moveable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<crate::sim::features::Zone>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<crate::sim::features::MapItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deployables: Vec<crate::sim::features::MapDeployable>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vents: Vec<crate::sim::features::Vent>,
    /// Optional duel spots (`sim::duel`): pairs of [x, y, z, yaw] a duel's
    /// round starts from, one player at each (platforms, ledges, peaks).
    /// A map with none has random pairs on open ground.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub duel_spots: Vec<[[f32; 4]; 2]>,
    /// Optional authored models drawn over the map's structures (render
    /// only, from the `models.glb` payload; collision stays in
    /// `collision.bin`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelPlacement>,
    /// The run of `vertices.bin` (first vertex, vertex count) that stands in
    /// for `models` where they aren't drawn: the browser build, or with
    /// `PEAKRUNNER_MODELS=0`, or a model file that fails to load.
    #[serde(default)]
    pub standin: Option<[u32; 2]>,
    /// Optional smoke sources, drawn only: [x, y, z, size] for a chimney's
    /// plume or a fire's smoke (size 0.1 to 8, about the plume's width in
    /// metres at its base).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub smoke: Vec<[f32; 4]>,
}

/// One authored model in a map: the node of `models.glb` named `model` (with
/// its children), placed at `position` and turned `yaw` degrees about +Y the
/// way the map builders turn structures, painted in `team`'s color where its
/// texture says (none: neutral gray).
#[derive(Clone, Debug, Deserialize)]
pub struct ModelPlacement {
    pub model: String,
    pub position: [f32; 3],
    #[serde(default)]
    pub yaw: f32,
    #[serde(default)]
    pub team: Option<u8>,
}

/// Upper bound on `models.glb`.
const MODELS_MAX_BYTES: u64 = 64_000_000;

pub struct MapPack {
    embedded: Option<Embedded>,
    pub manifest: Manifest,
    pub root: PathBuf,
    pub fingerprint: String,
    triangles: Vec<[Vec3; 3]>,
    /// Each triangle's bounding box (low, high): a sweep skips triangles
    /// whose box misses its own before the exact test. On a dense map most of
    /// a 32 m cell's triangles are metres from any one query.
    bounds: Vec<(Vec3, Vec3)>,
    /// Each collision triangle's unit normal (zero for a degenerate one).
    normals: Vec<Vec3>,
    buckets: HashMap<(i32, i32), Vec<usize>>,
    holes: Vec<bool>,
    pub heights: Vec<u8>,
    /// The optional `lava.bin` (empty when absent); see `lava_cell`.
    lava: Vec<u8>,
}

/// Lava lying over one terrain cell (`MapPack::lava_cell`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LavaCell {
    /// World height of its surface.
    pub surface: f32,
    /// Damage a second to whoever touches it (`water::lava_burn`).
    pub burn: f32,
    /// Ground like a lava volume: players run and ski across it, never
    /// sink, and shots burst on it. Otherwise it lies on the ground it
    /// covers and only burns.
    pub skim: bool,
}

/// Size of the optional `lava.bin`: a 4-byte record per terrain cell.
const LAVA_BYTES: usize = 256 * 256 * 4;

/// Inflate a zlib payload: a `<name>.zlib` file in a map folder, or (in the
/// browser build) a payload `crates/core/build.rs` compressed. Hashes are
/// always over the inflated bytes, so fingerprints don't depend on storage.
fn inflate(bytes: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(bytes, limit)
        .map_err(|e| format!("Corrupt compressed map asset: {:?}", e.status))
}

/// A browser build's embedded map: its payloads by file name.
pub(crate) type Embedded = fn(&str) -> Result<Vec<u8>, String>;

#[cfg(target_arch = "wasm32")]
macro_rules! embedded_map {
    ($func:ident, $dir:literal) => {
        fn $func(name: &str) -> Result<Vec<u8>, String> {
            macro_rules! packed {($file:literal) => {
                inflate(include_bytes!(concat!(env!("OUT_DIR"), "/maps/", $dir, "/", $file, ".zlib")), 128_000_000)
            }}
            match name {
                "map.json" => Ok(include_bytes!(concat!("../../../assets/maps/", $dir, "/map.json")).to_vec()),
                "vertices.bin" => packed!("vertices.bin"),
                "collision.bin" => packed!("collision.bin"),
                "height.bin" => packed!("height.bin"),
                "weights.rgba" => packed!("weights.rgba"),
                "textures.rgba" => packed!("textures.rgba"),
                "ambient.f32" => packed!("ambient.f32"),
                "shade.rg" => packed!("shade.rg"),
                "props.bin" => packed!("props.bin"),
                _ => Err(concat!("Unknown ", $dir, " asset").into()),
            }
        }
    };
}

/// The browser build has no folders: it embeds the default maps (payloads
/// compressed by `crates/core/build.rs`) with their `info.json`. Native builds
/// read every map from its folder (`crate::catalog`).
#[cfg(target_arch = "wasm32")]
pub(crate) fn embedded(key: &str) -> Option<(Embedded, &'static str)> {
    macro_rules! maps {($($func:ident $dir:literal),*) => {{
        $(embedded_map!($func, $dir);)*
        match key {
            $($dir => Some(($func as Embedded, include_str!(concat!("../../../assets/maps/", $dir, "/info.json")))),)*
            _ => None,
        }
    }}}
    maps!(old_holler "old-holler", tower_complex "tower-complex", cairnhold "cairnhold", frostline "frostline",
        dustreach "dustreach", longfield "longfield", highgoal "highgoal", ozarktic "ozarktic-blast", reefbreak "reefbreak")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn embedded(_key: &str) -> Option<(Embedded, &'static str)> { None }

/// The validated pack for `map`, loaded on first use from wherever the
/// catalog found it. `None`: Valley, a map that isn't installed, or one that
/// failed validation (`catalog::entry(map).problem()` says why).
pub fn on(map: crate::terrain::MapId) -> Option<&'static MapPack> {
    crate::catalog::entry(map).pack()
}

/// Side of the optional `shade.rg` terrain shade map, in texels (2 bytes each).
pub const SHADE_SIZE: u32 = 1024;
const SHADE_BYTES: usize = (SHADE_SIZE * SHADE_SIZE * 2) as usize;

/// Upper bound on `props.bin`: header, mesh table, vertices and instances.
const PROPS_MAX_BYTES: usize = 64_000_000;
/// Floats per prop vertex (local position, local normal, material, pad) and
/// per instance (world position, yaw, scale, mesh index, pad, pad).
pub const PROP_FLOATS: usize = 8;

/// One shared prop shape and the contiguous run of instances that use it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropMesh { pub first_vertex: u32, pub vertex_count: u32, pub first_instance: u32, pub instance_count: u32 }

/// Render-only props as GPU instances, parsed from `props.bin`: a few local
/// meshes per kind and a sorted instance list. Placement applies the kit's yaw
/// (`Mesh.point`) and a uniform scale; collision never comes from here.
pub struct PropSet { pub meshes: Vec<PropMesh>, pub vertices: Vec<f32>, pub instances: Vec<f32> }

impl PropSet {
    pub fn parse(bytes: &[u8], texture_count: u32) -> Result<Self, String> {
        let bad = |what: &str| format!("Invalid props.bin: {what}");
        if bytes.len() < 16 || &bytes[..4] != b"PRP1" { return Err(bad("header")); }
        let word = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let (meshes, instances) = (word(4) as usize, word(8) as usize);
        if meshes == 0 || meshes > 4096 || instances > 1_000_000 { return Err(bad("counts")); }
        let table_end = 16 + meshes * 16;
        if bytes.len() < table_end { return Err(bad("table")); }
        let mut table = Vec::with_capacity(meshes);
        let (mut next_vertex, mut next_instance) = (0u32, 0u32);
        for m in 0..meshes {
            let at = 16 + m * 16;
            let mesh = PropMesh { first_vertex: word(at), vertex_count: word(at + 4),
                first_instance: word(at + 8), instance_count: word(at + 12) };
            // Meshes are packed back to back, in instance order, whole triangles only.
            if mesh.first_vertex != next_vertex || mesh.first_instance != next_instance
                || mesh.vertex_count == 0 || mesh.vertex_count % 3 != 0 || mesh.vertex_count > 30_000 {
                return Err(bad("mesh table"));
            }
            next_vertex = next_vertex.checked_add(mesh.vertex_count).ok_or_else(|| bad("mesh table"))?;
            next_instance = next_instance.checked_add(mesh.instance_count).ok_or_else(|| bad("mesh table"))?;
            table.push(mesh);
        }
        if next_instance as usize != instances { return Err(bad("instance count")); }
        let floats = (next_vertex as usize + instances) * PROP_FLOATS;
        if bytes.len() != table_end + floats * 4 { return Err(bad("length")); }
        let data: Vec<f32> = bytes[table_end..].chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        if data.iter().any(|v| !v.is_finite()) { return Err(bad("non-finite value")); }
        let (vertices, instances) = data.split_at(next_vertex as usize * PROP_FLOATS);
        for v in vertices.chunks_exact(PROP_FLOATS) {
            if v[..3].iter().any(|x| x.abs() > 64.0) || v[3..6].iter().any(|x| x.abs() > 1.001)
                || v[6] < 0.0 || v[6] >= texture_count as f32 || v[6].fract() != 0.0 {
                return Err(bad("vertex"));
            }
        }
        for (m, mesh) in table.iter().enumerate() {
            let run = &instances[mesh.first_instance as usize * PROP_FLOATS..][..mesh.instance_count as usize * PROP_FLOATS];
            for i in run.chunks_exact(PROP_FLOATS) {
                if i[..3].iter().any(|x| x.abs() > 10000.0) || !(0.0..=64.0).contains(&i[4]) || i[4] == 0.0
                    || i[5] != m as f32 {
                    return Err(bad("instance"));
                }
            }
        }
        Ok(Self { meshes: table, vertices: vertices.to_vec(), instances: instances.to_vec() })
    }

    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|m| m.vertex_count as usize / 3 * m.instance_count as usize).sum()
    }
}

/// Distance-fog colour used by every map before per-map colours existed.
pub const DEFAULT_FOG: [f32; 3] = [0.62, 0.62, 0.62];

fn parse_fog(value: &str) -> Option<[f32; 3]> {
    let v: Vec<f32> = value.split_whitespace().map(|s| s.parse().ok()).collect::<Option<_>>()?;
    let c: [f32; 3] = v.try_into().ok()?;
    c.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x)).then_some(c)
}

impl MapPack {
    /// Optional manifest `sky.fogColor` ("r g b", 0..1); absent means [`DEFAULT_FOG`].
    pub fn fog_color(&self) -> [f32; 3] {
        self.manifest.sky.get("fogColor").and_then(|v| parse_fog(v)).unwrap_or(DEFAULT_FOG)
    }

    /// Optional manifest `sky.drawDistance` (m): how far the camera draws, for
    /// a small tile with scenery set out past it. Absent: the renderer's own
    /// reach (a little past the tile).
    pub fn draw_distance(&self) -> Option<f32> {
        self.manifest.sky.get("drawDistance").and_then(|v| v.parse::<f32>().ok())
    }

    /// A map folder: `map.json` and its payloads, each raw or zlib-compressed
    /// as `<name>.zlib`.
    pub fn load(root: &Path) -> Result<Self, String> {
        Self::from_assets(root, &|name, max| read_payload(root, name, max))
    }

    /// A browser build's embedded map.
    pub(crate) fn embedded(assets: Embedded) -> Result<Self, String> {
        let mut pack = Self::from_assets(Path::new(""), &|name, _| assets(name))?;
        pack.embedded = Some(assets);
        Ok(pack)
    }

    fn from_assets(root:&Path,read:&dyn Fn(&str,u64)->Result<Vec<u8>,String>)->Result<Self,String> {
        let json = read("map.json", 8_000_000)?;
        let manifest: Manifest = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
        if !manifest.terrain_step.is_finite() || !(1.0..=32.0).contains(&manifest.terrain_step)
            || manifest.name.len()>128 {
            return Err("Invalid terrain metadata".into());
        }
        if manifest.version != 1 || !(1..=256).contains(&manifest.texture_count) {
            return Err("Unsupported map version/texture count".into());
        }
        if manifest.terrain_layers.iter().chain(manifest.sky_layers.iter()).chain([&manifest.water_layer])
            .any(|&n| n >= manifest.texture_count) || manifest.sky_layers.len() < 6 {
            return Err("Invalid texture layer".into());
        }
        let sky_distance = |key: &str| -> Result<f32, String> {
            let n = manifest.sky.get(key).ok_or("Missing sky distance")?
                .parse::<f32>().map_err(|_| "Invalid sky distance")?;
            if !n.is_finite() || !(0.0..=10000.0).contains(&n) {
                return Err("Invalid sky distance".into());
            }
            Ok(n)
        };
        if sky_distance("visibleDistance")? <= sky_distance("fogDistance")? {
            return Err("Fog must start before visibility ends".into());
        }
        if manifest.sky.get("fogColor").is_some_and(|v| parse_fog(v).is_none()) {
            return Err("Invalid fog colour".into());
        }
        if manifest.sky.contains_key("drawDistance") && !(480.0..=10000.0).contains(&sky_distance("drawDistance")?) {
            return Err("Invalid draw distance".into());
        }
        manifest.look.validate()?;
        if manifest.flags.iter().chain(manifest.spawns.iter()).flatten().any(|v| !v.is_finite() || v.abs()>10000.0) {
            return Err("Invalid map positions".into());
        }
        if !manifest.spawn_points.is_empty() && (manifest.spawn_points.len()!=2
            || manifest.spawn_points.iter().any(|team| team.is_empty() || team.len()>32
                || team.iter().flatten().any(|v| !v.is_finite() || v.abs()>10000.0))) {
            return Err("Invalid spawn points".into());
        }
        if manifest.ambient_emitters.len()>32 || manifest.ambient_emitters.iter().any(|e|
            e.iter().any(|v|!v.is_finite()) || !(0.0..=1.0).contains(&e[3]) || e[4]<=0.0 || e[5]<=e[4]) {
            return Err("Invalid ambient emitter".into());
        }
        if manifest.smoke.len()>64 || manifest.smoke.iter().any(|s|
            s.iter().any(|v|!v.is_finite() || v.abs()>10000.0) || !(0.1..=8.0).contains(&s[3])) {
            return Err("Invalid smoke source".into());
        }
        for (key,count) in [("position",3),("scale",3)] {
            let values:Vec<f32>=manifest.water.get(key).ok_or("Missing water bounds")?.split_whitespace()
                .map(|s|s.parse::<f32>()).collect::<Result<_,_>>().map_err(|_|"Invalid water bounds")?;
            if values.len()!=count || values.iter().any(|n|!n.is_finite() || n.abs()>10000.0)
                || (key=="scale" && values.iter().any(|v|*v<=0.0)) {return Err("Invalid water bounds".into());}
        }
        let mut collision = Vec::new();
        let mut heights = Vec::new();
        let mut vertex_count = 0usize;
        for name in ["vertices.bin", "collision.bin", "height.bin", "weights.rgba", "textures.rgba", "ambient.f32"] {
            let bytes = read(name, 128_000_000)?;
            let hash = format!("{:x}", Sha256::digest(&bytes));
            if manifest.files.get(name) != Some(&hash) { return Err(format!("Checksum mismatch: {name}")); }
            match name {
                "collision.bin" => collision = bytes,
                "vertices.bin" => {
                    if bytes.len()%144 != 0 {return Err("Invalid vertex stride".into());}
                    vertex_count = bytes.len()/48;
                    for v in bytes.chunks_exact(48) {
                        let f: Vec<f32> = v.chunks_exact(4).map(|x| f32::from_le_bytes(x.try_into().unwrap())).collect();
                        if f.iter().any(|n| !n.is_finite()) || f[..3].iter().any(|n| n.abs()>10000.0)
                            || f[10]<0.0 || f[10]>=manifest.texture_count as f32 || f[11]>=manifest.texture_count as f32
                            || f[11]< -3.0-manifest.texture_count as f32 {
                            return Err("Invalid vertex data".into());
                        }
                    }
                }
                "height.bin" => {
                    if bytes.len()!=256*256*2 {return Err("Invalid heightfield dimensions".into());}
                    heights=bytes;
                }
                "weights.rgba" if bytes.len()!=256*256*4 => return Err("Invalid layer weights".into()),
                "textures.rgba" if bytes.len()!=349524*manifest.texture_count as usize => return Err("Invalid textures".into()),
                "ambient.f32" if bytes.len()%4!=0 || bytes.len()>44100*4*60
                    || bytes.chunks_exact(4).any(|b| {let v=f32::from_le_bytes(b.try_into().unwrap());!v.is_finite() || v.abs()>1.0}) => return Err("Invalid ambience".into()),
                _ => (),
            }
        }
        // Optional baked terrain shade (scripts/assets/terrain_shade.py): sun
        // visibility and ambient occlusion over the tile, two bytes per texel.
        if manifest.files.contains_key("shade.rg") {
            let bytes = read("shade.rg", SHADE_BYTES as u64)?;
            if manifest.files.get("shade.rg") != Some(&format!("{:x}", Sha256::digest(&bytes))) {
                return Err("Checksum mismatch: shade.rg".into());
            }
            if bytes.len() != SHADE_BYTES { return Err("Invalid terrain shade".into()); }
        }
        // Optional render-only prop instances (scripts/assets/props.py Instancer).
        if manifest.files.contains_key("props.bin") {
            let bytes = read("props.bin", PROPS_MAX_BYTES as u64)?;
            if manifest.files.get("props.bin") != Some(&format!("{:x}", Sha256::digest(&bytes))) {
                return Err("Checksum mismatch: props.bin".into());
            }
            PropSet::parse(&bytes, manifest.texture_count)?;
        }
        // Optional lava ground (marker `lava2`): one record per terrain
        // cell, rows along x: the surface (u16, metres x 32; 0: no lava), the
        // burn (u8, damage a second; 0: `water::LAVA_BURN`) and flags (u8;
        // bit 0: skim). Lava that follows uneven ground, which flat
        // `water_volumes` can't.
        let mut lava = Vec::new();
        if manifest.files.contains_key("lava.bin") {
            let bytes = read("lava.bin", LAVA_BYTES as u64)?;
            if manifest.files.get("lava.bin") != Some(&format!("{:x}", Sha256::digest(&bytes))) {
                return Err("Checksum mismatch: lava.bin".into());
            }
            if bytes.len() != LAVA_BYTES || bytes.chunks_exact(4).any(|r| r[3] > 1) { return Err("Invalid lava ground".into()); }
            lava = bytes;
        }
        // Optional authored models: placements, the file (the browser build
        // draws none, so it neither embeds nor reads it) and the stand-in run.
        if manifest.models.len() > 64 || manifest.models.iter().any(|m| m.model.is_empty() || m.model.len() > 64
            || !m.model.bytes().all(|b| b.is_ascii_graphic()) || !m.yaw.is_finite() || m.team.is_some_and(|t| t > 1)
            || m.position.iter().any(|v| !v.is_finite() || v.abs() > 10000.0)) {
            return Err("Invalid model placement".into());
        }
        if !manifest.models.is_empty() && !manifest.files.contains_key("models.glb") {
            return Err("Model placements without models.glb".into());
        }
        if manifest.files.contains_key("models.glb") && !cfg!(target_arch = "wasm32") {
            let bytes = read("models.glb", MODELS_MAX_BYTES)?;
            if manifest.files.get("models.glb") != Some(&format!("{:x}", Sha256::digest(&bytes))) {
                return Err("Checksum mismatch: models.glb".into());
            }
            if bytes.get(..4) != Some(b"glTF") { return Err("Invalid models.glb".into()); }
        }
        if let Some([first, count]) = manifest.standin {
            if first % 3 != 0 || count % 3 != 0 || first as usize + count as usize > vertex_count {
                return Err("Invalid stand-in range".into());
            }
        }
        if collision.len()%36 != 0 || collision.len()>36*500_000 {return Err("Invalid collision data".into());}
        let mut triangles=Vec::new();
        let mut bounds=Vec::new();
        let mut buckets: HashMap<(i32,i32),Vec<usize>>=HashMap::new();
        for bytes in collision.chunks_exact(36) {
            let f: Vec<f32>=bytes.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
            if f.iter().any(|x| !x.is_finite() || x.abs()>10000.0) {return Err("Invalid triangle".into());}
            let tri=[Vec3::from_slice(&f[..3]),Vec3::from_slice(&f[3..6]),Vec3::from_slice(&f[6..])];
            let low=tri[0].min(tri[1]).min(tri[2]);let high=tri[0].max(tri[1]).max(tri[2]);
            if (high-low).max_element()>512.0 {return Err("Oversized collision triangle".into());}
            let id=triangles.len();
            for x in cell(low.x)..=cell(high.x) {for z in cell(low.z)..=cell(high.z) {
                buckets.entry((x,z)).or_default().push(id);
            }}
            triangles.push(tri);
            bounds.push((low,high));
        }
        let mut holes=vec![false;65536];
        for &i in &manifest.holes {
            if i>=holes.len() {return Err("Invalid terrain hole".into());}
            holes[i]=true;
        }
        crate::equipment::validate(&manifest.entities)?;
        crate::control::validate(&manifest.control_points)?;
        crate::water::validate(&manifest.water_volumes)?;
        if let Some(field) = &manifest.football { field.validate()?; }
        let normals=triangles.iter().map(|t:&[Vec3;3]|(t[1]-t[0]).cross(t[2]-t[0]).normalize_or_zero()).collect();
        Ok(Self {embedded:None,manifest,root:root.into(),fingerprint:format!("{:x}",Sha256::digest(&json)),triangles,bounds,normals,buckets,holes,heights,lava})
    }

    /// The lava ground over (x, z) from `lava.bin`, if any.
    pub fn lava_cell(&self, x: f32, z: f32) -> Option<LavaCell> {
        if self.lava.is_empty() { return None; }
        let (fx, fz) = (x / self.manifest.terrain_step, z / self.manifest.terrain_step);
        if !(0.0..256.0).contains(&fx) || !(0.0..256.0).contains(&fz) { return None; }
        let o = (fz as usize * 256 + fx as usize) * 4;
        let r = &self.lava[o..o + 4];
        let surface = u16::from_le_bytes([r[0], r[1]]);
        (surface > 0).then(|| LavaCell {
            surface: surface as f32 / 32.0,
            burn: if r[2] > 0 { r[2] as f32 } else { crate::water::LAVA_BURN },
            skim: r[3] & 1 != 0,
        })
    }

    /// True when the map has lava ground (`lava.bin`).
    pub fn has_lava_ground(&self) -> bool { !self.lava.is_empty() }

    pub fn triangle_count(&self)->usize {self.triangles.len()}

    /// Collision triangles, for offline analysis such as bot navigation, and
    /// the structures' footprints on the client's maps.
    pub fn triangles(&self)->&[[Vec3; 3]] {&self.triangles}

    pub fn hole(&self,x:f32,z:f32)->bool {
        let x=(x/self.manifest.terrain_step).floor().clamp(0.0,255.0) as usize;
        let z=(z/self.manifest.terrain_step).floor().clamp(0.0,255.0) as usize;
        self.holes[z*256+x]
    }

    pub fn asset(&self,name:&str)->Result<Vec<u8>,String> {
        // Renderer/audio reopen only listed fixed assets. Recheck the digest so
        // editing a pack after startup cannot split visible and physical worlds.
        if name.contains(['/', '\\']) || !self.manifest.files.contains_key(name) {return Err("Unknown map asset".into());}
        if let Some(embedded) = self.embedded {return embedded(name);}
        let bytes=read_payload(&self.root,name,128_000_000)?;
        if format!("{:x}",Sha256::digest(&bytes))!=self.manifest.files[name] {return Err(format!("Map asset changed: {name}"));}
        Ok(bytes)
    }

    /// Continuous sphere/triangle sweep; includes face, edge and vertex contacts.
    pub fn sweep(&self,start:Vec3,end:Vec3,radius:f32)->Option<(f32,Vec3)> {
        let low=start.min(end)-Vec3::splat(radius);let high=start.max(end)+Vec3::splat(radius);
        let mut best:Option<(f32,Vec3)>=None;
        for x in cell(low.x)..=cell(high.x) {for z in cell(low.z)..=cell(high.z) {
            if let Some(ids)=self.buckets.get(&(x,z)) {for &id in ids {
                let (tl,th)=self.bounds[id];
                if th.x<low.x || tl.x>high.x || th.y<low.y || tl.y>high.y || th.z<low.z || tl.z>high.z {continue;}
                if let Some(hit)=sweep_triangle(start,end,radius,self.triangles[id]) {
                    if best.is_none_or(|old|hit.0<old.0) {best=Some(hit);}
                }
            }}
        }}
        best
    }

    /// Walking collision's four body spheres. Callers must keep this in step
    /// with movement; a single sphere at the feet is not the player.
    pub fn body_sweep(&self, start: Vec3, end: Vec3) -> Option<(f32, Vec3)> {
        let mut best: Option<(f32, Vec3)> = None;
        for offset in [0.0, 0.5, 1.0, crate::terrain::EYE] {
            let rise = Vec3::Y * offset;
            if let Some(hit) = self.sweep(start + rise, end + rise, crate::terrain::PLAYER_RADIUS) {
                if best.is_none_or(|old| hit.0 < old.0) { best = Some(hit); }
            }
        }
        best
    }

    /// The wall the walking body overlaps most (`walls1`): the face's normal
    /// on the body's side and how deep. A sweep meets only the faces it
    /// reaches during a move, so a body that started a move a hair inside a
    /// wall (pushed there off a second face in a corner tighter than square)
    /// went on through it. Only near-vertical faces (|n.y| under `WALL_NY`):
    /// on ground and slopes, where a standing body rests on the surface,
    /// nothing changes.
    pub fn wall_overlap(&self, pos: Vec3) -> Option<(Vec3, f32)> {
        let r = crate::terrain::PLAYER_RADIUS;
        let mut best: Option<(Vec3, f32)> = None;
        self.near_body(pos, pos, 0.0, |id| {
            let (tri, n) = (self.triangles[id], self.normals[id]);
            if n.length_squared() < 0.5 || n.y.abs() >= WALL_NY { return; }
            for s in BODY.map(|o| pos + Vec3::Y * o) {
                let d = (s - tri[0]).dot(n);
                let depth = r - d.abs();
                if depth <= 0.0 || !inside(s - n * d, tri, n) { continue; }
                if best.is_none_or(|(_, old)| depth > old) { best = Some((if d >= 0.0 { n } else { -n }, depth)); }
            }
        });
        best
    }

    /// Whether a walking body set down at `pos` without walking there (a
    /// rider getting out) has a sphere above its feet inside the map: a
    /// sweep that starts inside a low deck can't see it. The feet are left
    /// out (they rest on the ground, maybe a slope), and only a real overlap
    /// counts, not a touch.
    pub fn body_stuck(&self, pos: Vec3) -> bool {
        let r = crate::terrain::PLAYER_RADIUS - 0.02;
        let mut stuck = false;
        self.near_body(pos, pos, 0.0, |id| {
            let tri = self.triangles[id];
            if !stuck { stuck = BODY[1..].iter().map(|&o| pos + Vec3::Y * o).any(|s| closest_on(s, tri).distance_squared(s) < r * r); }
        });
        stuck
    }

    /// Whether moving the walking body from `from` to `to` without a sweep
    /// (a contact's push off its surface, a snap down to the ground) heads
    /// into a face it touches (`walls1`; with `walls_only`, a near-vertical
    /// one): in a corner tighter than square the push off one face reaches
    /// into the other, and a sweep that starts inside a face can't see it.
    pub fn push_enters(&self, from: Vec3, to: Vec3, walls_only: bool) -> bool {
        let r = crate::terrain::PLAYER_RADIUS;
        let mut enters = false;
        self.near_body(from, to, 0.0, |id| {
            let (tri, n) = (self.triangles[id], self.normals[id]);
            if enters || (walls_only && n.y.abs() >= WALL_NY) { return; }
            for o in BODY {
                let (a, b) = (from + Vec3::Y * o, to + Vec3::Y * o);
                // Too far from its plane to touch it, before or after.
                if n != Vec3::ZERO && (a - tri[0]).dot(n).abs() >= r + TOUCH && (b - tri[0]).dot(n).abs() >= r { continue; }
                let c = closest_on(a, tri);
                let d = c.distance(a);
                // Already inside it (resting on a slope): not this one.
                if d < r - TOUCH || d < 1e-6 { continue; }
                // Touching it, to within rounding: pushed toward it, by
                // direction, which rounding can't blur. Clear of it: inside
                // it at the end.
                if d < r + TOUCH { if (b - a).dot((a - c) / d) < -0.02 * (b - a).length() { enters = true; } }
                else if closest_on(b, tri).distance_squared(b) < r * r { enters = true; }
            }
        });
        enters
    }

    /// Where the walking body, moved from `at` to `to` without a sweep,
    /// ends up clear of every face it was clear of at `at` (`walls1`): a
    /// hair off each it comes within `within` of, found in passes (as near
    /// as they come if they don't settle). After a contact's push (`within`
    /// a hair over touching), so the push leaves it off a corner's other
    /// face, or an edge beside it; after a whole move (`within` a hair
    /// inside), so a body never ends a move sunk into a face it started
    /// clear of, which a sweep can't see from inside and would let it through.
    pub fn settle(&self, at: Vec3, to: Vec3, within: f32) -> Vec3 {
        let r = crate::terrain::PLAYER_RADIUS;
        let mut near = false;
        self.near_body(at, to, 0.01, |id| {
            let (tri, n) = (self.triangles[id], self.normals[id]);
            if near { return; }
            for b in BODY.map(|o| to + Vec3::Y * o) {
                if n != Vec3::ZERO && (b - tri[0]).dot(n).abs() >= within { continue; }
                if closest_on(b, tri).distance(b) < within { near = true; }
            }
        });
        if !near { return to; }
        // Every face clear at `at` that the moves below might come near.
        let mut pairs: Vec<(usize, f32)> = Vec::new();
        self.near_body(at, to, 0.05, |id| {
            for o in BODY {
                let a = at + Vec3::Y * o;
                if !pairs.contains(&(id, o)) && closest_on(a, self.triangles[id]).distance(a) >= r - TOUCH { pairs.push((id, o)); }
            }
        });
        let mut p = to;
        for _ in 0..16 {
            let mut moved = false;
            for &(id, offset) in &pairs {
                let s = p + Vec3::Y * offset;
                let c = closest_on(s, self.triangles[id]);
                let d = c.distance(s);
                if d < within && d > 1e-6 { p += (s - c) / d * (r + 0.001 - d); moved = true; }
            }
            if !moved { return p; }
        }
        p
    }

    /// Each collision triangle near the walking body (`BODY`'s spheres)
    /// anywhere on its way from `a` to `b`, `pad` more: one pass over the
    /// buckets for all four spheres.
    fn near_body(&self, a: Vec3, b: Vec3, pad: f32, mut each: impl FnMut(usize)) {
        let r = crate::terrain::PLAYER_RADIUS + pad;
        let low = a.min(b) - Vec3::splat(r);
        let high = a.max(b) + Vec3::new(r, r + BODY[3], r);
        for x in cell(low.x)..=cell(high.x) { for z in cell(low.z)..=cell(high.z) {
            let Some(ids) = self.buckets.get(&(x, z)) else { continue };
            for &id in ids {
                let (tl, th) = self.bounds[id];
                if th.x < low.x || tl.x > high.x || th.y < low.y || tl.y > high.y || th.z < low.z || tl.z > high.z { continue; }
                each(id);
            }
        }}
    }

    /// `settle` after a contact's push (`walls1`): off any face it touched
    /// by a hair.
    pub fn settle_push(&self, at: Vec3, to: Vec3) -> Vec3 { self.settle(at, to, crate::terrain::PLAYER_RADIUS + TOUCH) }

    /// `settle` after a whole move (`walls1`): out of any face it sank into.
    pub fn unslip(&self, from: Vec3, to: Vec3) -> Vec3 { self.settle(from, to, crate::terrain::PLAYER_RADIUS - TOUCH) }

    pub fn floor(&self,pos:Vec3)->Option<(f32,Vec3)> {
        let start=pos+Vec3::Y*0.15;let end=Vec3::new(pos.x,-1000.0,pos.z);
        self.sweep(start,end,0.0).filter(|(_,n)|n.y>0.05).map(|(t,n)|(start.lerp(end,t).y,n))
    }

    /// Used once when constructing a safe overhead menu-camera altitude.
    pub fn highest_solid(&self)->f32 {
        self.triangles.iter().flatten().map(|p|p.y).fold(f32::NEG_INFINITY,f32::max)
    }
}

/// One file of a map folder, at most `max` bytes: `<name>`, else `<name>.zlib`.
fn read_payload(root: &Path, name: &str, max: u64) -> Result<Vec<u8>, String> {
    let path = root.join(name);
    match path.metadata() {
        Ok(meta) => {
            if meta.len() > max { return Err(format!("{name} exceeds size limit")); }
            return std::fs::read(path).map_err(|e| format!("{name}: {e}"));
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{name}: {e}")),
        Err(_) => (),
    }
    let packed = root.join(format!("{name}.zlib"));
    match packed.metadata() {
        Ok(meta) => {
            if meta.len() > max { return Err(format!("{name}.zlib exceeds size limit")); }
            inflate(&std::fs::read(packed).map_err(|e| format!("{name}.zlib: {e}"))?, max as usize)
        }
        // Launcher r1 cannot carry empty files. The manifest hash still must
        // prove that this map expects empty audio.
        Err(_) if name == "ambient.f32" => Ok(Vec::new()),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

fn cell(x:f32)->i32 {(x/32.0).floor() as i32}

/// The walking body's spheres, by their centers' height over its feet's
/// (`body_sweep`).
const BODY: [f32; 4] = [0.0, 0.5, 1.0, crate::terrain::EYE];

/// Within this of touching counts as touching for `push_enters` and
/// `settle`: positions here are near 1000 m, where single precision keeps
/// distances to about a tenth of a millimeter.
const TOUCH: f32 = 0.0005;

/// The steepest a face can lean and still count as a wall for
/// `MapPack::wall_overlap` (|n.y| below it: about 75 degrees and steeper).
const WALL_NY: f32 = 0.25;

/// The point of triangle `t` closest to `p` (by its Voronoi regions).
pub(crate) fn closest_on(p: Vec3, t: [Vec3; 3]) -> Vec3 {
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 { return a; }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 { return b; }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 { return a + ab * (d1 / (d1 - d3)); }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 { return c; }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 { return a + ac * (d2 / (d2 - d6)); }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 { return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))); }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

fn inside(p:Vec3,t:[Vec3;3],n:Vec3)->bool {
    (0..3).all(|i|(t[(i+1)%3]-t[i]).cross(p-t[i]).dot(n)>=-0.0001)
}

fn root(a:f32,b:f32,c:f32)->Option<f32> {
    if a<1e-12 {return None;}
    let d=b*b-a*c;
    if d<0.0 {return None;}
    let t=(-b-d.sqrt())/a;
    (0.0..=1.0).contains(&t).then_some(t)
}

fn sweep_triangle(s:Vec3,e:Vec3,r:f32,tri:[Vec3;3])->Option<(f32,Vec3)> {
    let v=e-s;let n=(tri[1]-tri[0]).cross(tri[2]-tri[0]).normalize_or_zero();
    if n.length_squared()<0.5 {return None;}
    let mut best=None;
    let mut accept=|t:f32,normal:Vec3| {
        if (0.0..=1.0).contains(&t) && v.dot(normal)< -1e-7 && best.is_none_or(|(old,_)|t<old) {best=Some((t,normal));}
    };
    let d=(s-tri[0]).dot(n);let dv=v.dot(n);
    if dv.abs()>1e-8 {
        for sign in [-1.0,1.0] {
            let t=(sign*r-d)/dv;
            if (0.0..=1.0).contains(&t) && inside(s+v*t-n*(sign*r),tri,n) {accept(t,n*sign);}
        }
    }
    if r>0.0 {
        for i in 0..3 {
            let a=tri[i];let edge=tri[(i+1)%3]-a;let len=edge.length_squared();
            let q=s-a;
            if let Some(t)=root(v.length_squared(),q.dot(v),q.length_squared()-r*r) {
                accept(t,(s+v*t-a).normalize_or_zero());
            }
            if len>1e-10 {
                let qp=q-edge*(q.dot(edge)/len);let vp=v-edge*(v.dot(edge)/len);
                if let Some(t)=root(vp.length_squared(),qp.dot(vp),qp.length_squared()-r*r) {
                    let u=(q+v*t).dot(edge)/len;
                    if (0.0..=1.0).contains(&u) {accept(t,(s+v*t-a-edge*u).normalize_or_zero());}
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod sweep_bounds_tests {
    use super::*;

    /// The bounding-box rejection never drops a hit: on the densest stock
    /// map, every sweep (a moving body, a floor probe, a sight line) meets
    /// the same first contact a scan of every collision triangle finds.
    /// A body set down with its head in a deck is stuck (a rider getting
    /// out under a low deck: a sweep starting there can't see it); one
    /// standing on the deck isn't.
    #[test]
    fn a_body_set_down_in_a_deck_is_stuck_and_on_it_is_not() {
        use crate::terrain::{EYE, PLAYER_RADIUS};
        let pack = on(crate::terrain::MapId::OldHoller).expect("Old Holler installed");
        let area = |t: &[Vec3; 3]| (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5;
        let deck = pack.triangles.iter().zip(&pack.normals).map(|(t, n)| (*t, *n)).find(|(t, n)| {
            let mid = (t[0] + t[1] + t[2]) / 3.0;
            n.y > 0.99 && area(t) > 4.0 && pack.sweep(mid + Vec3::Y * 0.1, mid + Vec3::Y * 4.0, PLAYER_RADIUS).is_none()
        }).expect("a level deck open above").0;
        let mid = (deck[0] + deck[1] + deck[2]) / 3.0;
        assert!(!pack.body_stuck(mid + Vec3::Y * (PLAYER_RADIUS + 0.05)), "standing on it");
        assert!(pack.body_stuck(mid - Vec3::Y * EYE), "head in it");
    }

    #[test]
    fn sweeps_find_what_a_full_scan_finds() {
        let pack = on(crate::terrain::MapId::Ashveil).expect("Ashveil installed");
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rand = move || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; (seed % 1_000_000) as f32 / 1_000_000.0 };
        let mut hits = 0;
        for k in 0..4000 {
            let at = Vec3::new(260.0 + rand() * 140.0, 20.0 + rand() * 50.0, 280.0 + rand() * 110.0);
            let (end, radius) = match k % 3 {
                0 => (at + Vec3::new(rand() - 0.5, rand() - 0.5, rand() - 0.5) * 2.0, crate::terrain::PLAYER_RADIUS),
                1 => (Vec3::new(at.x, -1000.0, at.z), 0.0),
                _ => (at + Vec3::new(rand() - 0.5, rand() - 0.5, rand() - 0.5) * 120.0, 0.0),
            };
            let fast = pack.sweep(at, end, radius).map(|h| h.0);
            let full = pack.triangles.iter().filter_map(|&t| sweep_triangle(at, end, radius, t)).map(|h| h.0).reduce(f32::min);
            assert_eq!(fast, full, "sweep {at:?} -> {end:?} r {radius}");
            hits += fast.is_some() as usize;
        }
        assert!(hits > 300, "the probes meet the map: {hits}");
    }
}

#[cfg(test)]
mod tests {
    /// Release-mode load timing probe: `cargo test --release -p peakrunner-core
    /// --lib pack_load_timing -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing probe"]
    fn pack_load_timing() {
        use super::*;
        use crate::terrain::MapId;
        let t = std::time::Instant::now();
        let rain = on(MapId::OldHoller).unwrap();
        println!("old-holler load {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
        for (map, label) in [(MapId::TowerComplex, "tower-complex"), (MapId::Cairnhold, "cairnhold"),
            (MapId::Frostline, "frostline"), (MapId::Dustreach, "dustreach")] {
            let t = std::time::Instant::now();
            let pack = on(map).unwrap();
            let load = t.elapsed();
            let t = std::time::Instant::now();
            let textures = pack.asset("textures.rgba").unwrap();
            println!("{label} load {:.1} ms, textures reopen {:.1} ms ({} bytes)",
                load.as_secs_f64() * 1e3, t.elapsed().as_secs_f64() * 1e3, textures.len());
        }
        let t = std::time::Instant::now();
        rain.asset("textures.rgba").unwrap();
        println!("old-holler textures reopen {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    }

    #[test]
    fn installed_payloads_match_their_manifest_hashes() {
        use super::*;
        use crate::terrain::MapId;
        assert!(inflate(b"not a deflate stream", 1000).is_err());
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
            let pack = on(map).unwrap();
            for name in pack.manifest.files.keys() {
                let bytes = pack.asset(name).unwrap();
                assert_eq!(format!("{:x}", Sha256::digest(&bytes)), pack.manifest.files[name], "{map:?} {name}");
            }
        }
    }

    #[test]
    fn props_instances_parse_and_reject_malformed_data() {
        use super::*;
        use crate::terrain::MapId;
        // Every shipped map carries its grass and small scenery as instances.
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
            let pack = on(map).unwrap();
            let set = PropSet::parse(&pack.asset("props.bin").unwrap(), pack.manifest.texture_count).unwrap();
            assert!(set.meshes.len() >= 24 && set.instances.len() / PROP_FLOATS > 1000, "{map:?}");
            assert!(set.triangle_count() > 50_000, "{map:?} {}", set.triangle_count());
        }
        // A minimal valid set: one triangle mesh, two instances.
        let build = |table: [u32; 4], verts: &[[f32; 8]], inst: &[[f32; 8]], meshes: u32, count: u32| {
            let mut b = b"PRP1".to_vec();
            for w in [meshes, count, 0] { b.extend(w.to_le_bytes()); }
            for w in table { b.extend(w.to_le_bytes()); }
            for row in verts.iter().chain(inst) { for v in row { b.extend(v.to_le_bytes()); } }
            b
        };
        let tri = [[0., 0., 0., 0., 1., 0., 2., 0.], [1., 0., 0., 0., 1., 0., 2., 0.], [0., 1., 0., 0., 1., 0., 2., 0.]];
        let inst = [[10., 20., 30., 0.5, 1.2, 0., 0., 0.], [11., 20., 30., 0.1, 0.8, 0., 0., 0.]];
        let good = build([0, 3, 0, 2], &tri, &inst, 1, 2);
        let set = PropSet::parse(&good, 8).unwrap();
        assert_eq!(set.meshes, vec![PropMesh { first_vertex: 0, vertex_count: 3, first_instance: 0, instance_count: 2 }]);
        assert_eq!(set.triangle_count(), 2);
        // Each corruption is refused.
        let mut magic = good.clone(); magic[0] = b'X';
        assert!(PropSet::parse(&magic, 8).is_err());
        assert!(PropSet::parse(&good[..good.len() - 4], 8).is_err(), "truncated");
        assert!(PropSet::parse(&good, 2).is_err(), "material beyond the texture count");
        assert!(PropSet::parse(&build([0, 2, 0, 2], &tri[..2], &inst, 1, 2), 8).is_err(), "partial triangle");
        assert!(PropSet::parse(&build([0, 3, 0, 1], &tri, &inst, 1, 2), 8).is_err(), "instance count mismatch");
        let mut wrong_mesh = inst; wrong_mesh[1][5] = 3.;
        assert!(PropSet::parse(&build([0, 3, 0, 2], &tri, &wrong_mesh, 1, 2), 8).is_err(), "instance points at another mesh");
        let mut zero = inst; zero[0][4] = 0.;
        assert!(PropSet::parse(&build([0, 3, 0, 2], &tri, &zero, 1, 2), 8).is_err(), "zero scale");
        let mut far = inst; far[0][0] = f32::NAN;
        assert!(PropSet::parse(&build([0, 3, 0, 2], &tri, &far, 1, 2), 8).is_err(), "non-finite");
    }

    #[test]
    fn fog_colour_parses_validates_and_defaults() {
        use super::*;
        assert_eq!(parse_fog("0.8 0.69 0.52"), Some([0.8, 0.69, 0.52]));
        for bad in ["0.8 0.69", "0.8 0.69 0.52 1", "1.2 0 0", "x 0 0", "NaN 0 0"] {
            assert_eq!(parse_fog(bad), None, "{bad}");
        }
        use crate::terrain::MapId;
        // Maps without fogColor keep the historical grey (every shipped map now sets one).
        assert_eq!(DEFAULT_FOG, [0.62, 0.62, 0.62]);
        assert_eq!(on(MapId::Cairnhold).unwrap().fog_color(), [0.78, 0.71, 0.60]);
        assert_eq!(on(MapId::OldHoller).unwrap().fog_color(), [0.62, 0.66, 0.68]);
        assert_eq!(on(MapId::TowerComplex).unwrap().fog_color(), [0.74, 0.80, 0.88]);
        assert_eq!(on(MapId::Dustreach).unwrap().fog_color(), [0.80, 0.69, 0.52]);
        assert_eq!(on(MapId::Frostline).unwrap().fog_color(), [0.84, 0.87, 0.90]);
        // Only a map that sets `drawDistance` draws past the renderer's reach.
        assert_eq!(on(MapId::Cairnhold).unwrap().draw_distance(), None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn omitted_ambience_requires_the_empty_content_hash() {
        use super::*;
        let source = on(crate::terrain::MapId::OldHoller).unwrap().root.clone();
        let builtin_asset = |name: &str| read_payload(&source, name, 128_000_000);
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("peakrunner-empty-audio-{}-{nonce}",std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for name in ["map.json","vertices.bin","collision.bin","height.bin","weights.rgba","textures.rgba","shade.rg"] {
            std::fs::write(root.join(name), builtin_asset(name).unwrap()).unwrap();
        }
        // Its authored models, when it has them.
        if let Ok(models) = builtin_asset("models.glb") { std::fs::write(root.join("models.glb"), models).unwrap(); }
        // A pack that lists props.bin refuses to load without it.
        let mut listed: serde_json::Value = serde_json::from_slice(&builtin_asset("map.json").unwrap()).unwrap();
        listed["files"]["ambient.f32"] = format!("{:x}",Sha256::digest([])).into();
        std::fs::write(root.join("map.json"),serde_json::to_vec(&listed).unwrap()).unwrap();
        assert!(MapPack::load(&root).is_err(), "missing props.bin must fail");
        std::fs::write(root.join("props.bin"), builtin_asset("props.bin").unwrap()).unwrap();
        std::fs::write(root.join("map.json"), builtin_asset("map.json").unwrap()).unwrap();
        assert!(MapPack::load(&root).is_err(), "missing nonempty audio must fail");
        let mut manifest: serde_json::Value = serde_json::from_slice(&builtin_asset("map.json").unwrap()).unwrap();
        manifest["files"]["ambient.f32"] = format!("{:x}",Sha256::digest([])).into();
        std::fs::write(root.join("map.json"),serde_json::to_vec(&manifest).unwrap()).unwrap();
        let absent = MapPack::load(&root).unwrap();
        assert!(absent.asset("ambient.f32").unwrap().is_empty());
        std::fs::write(root.join("ambient.f32"),[]).unwrap();
        assert_eq!(absent.fingerprint, MapPack::load(&root).unwrap().fingerprint);
        std::fs::write(root.join("ambient.f32"),[1,2,3,4]).unwrap();
        assert!(absent.asset("ambient.f32").is_err());
        assert!(MapPack::load(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// `lava.bin` gives each terrain cell its lava: surface, burn and skim.
    /// A malformed one keeps the map from loading.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn lava_ground_reads_per_cell_and_refuses_bad_records() {
        use super::*;
        let source = on(crate::terrain::MapId::OldHoller).unwrap().root.clone();
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("peakrunner-lava-ground-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for name in ["vertices.bin", "collision.bin", "height.bin", "weights.rgba", "textures.rgba", "shade.rg", "props.bin", "ambient.f32", "models.glb"] {
            if let Ok(bytes) = read_payload(&source, name, 128_000_000) { std::fs::write(root.join(name), bytes).unwrap(); }
        }
        let manifest: serde_json::Value = serde_json::from_slice(&read_payload(&source, "map.json", 8_000_000).unwrap()).unwrap();
        let step = manifest["terrain_step"].as_f64().unwrap_or(8.0) as f32;
        let write = |lava: &[u8]| {
            let mut m = manifest.clone();
            m["files"]["lava.bin"] = format!("{:x}", Sha256::digest(lava)).into();
            std::fs::write(root.join("map.json"), serde_json::to_vec(&m).unwrap()).unwrap();
            std::fs::write(root.join("lava.bin"), lava).unwrap();
            MapPack::load(&root)
        };
        let mut lava = vec![0u8; LAVA_BYTES];
        // Cell (10, 20): skim lava at 37.5 m burning 150 a second; cell
        // (11, 20): lava lying on the ground, at the default burn.
        let record = |x: usize, z: usize| (z * 256 + x) * 4;
        lava[record(10, 20)..record(10, 20) + 4].copy_from_slice(&[(37.5f32 * 32.0) as u16 as u8, ((37.5f32 * 32.0) as u16 >> 8) as u8, 150, 1]);
        lava[record(11, 20)..record(11, 20) + 4].copy_from_slice(&[64, 6, 0, 0]);
        let pack = write(&lava).unwrap();
        assert!(pack.has_lava_ground());
        let at = |x: usize, z: usize| pack.lava_cell((x as f32 + 0.5) * step, (z as f32 + 0.5) * step);
        assert_eq!(at(10, 20), Some(LavaCell { surface: 37.5, burn: 150.0, skim: true }));
        assert_eq!(at(11, 20), Some(LavaCell { surface: 1600.0 / 32.0, burn: crate::water::LAVA_BURN, skim: false }));
        assert_eq!(at(12, 20), None);
        assert_eq!(pack.lava_cell(-1.0, 20.5 * step), None, "off the tile");
        lava[record(12, 20) + 3] = 2;
        assert!(write(&lava).is_err(), "an unknown flag");
        assert!(write(&lava[..LAVA_BYTES - 4]).is_err(), "a short file");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn smoke_sources_load_and_bad_ones_are_refused() {
        use super::*;
        let source = on(crate::terrain::MapId::OldHoller).unwrap().root.clone();
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("peakrunner-smoke-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for name in ["vertices.bin", "collision.bin", "height.bin", "weights.rgba", "textures.rgba", "shade.rg", "props.bin", "ambient.f32", "models.glb"] {
            if let Ok(bytes) = read_payload(&source, name, 128_000_000) { std::fs::write(root.join(name), bytes).unwrap(); }
        }
        let manifest: serde_json::Value = serde_json::from_slice(&read_payload(&source, "map.json", 8_000_000).unwrap()).unwrap();
        let write = |smoke: serde_json::Value| {
            let mut m = manifest.clone();
            m["smoke"] = smoke;
            std::fs::write(root.join("map.json"), serde_json::to_vec(&m).unwrap()).unwrap();
            MapPack::load(&root)
        };
        let pack = write(serde_json::json!([[100.0, 80.0, 120.0, 3.2], [90.0, 60.0, 110.0, 0.5]])).unwrap();
        assert_eq!(pack.manifest.smoke, vec![[100.0, 80.0, 120.0, 3.2], [90.0, 60.0, 110.0, 0.5]]);
        assert!(write(serde_json::json!([[100.0, 80.0, 120.0, 20.0]])).is_err(), "too big");
        assert!(write(serde_json::json!([[100.0, 80.0, 120.0, 0.0]])).is_err(), "no size");
        assert!(write(serde_json::json!(vec![[0.0, 0.0, 0.0, 1.0]; 65])).is_err(), "too many");
        std::fs::remove_dir_all(root).unwrap();
    }
    use super::*;

    #[test]
    fn fast_sweeps_stop_at_thin_faces_from_both_sides() {
        let t=[Vec3::new(-10.,0.,-10.),Vec3::new(0.,0.,10.),Vec3::new(10.,0.,-10.)];
        for sign in [-1.,1.] {
            let (time,n)=sweep_triangle(Vec3::Y*20.*sign,-Vec3::Y*20.*sign,0.5,t).unwrap();
            assert!((time-0.4875).abs()<1e-5);assert!(n.y*sign>0.99);
        }
        assert!(sweep_triangle(Vec3::new(50.,20.,0.),Vec3::new(50.,-20.,0.),0.5,t).is_none());
    }
    /// Installs keep payloads zlib-compressed (`scripts/stage-packs.sh`): the
    /// same pack, the same fingerprint, and reopened assets still checked.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn compressed_map_folders_load_to_the_same_pack() {
        let source = on(crate::terrain::MapId::Highgoal).unwrap();
        let root = std::env::temp_dir().join(format!("peakrunner-zlib-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::copy(source.root.join("map.json"), root.join("map.json")).unwrap();
        for name in source.manifest.files.keys() {
            let raw = read_payload(&source.root, name, 128_000_000).unwrap();
            std::fs::write(root.join(format!("{name}.zlib")), miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6)).unwrap();
        }
        let packed = MapPack::load(&root).unwrap();
        assert_eq!(packed.fingerprint, source.fingerprint);
        assert_eq!(packed.asset("textures.rgba").unwrap(), source.asset("textures.rgba").unwrap());
        std::fs::write(root.join("textures.rgba.zlib"), b"garbage").unwrap();
        assert!(packed.asset("textures.rgba").is_err(), "a corrupted payload is refused on reopen");
        assert!(MapPack::load(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Authored models: placements need `models.glb`, the file must match its
    /// hash, and the stand-in run must be whole triangles inside the vertices.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn model_placements_need_their_file_and_a_whole_standin_run() {
        let source = on(crate::terrain::MapId::OldHoller).unwrap();
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("peakrunner-models-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for name in source.manifest.files.keys() {
            std::fs::write(root.join(name), read_payload(&source.root, name, 128_000_000).unwrap()).unwrap();
        }
        let glb = b"glTF\x02\0\0\0 not checked here".to_vec();
        std::fs::write(root.join("models.glb"), &glb).unwrap();
        let base: serde_json::Value = serde_json::from_slice(&read_payload(&source.root, "map.json", 8_000_000).unwrap()).unwrap();
        let vertices = read_payload(&source.root, "vertices.bin", 128_000_000).unwrap().len() as u64 / 48;
        let load = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut m = base.clone();
            m["files"]["models.glb"] = format!("{:x}", Sha256::digest(&glb)).into();
            m["models"] = serde_json::json!([{"model": "ship", "position": [1024.0, 80.0, 700.0], "yaw": 180.0, "team": 0}]);
            m["standin"] = serde_json::json!([vertices - 6, 6]);
            edit(&mut m);
            std::fs::write(root.join("map.json"), serde_json::to_vec(&m).unwrap()).unwrap();
            MapPack::load(&root)
        };
        let pack = load(&|_| {}).unwrap();
        assert_eq!(pack.manifest.models[0].model, "ship");
        assert_eq!(pack.manifest.standin, Some([vertices as u32 - 6, 6]));
        assert!(load(&|m| { m["files"].as_object_mut().unwrap().remove("models.glb"); }).is_err(), "placements need models.glb");
        assert!(load(&|m| m["files"]["models.glb"] = "00".repeat(32).into()).is_err(), "the file must match its hash");
        assert!(load(&|m| m["models"][0]["team"] = 2.into()).is_err(), "teams are 0 and 1");
        assert!(load(&|m| m["models"][0]["model"] = "".into()).is_err());
        assert!(load(&|m| m["standin"] = serde_json::json!([vertices - 6, 4])).is_err(), "whole triangles");
        assert!(load(&|m| m["standin"] = serde_json::json!([vertices - 3, 6])).is_err(), "inside the vertices");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loaded_pack_is_valid_when_requested() {
        if let Some(pack)=on(crate::terrain::MapId::OldHoller) {
            assert!(pack.triangles.len()>1000);
            for flag in pack.manifest.flags {
                let flag=Vec3::from_array(flag);
                let (floor,_)=pack.floor(flag+Vec3::Y*0.5).expect("flag platform");
                assert!((flag.y-floor).abs()<3.0,"flag {flag:?} floor {floor}");
            }
            for ember in [true,false] {
                let spawn=crate::terrain::spawn_on(crate::terrain::MapId::OldHoller,ember);
                assert!(spawn.is_finite());
                let clearance=if pack.manifest.exact_spawns {2.0} else {80.0};
                assert!(pack.sweep(spawn,spawn+Vec3::Y*clearance,0.52).is_none(),"spawn must not be inside a structure");
            }
            for &hole in &pack.manifest.holes {
                let step=pack.manifest.terrain_step;
                let x=(hole%256) as f32*step+step/2.;let z=(hole/256) as f32*step+step/2.;
                let y=crate::terrain::height_on(crate::terrain::MapId::OldHoller,x,z);
                assert!(crate::terrain::segment_hit(crate::terrain::MapId::OldHoller,
                    Vec3::new(x,y+2.0,z),Vec3::new(x,y-2.0,z),0.0).is_none(),"entrance terrain cutout");
            }
            assert!(pack.asset("../map.json").is_err());
        }
    }
}
