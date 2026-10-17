//! Exports Burnout Paradise track units (TRK_UNIT*_GR.BNDL) to glTF 2.0 with
//! textures, as the neutral intermediate on the way to GTA V formats.

use crate::tex;
use anyhow::{Context, Result};
use bnd2::Bundle;
use bpworld::{self as bw, types, Res};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Bundles that hold world textures referenced (externally) by track units.
const TEXTURE_BUNDLES: &[&str] = &[
    "WORLDTEX0.BIN",
    "WORLDTEX1.BIN",
    "WORLDTEX2.BIN",
    "WORLDTEX3.BIN",
    "GLOBALTEXTUREDICTIONARY.BIN",
    "MASSIVETEXTUREDICTIONARY.BIN",
];

pub struct TextureIndex {
    bundles: Vec<Bundle>,
    by_id: HashMap<u64, usize>,
}

impl TextureIndex {
    pub fn load(game: &Path) -> Result<Self> {
        let mut bundles = Vec::new();
        let mut by_id = HashMap::new();
        for name in TEXTURE_BUNDLES {
            let p = game.join(name);
            if !p.exists() {
                continue;
            }
            let b = Bundle::open(&p)?;
            for e in b.entries.iter().filter(|e| e.type_id == types::TEXTURE) {
                by_id.insert(e.id, bundles.len());
            }
            bundles.push(b);
        }
        Ok(TextureIndex { bundles, by_id })
    }

    /// Short name of a texture, if it is in one of the texture bundles.
    pub fn name(&self, id: u64) -> Option<String> {
        let (b, _) = self.get(id)?;
        Some(short_name(b.name_of(id)?))
    }

    fn get(&self, id: u64) -> Option<(&Bundle, &bnd2::Entry)> {
        let b = &self.bundles[*self.by_id.get(&id)?];
        Some((b, b.find(id)?))
    }
}

/// Burnout terrain shaders blend three layers; the first is used as diffuse.
const SAMPLER_TERRAIN_LAYER0: u32 = 0x39B8_7765;
/// Road shaders' line-marking layer (alpha-tested, second UV set).
pub const SAMPLER_LINE_LAYER: u32 = 0xC595_7B0C;
/// Slot roles that are never a diffuse map: reflection cube map, specular,
/// detail map, normal maps, illuminance map, light map, tint mask.
const NON_DIFFUSE_SLOTS: &[u32] =
    &[0xD0F6_5026, 0x610F_08DB, 0xFB9B_1028, 0x3E15_746F, 0xAB5C_9D24, 0x61D1_EACC, 0x45E5_E34D, 0x601E_F771, 0xB2F3_0F83];

#[derive(Debug, Default, Clone)]
pub struct MaterialInfo {
    /// Burnout shader name, e.g. "Specular_1Bit_Doublesided".
    pub shader: String,
    pub diffuse: Option<u64>,
    pub line: Option<u64>,
}

impl MaterialInfo {
    pub fn double_sided(&self) -> bool {
        self.shader.contains("Doublesided")
    }
    pub fn alpha_tested(&self) -> bool {
        self.shader.contains("1Bit")
    }
    pub fn transparent(&self) -> bool {
        self.shader.contains("Transparent")
    }
    /// Multiplicative (darkening) overlays such as road grime and graffiti.
    pub fn greyscale(&self) -> bool {
        self.shader.contains("Greyscale")
    }
}

