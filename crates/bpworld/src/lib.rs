//! Parsers for the Burnout Paradise Remastered (PC, 32-bit pointers) world
//! resources needed to rebuild the map: InstanceList -> Model -> Renderable
//! -> (VertexDescriptor, Material -> TextureState -> Texture).
//!
//! Pointers inside a resource's main chunk are byte offsets into that chunk.
//! Pointers to other resources are zero on disk and resolved through the
//! resource's import table (pointer offset -> resource id).

use anyhow::{bail, Context, Result};
use bnd2::{f32_at, u16_at, u32_at, Bundle, Entry};
use std::collections::HashMap;

/// Resource ids are the CRC32 of the lowercased resource name
/// (e.g. "trk_col_373" -> 0x005ACF8F).
pub fn resource_id(name: &str) -> u64 {
    let mut c = flate2::Crc::new();
    c.update(name.to_ascii_lowercase().as_bytes());
    c.sum() as u64
}

/// Track unit number of a `TRK_COL_n` collision resource, using the debug
/// name when present and otherwise matching the id against name hashes
/// (bundles written by tools often omit debug data).
pub fn collision_unit(b: &Bundle, id: u64) -> Option<u32> {
    if let Some(n) = b.name_of(id).and_then(|n| n.strip_prefix("TRK_COL_")) {
        return n.parse().ok();
    }
    (0..2048).find(|n| resource_id(&format!("trk_col_{n}")) == id)
}

/// One collision polygon (triangle or quad) in world space.
#[derive(Debug, Clone)]
pub struct CollisionPoly {
    pub verts: Vec<[f32; 3]>,
    /// Raw 32-bit collision tag as read little-endian (see [`CollisionTag`]).
    pub tag: u32,
}

/// World collision for one track unit (`TRK_COL_n` in WORLDCOL.BIN), a
/// PolygonSoupList: soups of up to 255 polys sharing quantised u16 vertices.
#[derive(Debug, Clone)]
pub struct CollisionMesh {
    pub aabb_min: [f32; 3],
    pub aabb_max: [f32; 3],
    pub polys: Vec<CollisionPoly>,
}

pub fn parse_polygon_soup_list(r: &Res) -> CollisionMesh {
    let v3 = |o: usize| [r.f32(o), r.f32(o + 4), r.f32(o + 8)];
    let mut mesh = CollisionMesh { aabb_min: v3(0), aabb_max: v3(0x10), polys: Vec::new() };
    if r.main.len() < 0x30 {
        return mesh;
    }
    let soups = r.u32(0x20) as usize;
    let n = r.u32(0x28) as usize;
    for i in 0..n {
        let s = r.u32(soups + i * 4) as usize;
        let offs = [r.u32(s) as i32, r.u32(s + 4) as i32, r.u32(s + 8) as i32];
        let gran = r.f32(s + 0xC);
        let polys = r.u32(s + 0x10) as usize;
        let verts = r.u32(s + 0x14) as usize;
        let total = r.u8(s + 0x1A) as usize;
        let quads = r.u8(s + 0x1B) as usize;
        let nv = r.u8(s + 0x1C) as usize;
        let vert = |k: usize| -> Option<[f32; 3]> {
            (k < nv).then(|| {
                let o = verts + k * 6;
                [0, 1, 2].map(|a| (offs[a] + r.u16(o + a * 2) as i32) as f32 * gran)
            })
        };
        for p in 0..total {
            let o = polys + p * 12;
            // Quad corners are stored in triangle-strip order (0,1,3,2 around
            // the edge; verified on ~37k quads), so read them back in
            // perimeter order: consumers then split quads as (0,1,2)+(0,2,3).
            let order: &[usize] = if p < quads { &[0, 1, 3, 2] } else { &[0, 1, 2] };
            let idx = order.iter().map(|&c| r.u8(o + 4 + c) as usize);
            let Some(verts) = idx.map(vert).collect::<Option<Vec<_>>>() else { continue };
            mesh.polys.push(CollisionPoly { verts, tag: r.u32(o) });
        }
    }
    mesh
}

/// Meaning of each collision surface id and the closest GTA V collision
/// material (name, index in GTA V Legacy's common.rpf materials.dat v12).
/// Inferred with `bpx surfaces` (nearest visual material names, verticality
/// and flags per id); SURFACELIST.BIN only has hashed names.
pub const SURFACES: [(&str, &str, u16); 20] = [
    ("asphalt_island", "TARMAC", 4),
    ("asphalt_road", "TARMAC", 4),
    ("concrete", "CONCRETE", 1),
    ("rock_dirt", "ROCK", 9),
    ("gravel", "GRAVEL_SMALL", 31),
    ("cobbles", "COBBLESTONE", 12),
    ("stone", "STONE", 11),
    ("chainlink_fence", "METAL_CHAINLINK_LARGE", 62),
    ("metal", "METAL_SOLID_MEDIUM", 56),
    ("wood_light", "WOOD_SOLID_SMALL", 69),
    ("wood", "WOOD_SOLID_MEDIUM", 70),
    ("sand", "SAND_LOOSE", 18),
    ("foliage", "BUSHES", 50),
    ("plastic_fabric", "PLASTIC", 86),
    ("glass", "GLASS_SHOOT_THROUGH", 112),
    ("water", "WATER", 125),
    ("rail_track", "GRAVEL_TRAIN_TRACK", 34),
    ("invisible_wall", "CONCRETE", 1),
    ("grass_terrain", "GRASS", 47),
    ("pavement", "CONCRETE_PAVEMENT", 174),
];

pub const SURFACE_WATER: u8 = 15;
pub const SURFACE_INVISIBLE_WALL: u8 = 17;

pub fn surface_name(id: u8) -> &'static str {
    SURFACES.get(id as usize).map_or("unknown", |s| s.0)
}

pub fn surface_gta_material(id: u8) -> &'static str {
    SURFACES.get(id as usize).map_or("CONCRETE", |s| s.1)
}

pub fn surface_gta_material_index(id: u8) -> u16 {
    SURFACES.get(id as usize).map_or(1, |s| s.2)
}

/// Decoded collision tag. Read as a little-endian u32, the high half is the
/// material tag and the low half the group (AI section) tag. The wiki numbers
/// bitfields from the most significant bit, so "bit 0" is 0x8000.
#[derive(Debug, Clone, Copy)]
pub struct CollisionTag {
    pub surface_id: u8,
    /// KU_COLLISION_FLAG_* bits (0x1000 superfatal, 0x2000 driveable, 0x4000 fatal).
    pub flags: u16,
    /// Lane direction info: 0 = no lanes, 1 = unknown, else angle (v - 2) / 14 * 2pi.
    pub traffic: u8,
    /// 0x7FFF when the polygon belongs to no AI section.
    pub ai_section: u16,
}

impl CollisionTag {
    pub fn decode(tag: u32) -> Self {
        let material = (tag >> 16) as u16;
        let group = (tag & 0xFFFF) as u16;
        CollisionTag {
            surface_id: ((material >> 4) & 0x3F) as u8,
            flags: material & 0x7000,
            traffic: (material & 0xF) as u8,
            ai_section: group & 0x7FFF,
        }
    }
}

mod shader_roles;
pub mod traffic;

pub mod types {
    pub const TEXTURE: u32 = 0x0;
    pub const MATERIAL: u32 = 0x1;
    pub const VERTEX_DESCRIPTOR: u32 = 0xA;
    pub const RENDERABLE: u32 = 0xC;
    pub const TEXTURE_STATE: u32 = 0xE;
    pub const INSTANCE_LIST: u32 = 0x23;
    pub const MODEL: u32 = 0x2A;
    pub const POLYGON_SOUP_LIST: u32 = 0x43;
}

/// A resource's main chunk plus its import table keyed by pointer offset.
pub struct Res {
    pub id: u64,
    pub main: Vec<u8>,
    pub imports: HashMap<u32, u64>,
    pub import_list: Vec<(u64, u32)>,
}

impl Res {
    pub fn load(b: &Bundle, e: &Entry) -> Result<Self> {
        let main = b.chunk(e, 0)?;
        let import_list = b.imports(e, &main);
        let imports = import_list.iter().map(|&(id, off)| (off, id)).collect();
        Ok(Res { id: e.id, main, imports, import_list })
    }
    pub fn import(&self, off: usize) -> Option<u64> {
        self.imports.get(&(off as u32)).copied()
    }
    fn u8(&self, o: usize) -> u8 {
        self.main[o]
    }
    fn u16(&self, o: usize) -> u16 {
        u16_at(&self.main, o)
    }
    fn u32(&self, o: usize) -> u32 {
        u32_at(&self.main, o)
    }
    fn f32(&self, o: usize) -> f32 {
        f32_at(&self.main, o)
    }
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub model: u64,
    /// Row-major 4x4 affine matrix; translation in the last row (D3D style).
    pub transform: [f32; 16],
}