/// Short readable name from a `gamedb://.../name.Type?ID=n` debug path.
pub(crate) fn short_name(full: &str) -> String {
    let base = full.rsplit('/').next().unwrap_or(full);
    let base = base.split('?').next().unwrap_or(base);
    let base = base.split('.').next().unwrap_or(base);
    base.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// Picks the texture most likely to be the diffuse map from a material's
/// sampler textures: skip normal maps (BC5 or *_n/_normal names) and
/// specular/mask-style maps where possible.
fn pick_diffuse(cands: &[(u64, String, u32)]) -> Option<u64> {
    let lower = |s: &str| s.to_ascii_lowercase();
    let is_normal = |n: &str, f: u32| {
        let n = lower(n);
        (82..=84).contains(&f) || n.ends_with("_n") || n.ends_with("_nm") || n.contains("normal") || n.contains("_bump")
    };
    let is_aux = |n: &str| {
        let n = lower(n);
        n.ends_with("_s") || n.ends_with("_spec") || n.ends_with("_sp") || n.contains("specular") || n.contains("mask")
    };
    cands
        .iter()
        .find(|(_, n, f)| !is_normal(n, *f) && !is_aux(n))
        .or_else(|| cands.iter().find(|(_, n, f)| !is_normal(n, *f)))
        .or(cands.first())
        .map(|c| c.0)
}

#[derive(Default)]
pub(crate) struct Gltf {
    pub bin: Vec<u8>,
    pub buffer_views: Vec<Value>,
    pub accessors: Vec<Value>,
    pub meshes: Vec<Value>,
    pub materials: Vec<Value>,
    pub textures: Vec<Value>,
    pub images: Vec<Value>,
    pub nodes: Vec<Value>,
}

impl Gltf {
    fn push_view(&mut self, bytes: &[u8], target: u32) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let off = self.bin.len();
        self.bin.extend_from_slice(bytes);
        self.buffer_views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": bytes.len(), "target": target}));
        self.buffer_views.len() - 1
    }

    pub fn push_vec3(&mut self, v: &[[f32; 3]]) -> usize {
        let bytes: Vec<u8> = v.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let view = self.push_view(&bytes, 34962);
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for p in v {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        self.accessors.push(json!({"bufferView": view, "componentType": 5126, "count": v.len(), "type": "VEC3", "min": min, "max": max}));
        self.accessors.len() - 1
    }

    pub fn push_vec2(&mut self, v: &[[f32; 2]]) -> usize {
        let bytes: Vec<u8> = v.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
        let view = self.push_view(&bytes, 34962);
        self.accessors.push(json!({"bufferView": view, "componentType": 5126, "count": v.len(), "type": "VEC2"}));
        self.accessors.len() - 1
    }

    pub fn push_indices(&mut self, v: &[u32]) -> usize {
        let bytes: Vec<u8> = v.iter().flat_map(|i| i.to_le_bytes()).collect();
        let view = self.push_view(&bytes, 34963);
        self.accessors.push(json!({"bufferView": view, "componentType": 5125, "count": v.len(), "type": "SCALAR"}));
        self.accessors.len() - 1
    }
}

pub struct Exporter<'a> {
    pub game: PathBuf,
    pub out: PathBuf,
    pub textures: &'a TextureIndex,
    /// Lightweight mode for whole-map previews: lowest LOD and flat colours
    /// (each texture's average) instead of texture images.
    pub overview: bool,
    /// texture id -> file stem already written (shared across units)
    written: HashMap<u64, Option<(String, bool)>>,
    avg_colour: HashMap<u64, Option<[f32; 4]>>,
}

impl<'a> Exporter<'a> {
    pub fn new(game: PathBuf, out: PathBuf, textures: &'a TextureIndex, overview: bool) -> Self {
        Exporter { game, out, textures, overview, written: HashMap::new(), avg_colour: HashMap::new() }
    }

    /// Average linear-space colour of a texture's top mip.
    fn texture_average(&mut self, id: u64) -> Option<[f32; 4]> {
        if let Some(c) = self.avg_colour.get(&id) {
            return *c;
        }
        let c = self.textures.get(id).and_then(|(b, e)| {
            let rgba = tex::decode_rgba(&bw::parse_texture(b, e).ok()?).ok()?;
            let n = (rgba.len() / 4).max(1) as f64;
            let mut sum = [0f64; 3];
            for px in rgba.chunks(4) {
                for k in 0..3 {
                    sum[k] += (px[k] as f64 / 255.0).powf(2.2);
                }
            }
            Some([(sum[0] / n) as f32, (sum[1] / n) as f32, (sum[2] / n) as f32, 1.0])
        });
        self.avg_colour.insert(id, c);
        c
    }

    /// Writes PNG + DDS for a texture once; returns (file stem, has alpha).
    fn texture_file(&mut self, id: u64) -> Option<(String, bool)> {
        if let Some(w) = self.written.get(&id) {
            return w.clone();
        }
        let res = (|| -> Result<(String, bool)> {
            let (b, e) = self.textures.get(id).context("texture not found")?;
            let t = bw::parse_texture(b, e)?;
            let stem = format!("{}_{:08X}", short_name(b.name_of(id).unwrap_or("tex")), id as u32);
            let dir = self.out.join("textures");
            std::fs::create_dir_all(&dir)?;
            tex::write_dds(&t, &dir.join(format!("{stem}.dds")))?;
            tex::write_png(&t, &dir.join(format!("{stem}.png")))?;
            // BC1 can carry 1-bit (punch-through) alpha, e.g. road line
            // layers, so check the pixels rather than the format alone.
            let alpha = tex::has_alpha(t.dxgi_format)
                || (matches!(t.dxgi_format, 70..=72) && tex::decode_rgba(&t).is_ok_and(|p| p.chunks(4).any(|px| px[3] < 128)));
            Ok((stem, alpha))
        })();
        let r = match res {
            Ok(v) => Some(v),
            Err(err) => {
                eprintln!("  warning: texture {id:016X}: {err:#}");
                None
            }
        };
        self.written.insert(id, r.clone());
        r
    }