pub fn parse_instance_list(r: &Res) -> Vec<Instance> {
    let arr = r.u32(0) as usize;
    let n = r.u32(8) as usize;
    (0..n)
        .filter_map(|i| {
            let o = arr + i * 0x50;
            let model = r.import(o)?;
            let mut transform = [0.0; 16];
            for (k, t) in transform.iter_mut().enumerate() {
                *t = r.f32(o + 0x10 + k * 4);
            }
            Some(Instance { model, transform })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Model {
    pub renderables: Vec<u64>,
    /// Renderable index for each state (state 0 = highest detail LOD).
    pub state_renderable: Vec<u8>,
    pub lod_distances: Vec<f32>,
}

pub fn parse_model(r: &Res) -> Model {
    let arr = r.u32(0) as usize;
    let states = r.u32(4) as usize;
    let lods = r.u32(8) as usize;
    let n = r.u8(0x10) as usize;
    let ns = r.u8(0x12) as usize;
    Model {
        renderables: (0..n).filter_map(|i| r.import(arr + i * 4)).collect(),
        state_renderable: (0..ns).map(|i| r.u8(states + i)).collect(),
        lod_distances: (0..ns).map(|i| r.f32(lods + i * 4)).collect(),
    }
}

impl Model {
    pub fn lod0(&self) -> Option<u64> {
        let idx = self.state_renderable.first().copied().unwrap_or(0) as usize;
        self.renderables.get(idx).or(self.renderables.first()).copied()
    }

    /// Lowest-detail renderable among the LOD states (states 0..16).
    pub fn lowest_lod(&self) -> Option<u64> {
        let lods = self.state_renderable.iter().take(16).filter(|&&i| (i as usize) < self.renderables.len());
        lods.last().and_then(|&i| self.renderables.get(i as usize)).copied().or_else(|| self.lod0())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    Position,
    Normal,
    Tangent,
    TexCoord(u8),
    Color,
    Other(u8),
}

#[derive(Debug, Clone)]
pub struct VertexElement {
    pub usage: Usage,
    pub format: u32,
    pub offset: u32,
    pub stride: u32,
}

/// Element usage codes are the wiki's "indexed usage" table shifted by one
/// (1 = POSITION, 3 = NORMAL, 5.. = TEXCOORD0.., 15 = TANGENT), confirmed by
/// the descriptor's usage bitmask.
pub fn parse_vertex_descriptor(r: &Res) -> Vec<VertexElement> {
    let n = r.u8(0xC) as usize;
    (0..n)
        .map(|i| {
            let o = 0x10 + i * 0x14;
            let usage = match r.u8(o) {
                1 => Usage::Position,
                3 => Usage::Normal,
                4 | 17 => Usage::Color,
                u @ 5..=12 => Usage::TexCoord(u - 5),
                15 => Usage::Tangent,
                u => Usage::Other(u),
            };
            VertexElement { usage, format: r.u32(o + 4), offset: r.u32(o + 8), stride: r.u32(o + 0x10) }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub topology: u32,
    pub base_vertex: i32,
    pub start_index: u32,
    pub index_count: u32,
    pub material: Option<u64>,
    pub vertex_descriptors: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct Renderable {
    pub meshes: Vec<Mesh>,
    pub index_size: u32,
    pub index_range: (usize, usize),
    pub vertex_range: (usize, usize),
}

pub fn parse_renderable(r: &Res) -> Result<Renderable> {
    let version = r.u16(0x10);
    if version != 11 {
        bail!("renderable {:016X}: unsupported version {version}", r.id);
    }
    let n = r.u16(0x12) as usize;
    let meshes_ptr = r.u32(0x14) as usize;
    let ib = r.u32(0x20) as usize;
    let vb = r.u32(0x24) as usize;
    let meshes = (0..n)
        .map(|i| {
            let m = r.u32(meshes_ptr + i * 4) as usize;
            let nvd = r.u8(m + 0x54) as usize;
            Mesh {
                topology: r.u32(m + 0x40),
                base_vertex: r.u32(m + 0x44) as i32,
                start_index: r.u32(m + 0x48),
                index_count: r.u32(m + 0x4C),
                material: r.import(m + 0x50),
                vertex_descriptors: (0..nvd).filter_map(|k| r.import(m + 0x60 + k * 4)).collect(),
            }
        })
        .collect();
    Ok(Renderable {
        meshes,
        index_size: r.u32(ib + 0x14),
        index_range: (r.u32(ib + 0xC) as usize, r.u32(ib + 0x10) as usize),
        vertex_range: (r.u32(vb + 0xC) as usize, r.u32(vb + 0x10) as usize),
    })
}

/// A material's texture slots: (slot role hash, TextureState id). Samplers
/// are 0x14-byte records at the pointer in 0xC (count i8 at 0x9): role hash
/// at +0x4, imported TextureState at +0x10. Observed role hashes:
/// 0x6A640902 diffuse, 0xC6BA11F3 diffuse (road detail-map shaders),
/// 0xFB9B1028 detail map, 0x610F08DB specular, 0x61D1EACC illuminance map,
/// 0xD0F65026 reflection cube map.
///
/// A modded map's rebuilt bundles can have the role hashes zeroed (and the
/// records reordered): those are filled in from the material's shader by the
/// slot number at +0x8 (shader_roles.rs).
pub fn material_samplers(r: &Res) -> Vec<(u32, Option<u64>)> {
    if r.main.len() < 0x10 {
        return Vec::new();
    }
    let n = r.u8(0x9) as usize;
    let base = r.u32(0xC) as usize;
    let roles = material_shader(r).and_then(|id| shader_roles::SHADER_SLOT_ROLES.iter().find(|(s, _)| *s == id)).map(|(_, r)| *r);
    (0..n)
        .filter_map(|i| {
            let o = base + i * 0x14;
            if o + 0x14 > r.main.len() {
                return None;
            }
            // Zeroed: the role of the shader slot this sampler binds (its
            // number at +0x8; the order of the records can differ).
            let role = match r.u32(o + 4) {
                0 => roles.and_then(|l| l.iter().find(|(slot, _)| *slot == r.u32(o + 8))).map_or(0, |(_, role)| *role),
                h => h,
            };
            Some((role, r.import(o + 0x10)))
        })
        .collect()
}

/// The shader resource a material uses (an external import at 0x10); its
/// debug name, e.g. ".../Specular_Opaque_Singlesided.fx.Shader", describes
/// blending (Opaque/1Bit/Greyscale/Transparent) and sidedness.
pub fn material_shader(r: &Res) -> Option<u64> {
    r.import(0x10)
}

pub const SAMPLER_DIFFUSE: u32 = 0x6A64_0902;
pub const SAMPLER_DIFFUSE_ROAD: u32 = 0xC6BA_11F3;

/// Texture-state ids referenced by a material, in sampler order.
pub fn material_texture_states(b: &Bundle, r: &Res) -> Vec<u64> {
    r.import_list
        .iter()
        .filter(|(id, _)| b.find(*id).is_some_and(|e| e.type_id == types::TEXTURE_STATE))
        .map(|&(id, _)| id)
        .collect()
}

pub fn texture_state_texture(r: &Res) -> Option<u64> {
    r.import(0x38)
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub dxgi_format: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mips: u32,
    pub data: Vec<u8>,
}

pub fn parse_texture(b: &Bundle, e: &Entry) -> Result<Texture> {
    let main = b.chunk(e, 0)?;
    if main.len() < 0x30 {
        bail!("texture {:016X} header too small", e.id);
    }
    Ok(Texture {
        dxgi_format: u32_at(&main, 0x1C),
        width: u16_at(&main, 0x24) as u32,
        height: u16_at(&main, 0x26) as u32,
        depth: u16_at(&main, 0x28) as u32,
        mips: main[0x2D].max(1) as u32,
        data: b.chunk(e, 1).context("texture pixels")?,
    })
}

/// Decoded geometry for one mesh, in the renderable's local space.
#[derive(Debug, Default, Clone)]
pub struct MeshGeometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Second texture coordinate set (e.g. the road line-marking layer).
    pub uvs1: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub material: Option<u64>,
}

/// Extracts triangle lists for every mesh of a renderable. Only the vertices
/// each mesh actually references are kept, so meshes are self-contained.
pub fn decode_renderable(b: &Bundle, e: &Entry) -> Result<Vec<MeshGeometry>> {
    let r = Res::load(b, e)?;
    let rend = parse_renderable(&r)?;
    let gfx = b.chunk(e, 1)?;
    let (ib_off, ib_len) = rend.index_range;
    let (vb_off, vb_len) = rend.vertex_range;
    let ib = gfx.get(ib_off..ib_off + ib_len).context("index buffer out of range")?;
    let vb = gfx.get(vb_off..vb_off + vb_len).context("vertex buffer out of range")?;
    let read_index = |i: usize| -> Option<u32> {
        match rend.index_size {
            2 => ib.get(i * 2..i * 2 + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as u32),
            _ => ib.get(i * 4..i * 4 + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())),
        }
    };

    let mut out = Vec::new();
    for mesh in &rend.meshes {
        let Some(vd_entry) = mesh.vertex_descriptors.first().and_then(|id| b.find(*id)) else {
            continue;
        };
        let elems = parse_vertex_descriptor(&Res::load(b, vd_entry)?);
        let find = |u: Usage| elems.iter().find(|el| el.usage == u);
        let Some(pos) = find(Usage::Position) else { continue };
        let stride = pos.stride as usize;
        let nrm = find(Usage::Normal);
        let uv = find(Usage::TexCoord(0));
        let uv1 = find(Usage::TexCoord(1));

        // Raw index list for this mesh, expanded to a triangle list.
        let raw: Vec<u32> = (0..mesh.index_count as usize)
            .filter_map(|i| read_index(mesh.start_index as usize + i))
            .map(|i| (i as i64 + mesh.base_vertex as i64) as u32)
            .collect();
        let tris: Vec<u32> = match mesh.topology {
            4 => raw,
            5 => strip_to_list(&raw),
            t => bail!("unsupported topology {t}"),
        };

        let mut g = MeshGeometry { material: mesh.material, ..Default::default() };
        let mut remap: HashMap<u32, u32> = HashMap::new();
        for &vi in &tris {
            let next = remap.len() as u32;
            let id = *remap.entry(vi).or_insert(next);
            if id == next {
                let base = vi as usize * stride;
                if base + stride > vb.len() {
                    bail!("vertex {vi} outside vertex buffer");
                }
                g.positions.push(read_vec3(vb, base + pos.offset as usize, pos.format));
                if let Some(n) = nrm {
                    g.normals.push(read_vec3(vb, base + n.offset as usize, n.format));
                }
                if let Some(t) = uv {
                    g.uvs.push(read_vec2(vb, base + t.offset as usize, t.format));
                }
                if let Some(t) = uv1 {
                    g.uvs1.push(read_vec2(vb, base + t.offset as usize, t.format));
                }
            }
            g.indices.push(id);
        }
        out.push(g);
    }
    Ok(out)
}

fn strip_to_list(s: &[u32]) -> Vec<u32> {
    let mut out = Vec::new();
    for i in 2..s.len() {
        let (a, b, c) = if i % 2 == 0 { (s[i - 2], s[i - 1], s[i]) } else { (s[i - 1], s[i - 2], s[i]) };
        if a != b && b != c && a != c {
            out.extend([a, b, c]);
        }
    }
    out
}

fn read_vec3(b: &[u8], o: usize, fmt: u32) -> [f32; 3] {
    match fmt {
        2 | 6 => [f32_at(b, o), f32_at(b, o + 4), f32_at(b, o + 8)],
        // R16G16B16A16_SNORM
        13 => {
            let s = |k: usize| i16::from_le_bytes([b[o + k * 2], b[o + k * 2 + 1]]) as f32 / 32767.0;
            [s(0), s(1), s(2)]
        }
        // R16G16B16A16_FLOAT
        10 => {
            let h = |k: usize| half_to_f32(u16::from_le_bytes([b[o + k * 2], b[o + k * 2 + 1]]));
            [h(0), h(1), h(2)]
        }
        // R8G8B8A8_SNORM / UNORM
        31 => [0, 1, 2].map(|k| b[o + k] as i8 as f32 / 127.0),
        28 => [0, 1, 2].map(|k| b[o + k] as f32 / 127.5 - 1.0),
        _ => [0.0; 3],
    }
}

fn read_vec2(b: &[u8], o: usize, fmt: u32) -> [f32; 2] {
    match fmt {
        16 | 2 | 6 => [f32_at(b, o), f32_at(b, o + 4)],
        34 => [0, 1].map(|k| half_to_f32(u16::from_le_bytes([b[o + k * 2], b[o + k * 2 + 1]]))),
        37 => [0, 1].map(|k| i16::from_le_bytes([b[o + k * 2], b[o + k * 2 + 1]]) as f32 / 32767.0),
        _ => [0.0; 2],
    }
}

fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1F) as i32;
    let man = (h & 0x3FF) as f32;
    match exp {
        0 => sign * man * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + man / 1024.0) * 2f32.powi(exp - 15),
    }
}