    /// A material's shader name and its diffuse / line-layer texture ids,
    /// chosen by texture-slot role (see `bw::material_samplers`). Falls back
    /// to texture-name heuristics only for slots of unknown role, and never
    /// uses cube maps, normal, detail, specular or light maps as diffuse.
    pub fn material_info(&self, b: &Bundle, mat_id: u64) -> MaterialInfo {
        let mut info = MaterialInfo::default();
        let Some(e) = b.find(mat_id) else { return info };
        let Ok(r) = Res::load(b, e) else { return info };
        info.shader = bw::material_shader(&r)
            .and_then(|id| b.name_of(id))
            .map(|n| n.rsplit('/').next().unwrap_or(n).split(".fx").next().unwrap_or(n).to_string())
            .unwrap_or_default();
        let tex_of = |ts: Option<u64>| -> Option<u64> {
            let ts_r = Res::load(b, b.find(ts?)?).ok()?;
            bw::texture_state_texture(&ts_r).filter(|id| self.textures.get(*id).is_some())
        };
        let slots: Vec<(u32, Option<u64>)> = bw::material_samplers(&r).into_iter().map(|(h, ts)| (h, tex_of(ts))).collect();
        let slot = |h: u32| slots.iter().find(|s| s.0 == h).and_then(|s| s.1);
        info.diffuse = slot(bw::SAMPLER_DIFFUSE).or_else(|| slot(bw::SAMPLER_DIFFUSE_ROAD)).or_else(|| slot(SAMPLER_TERRAIN_LAYER0));
        info.line = slot(SAMPLER_LINE_LAYER);
        if info.diffuse.is_none() {
            let cands: Vec<(u64, String, u32)> = slots
                .iter()
                .filter(|s| !NON_DIFFUSE_SLOTS.contains(&s.0))
                .filter_map(|s| {
                    let id = s.1?;
                    let (tb, te) = self.textures.get(id)?;
                    let fmt = bw::parse_texture(tb, te).map(|t| t.dxgi_format).unwrap_or(0);
                    Some((id, short_name(tb.name_of(id).unwrap_or("")), fmt))
                })
                .collect();
            info.diffuse = pick_diffuse(&cands);
        }
        info
    }

    /// Writes a texture's DDS/PNG on first use: (file stem, has alpha).
    pub fn texture(&mut self, id: u64) -> Option<(String, bool)> {
        self.texture_file(id)
    }

    /// Decoded top mip of a texture: (width, height, RGBA8).
    pub fn texture_rgba(&self, id: u64) -> Option<(u32, u32, Vec<u8>)> {
        let (b, e) = self.textures.get(id)?;
        let t = bw::parse_texture(b, e).ok()?;
        let rgba = tex::decode_rgba(&t).ok()?;
        Some((t.width, t.height, rgba))
    }

    pub fn export_unit(&mut self, unit: u32) -> Result<()> {
        let path = self.game.join(format!("TRK_UNIT{unit}_GR.BNDL"));
        let b = Bundle::open(&path)?;
        let Some(list) = b.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else {
            println!("unit {unit}: no instance list, skipping");
            return Ok(());
        };
        let instances = bw::parse_instance_list(&Res::load(&b, list)?);

        let mut g = Gltf::default();
        let mut mesh_for_model: HashMap<u64, Option<usize>> = HashMap::new();
        let mut material_index: HashMap<u64, usize> = HashMap::new();
        let mut image_index: HashMap<String, usize> = HashMap::new();
        let mut tri_count = 0usize;

        for inst in &instances {
            if !mesh_for_model.contains_key(&inst.model) {
                let mesh = self.build_mesh(&b, inst.model, &mut g, &mut material_index, &mut image_index, &mut tri_count);
                let mesh = match mesh {
                    Ok(m) => m,
                    Err(err) => {
                        eprintln!("  warning: model {:016X}: {err:#}", inst.model);
                        None
                    }
                };
                mesh_for_model.insert(inst.model, mesh);
            }
            let Some(mesh) = mesh_for_model[&inst.model] else { continue };
            let mut m = inst.transform;
            m[3] = 0.0;
            m[7] = 0.0;
            m[11] = 0.0;
            m[15] = 1.0;
            let name = b.name_of(inst.model).map(short_name).unwrap_or_default();
            g.nodes.push(json!({"name": name, "mesh": mesh, "matrix": m}));
        }

        let dir = self.out.join(format!("unit{unit}"));
        std::fs::create_dir_all(&dir)?;
        let stem = format!("unit{unit}");
        std::fs::write(dir.join(format!("{stem}.bin")), &g.bin)?;
        let children: Vec<usize> = (0..g.nodes.len()).collect();
        let doc = json!({
            "asset": {"version": "2.0", "generator": "gta-paradise bpx"},
            "scene": 0,
            "scenes": [{"nodes": children}],
            "nodes": g.nodes,
            "meshes": g.meshes,
            "materials": g.materials,
            "textures": g.textures,
            "images": g.images,
            "samplers": [{"wrapS": 10497, "wrapT": 10497}],
            "accessors": g.accessors,
            "bufferViews": g.buffer_views,
            "buffers": [{"uri": format!("{stem}.bin"), "byteLength": g.bin.len()}],
        });
        std::fs::write(dir.join(format!("{stem}.gltf")), serde_json::to_vec(&doc)?)?;
        println!(
            "unit {unit}: {} instances, {} unique models, {} materials, {} triangles",
            instances.len(),
            g.meshes.len(),
            g.materials.len(),
            tri_count
        );
        Ok(())
    }

    fn build_mesh(
        &mut self,
        b: &Bundle,
        model_id: u64,
        g: &mut Gltf,
        material_index: &mut HashMap<u64, usize>,
        image_index: &mut HashMap<String, usize>,
        tri_count: &mut usize,
    ) -> Result<Option<usize>> {
        let model_e = b.find(model_id).context("model not in bundle")?;
        let model = bw::parse_model(&Res::load(b, model_e)?);
        let rend = if self.overview { model.lowest_lod() } else { model.lod0() };
        let Some(rend_e) = rend.and_then(|id| b.find(id)) else {
            return Ok(None);
        };
        let geoms = bw::decode_renderable(b, rend_e)?;
        let mut prims = Vec::new();
        for geo in geoms.iter().filter(|g| !g.indices.is_empty()) {
            let pos = g.push_vec3(&geo.positions);
            let mut attrs = json!({"POSITION": pos});
            if geo.normals.len() == geo.positions.len() {
                attrs["NORMAL"] = json!(g.push_vec3(&geo.normals));
            }
            if geo.uvs.len() == geo.positions.len() {
                attrs["TEXCOORD_0"] = json!(g.push_vec2(&geo.uvs));
            }
            let idx = g.push_indices(&geo.indices);
            *tri_count += geo.indices.len() / 3;
            let mut prim = json!({"attributes": attrs, "indices": idx});
            if let Some(mat) = geo.material {
                prim["material"] = json!(self.material(b, mat, g, material_index, image_index));
            }
            prims.push(prim);
        }
        if prims.is_empty() {
            return Ok(None);
        }
        g.meshes.push(json!({"name": b.name_of(model_id).map(short_name), "primitives": prims}));
        Ok(Some(g.meshes.len() - 1))
    }

    fn material(
        &mut self,
        b: &Bundle,
        mat_id: u64,
        g: &mut Gltf,
        material_index: &mut HashMap<u64, usize>,
        image_index: &mut HashMap<String, usize>,
    ) -> usize {
        if let Some(&i) = material_index.get(&mat_id) {
            return i;
        }
        let name = b.name_of(mat_id).map(short_name).unwrap_or_else(|| format!("mat_{mat_id:08X}"));
        let info = self.material_info(b, mat_id);
        let diffuse = info.diffuse;
        let mut m = json!({"name": name, "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.9}});
        if self.overview {
            if let Some(c) = diffuse.and_then(|id| self.texture_average(id)) {
                m["pbrMetallicRoughness"]["baseColorFactor"] = json!(c);
            }
        } else if let Some((stem, alpha)) = diffuse.and_then(|id| self.texture_file(id)) {
            let img = *image_index.entry(stem.clone()).or_insert_with(|| {
                g.images.push(json!({"uri": format!("../textures/{stem}.png")}));
                g.textures.push(json!({"source": g.images.len() - 1, "sampler": 0}));
                g.textures.len() - 1
            });
            m["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index": img});
            // Cut out only where Burnout's shader alpha-tests ("1Bit"), as in
            // gtavis: road textures keep other data in alpha, and BC1 ones
            // of opaque materials can have stray punch-through texels.
            if alpha && info.alpha_tested() {
                m["alphaMode"] = json!("MASK");
                m["alphaCutoff"] = json!(0.5);
            }
        }
        g.materials.push(m);
        material_index.insert(mat_id, g.materials.len() - 1);
        g.materials.len() - 1
    }
}
