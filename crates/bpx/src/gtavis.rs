//! Converts Burnout Paradise track units' visual models into GTA V map assets
//! as CodeWalker XML, for `cwconv convert`:
//! - shared texture dictionaries (`.ytd`) of at most ~48 MB: textures used
//!   across much of the map go into "common" groups, the rest are grouped by
//!   where on the map they are used, so nothing is duplicated and a unit only
//!   pulls in nearby groups (a single dictionary for all of a unit's textures
//!   exceeds GTA's resource size limit);
//! - per track unit, one drawable (`.ydr`) per texture group it uses, with
//!   every placed model instance baked in (LOD0), in GTA coordinates relative
//!   to the unit's centre;
//! - one map types file (`.ytyp`) with an archetype per drawable;
//! - one map data file (`.ymap`) per unit placing its drawables;
//! - Burnout's backdrops (GLOBALBACKDROPS.BNDL), placed by the entries past
//!   the end of each unit's instance list: the low-detail stand-in for each
//!   unit (`BdZone<n>`, Burnout's own LOD of unit n) becomes the GTA LOD
//!   parent of that unit's drawables, the mountains and other scenery that
//!   Burnout draws even from the units next to them are always drawn, and a
//!   big stand-in such as the whole of Big Surf Island becomes the parent of
//!   the units it covers that have no stand-in of their own (see
//!   `export_backdrops`). They share one texture dictionary of cut-down
//!   textures and live in one LOD map file.
//!
//! Burnout materials map onto GTA's diffuse-only shaders, with parameters and
//! vertex layout copied from Rockstar's own drawables:
//! - opaque -> `default.sps`; alpha-tested ("1Bit") -> `cutout.sps`;
//! - overlays -> alpha-blended `decal.sps`: road line markings (a second
//!   pass over the road using its second UV set), glass ("Transparent", a
//!   generated translucent tint), and multiplicative "Greyscale" grime and
//!   graffiti (converted to black with alpha = how much they darken);
//! - "Doublesided" materials get back faces, since GTA culls them.

use crate::export::{Exporter, MaterialInfo, TextureIndex};
use crate::gta::Placement;
use crate::tex;
use anyhow::{Context, Result};
use bnd2::Bundle;
use bpworld::{self as bw, types, Res};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// GTA geometries use 16-bit indices; stay safely below 65536 vertices.
const MAX_GEOM_VERTS: usize = 60000;

/// Neutral texture for Burnout materials without a usable diffuse map.
const WHITE_TEXTURE: &str = "bp_white";
/// Translucent tint for Burnout glass, which has only a reflection map.
const GLASS_TEXTURE: &str = "bp_glass";
/// Overhead cables (telephone, power, tram) have no texture: solid black.
const BLACK_TEXTURE: &str = "bp_black";
/// Overlays are lifted off their base surface to avoid z-fighting.
const DECAL_LIFT: f32 = 0.01;

/// Texture dictionary size cap (CodeWalker/GTA reject very large resources;
/// ~140 MB failed, 135 MB passed).
const GROUP_MAX_BYTES: u64 = 48 << 20;
/// Textures used by at least this many units (capped to a quarter of the
/// exported units) are "common" and grouped together.
const COMMON_MIN_UNITS: usize = 24;

pub struct Options {
    pub placement: Placement,
    /// Distance at which each unit's full-detail model is drawn (beyond it,
    /// the unit's stand-in, if it has one).
    pub lod_dist: f32,
    /// Backdrops: stand-ins and scenery (off: none), drawn out to this far.
    pub lods: bool,
    pub lod_far: f32,
    /// Prefix for asset names; GTA names must be globally unique.
    pub prefix: String,
}

#[derive(Default)]
struct Geom {
    shader: usize,
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    idx: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Opaque,
    Cutout,
    Decal,
}

/// (texture stem, shader kind)
type ShaderKey = (String, Kind);

/// One drawable + archetype: a unit's geometry for one texture group.
struct UnitModel {
    name: String,
    txd: String,
    origin: [f32; 3],
    bb_min: [f32; 3],
    bb_max: [f32; 3],
    triangles: usize,
    lod_dist: f32,
}

/// Stand-in textures are cut down to this size (px).
const LOD_TEXTURE_MAX: u32 = 256;
/// A backdrop piece counts as scenery when this many of the 6 units nearest
/// it still draw it (Burnout hides a stand-in from the units it stands in for).
const SCENERY_LISTED: usize = 5;
/// A big stand-in becomes the parent of a unit without its own when it covers
/// this share of the unit's footprint.
const STAND_IN_COVER: f32 = 0.3;
/// Stand-ins are drawn out to this far (scenery: Options::lod_far).
const STAND_IN_DIST: f32 = 8000.0;

/// A backdrop piece as Burnout places it: an entry past the end of a unit's
/// instance list (its model in GLOBALBACKDROPS.BNDL), drawn while the camera
/// is in one of the units listing it. Zone n >= 0: the stand-in for unit n.
struct BackdropInst {
    model: u64,
    zone: i16,
    transform: [f32; 16],
    listed_by: Vec<u32>,
}

/// A backdrop drawable placed in the LOD map, and the units whose drawables
/// it stands in for (none: always drawn).
struct LodModel {
    model: UnitModel,
    children: Vec<u32>,
}

/// Material id -> (base shader, optional line-layer shader, double sided).
/// Material ids are hashes of resource names, so they are global.
type MaterialCache = HashMap<u64, (ShaderKey, Option<ShaderKey>, bool, bool)>;

/// Materials whose Burnout shader has no name anywhere (then alpha testing,
/// overlays and double-sidedness can't be told).
static UNKNOWN_SHADERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Models placed in a unit that gave no geometry (not in the unit's bundle,
/// or not decodable).
static NO_GEOMETRY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// (base shader, line-layer shader, double sided, line layer on the second UV set).
fn material_keys(cache: &mut MaterialCache, ex: &mut Exporter, b: &Bundle, mat: u64, out: &Path) -> (ShaderKey, Option<ShaderKey>, bool, bool) {
    cache
        .entry(mat)
        .or_insert_with(|| {
            let info = ex.material_info(b, mat);
            if info.shader.is_empty() {
                UNKNOWN_SHADERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if std::env::var_os("BPX_DEBUG_SHADERS").is_some() {
                    let r = b.find(mat).and_then(|e| bpworld::Res::load(b, e).ok());
                    eprintln!("unknown shader: material {mat:x} shader {:x?} in this bundle {}", r.as_ref().and_then(bpworld::material_shader), r.as_ref().and_then(bpworld::material_shader).is_some_and(|id| b.find(id).is_some()));
                }
            }
            if info.diffuse.is_none() && std::env::var_os("BPX_DEBUG_WHITE").is_some() {
                eprintln!("white: material {mat:x} {} shader {}", b.name_of(mat).map(crate::export::short_name).unwrap_or_default(), info.shader);
            }
            let (base, line) = shaders_for(ex, &info, out);
            (base, line, info.double_sided(), !info.lightmapped)
        })
        .clone()
}

fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|k| p[0] * m[k] + p[1] * m[4 + k] + p[2] * m[8 + k] + m[12 + k])
}

fn transform_dir(m: &[f32; 16], d: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|k| d[0] * m[k] + d[1] * m[4 + k] + d[2] * m[8 + k])
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-8 {
        v.map(|c| c / l)
    } else {
        [0.0, 0.0, 1.0]
    }
}

pub(crate) fn v3(tag: &str, p: [f32; 3]) -> String {
    format!("<{tag} x=\"{}\" y=\"{}\" z=\"{}\" />", p[0], p[1], p[2])
}

fn v4w(tag: &str, p: [f32; 3]) -> String {
    // Rockstar's geometry boxes repeat x in w.
    format!("<{tag} x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />", p[0], p[1], p[2], p[0])
}

fn bbox<'a>(pts: impl Iterator<Item = &'a [f32; 3]>) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in pts {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// Rockstar's case-insensitive one-at-a-time hash (used for entity guids).
pub(crate) fn joaat(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.to_ascii_lowercase().bytes() {
        h = h.wrapping_add(c as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

/// Width, height, mip count and D3D format name from one of our DDS files
/// (DX10 header, written by `tex::write_dds`).
fn dds_info(path: &Path) -> Result<(u32, u32, u32, &'static str)> {
    let b = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let format = match &b[84..88] {
        b"DX10" => match u(128) {
            71 | 72 => "D3DFMT_DXT1",
            74 | 75 => "D3DFMT_DXT3",
            77 | 78 => "D3DFMT_DXT5",
            80 => "D3DFMT_ATI1",
            83 => "D3DFMT_ATI2",
            98 | 99 => "D3DFMT_BC7",
            87 => "D3DFMT_A8R8G8B8",
            _ => "D3DFMT_A8B8G8R8",
        },
        b"DXT1" => "D3DFMT_DXT1",
        b"DXT3" => "D3DFMT_DXT3",
        b"DXT5" => "D3DFMT_DXT5",
        _ => "D3DFMT_A8R8G8B8", // classic uncompressed BGRA (write_rgba_dds)
    };
    Ok((u(16), u(12), u(28).max(1), format))
}

pub fn export_visual(game: &Path, out_assets: &Path, out_meta: &Path, units: &[u32], opts: &Options) -> Result<()> {
    std::fs::create_dir_all(out_assets)?;
    std::fs::create_dir_all(out_meta)?;
    let textures = TextureIndex::load(game)?;
    // Textures are decoded once into <repo>/out/textures (shared with the
    // glTF export) and copied next to the dictionaries that use them.
    let tex_root = PathBuf::from("out");
    let mut ex = Exporter::new(game.to_path_buf(), tex_root.clone(), &textures, false);
    write_white_texture(out_assets)?;
    let mut cache = MaterialCache::new();

    // Pass 1: which textures each unit uses, and where the unit is.
    let mut usage: Vec<(u32, [f32; 2], Vec<String>)> = Vec::new();
    for &unit in units {
        if let Some(u) = unit_textures(game, out_assets, &mut ex, &mut cache, unit, opts)? {
            usage.push(u);
        }
    }
    let (group_of, groups) = group_textures(&usage, out_assets, &tex_root)?;
    for (gi, stems) in groups.iter().enumerate() {
        write_ytd(out_assets, &format!("{}txd_{gi}", opts.prefix), stems)?;
    }
    let group_mb: Vec<String> = groups
        .iter()
        .map(|g| format!("{:.0}", g.iter().map(|t| texture_bytes(out_assets, t)).sum::<u64>() as f64 / 1048576.0))
        .collect();
    println!("{} texture groups (MB: {})", groups.len(), group_mb.join(" "));
    let unknown = UNKNOWN_SHADERS.load(std::sync::atomic::Ordering::Relaxed);
    if unknown > 0 {
        println!("{unknown} materials with an unknown Burnout shader (drawn as plain opaque)");
    }
    let empty = NO_GEOMETRY.load(std::sync::atomic::Ordering::Relaxed);
    if empty > 0 {
        println!("{empty} placed models gave no geometry (left out)");
    }

    // Pass 2: geometry, one drawable per (unit, texture group).
    let mut by_unit: std::collections::BTreeMap<u32, Vec<UnitModel>> = std::collections::BTreeMap::new();
    for &unit in units {
        let unit_models = export_unit(game, out_assets, &mut ex, &mut cache, &group_of, unit, opts)?;
        if unit_models.is_empty() {
            println!("unit {unit}: no visual geometry");
            continue;
        }
        let tris: usize = unit_models.iter().map(|m| m.triangles).sum();
        println!("unit {unit}: {tris} triangles in {} drawables", unit_models.len());
        by_unit.insert(unit, unit_models);
    }

    // Backdrops: stand-ins (LOD parents of units) and scenery, in one LOD map.
    let lods = if opts.lods { export_backdrops(game, out_assets, &mut ex, &by_unit, opts)? } else { Vec::new() };
    let lod_map = format!("{}lod", opts.prefix);
    let mut parent_of: HashMap<u32, usize> = HashMap::new();
    for (i, l) in lods.iter().enumerate() {
        for &u in &l.children {
            parent_of.insert(u, i);
        }
    }
    // A stand-in's children are every drawable of the units it stands in for.
    let mut child_counts = vec![0usize; lods.len()];
    for (u, &i) in &parent_of {
        child_counts[i] += by_unit.get(u).map_or(0, Vec::len);
    }
    let mut models = Vec::new();
    for (unit, unit_models) in by_unit {
        write_ymap(out_meta, unit, &unit_models, parent_of.get(&unit).map(|&i| (lod_map.as_str(), i)), opts)?;
        models.extend(unit_models);
    }
    if !lods.is_empty() {
        write_lod_ymap(out_meta, &lod_map, &lods, &child_counts, opts)?;
    }
    let lod_count = lods.len();
    models.extend(lods.into_iter().map(|l| l.model));
    write_ytyp(out_meta, &models, opts)?;
    println!("wrote {} drawables ({lod_count} backdrops), {}paradise.ytyp, {} ymaps", models.len(), opts.prefix, usage.len());
    Ok(())
}

/// Every backdrop piece Burnout places, once each (the same piece is listed
/// by every unit it's drawn from, its rotation sometimes a rounding error
/// apart), with the units listing it.
fn collect_backdrops(game: &Path, bdb: &Bundle) -> Result<Vec<BackdropInst>> {
    let mut units: Vec<u32> = std::fs::read_dir(game)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok()?.strip_prefix("TRK_UNIT")?.strip_suffix("_GR.BNDL")?.parse().ok())
        .collect();
    units.sort();
    let mut list: Vec<BackdropInst> = Vec::new();
    let mut index: HashMap<(u64, [i32; 3]), usize> = HashMap::new();
    for unit in units {
        let b = Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
        let Some(e) = b.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else { continue };
        let r = Res::load(&b, e)?;
        let m = &r.main;
        let (arr, size, count) = (bnd2::u32_at(m, 0) as usize, bnd2::u32_at(m, 4) as usize, bnd2::u32_at(m, 8) as usize);
        for i in count..size {
            let o = arr + i * 0x50;
            if o + 0x50 > m.len() {
                break;
            }
            let Some(model) = r.import(o) else { continue };
            if bdb.find(model).is_none() {
                continue;
            }
            let mut transform = [0f32; 16];
            for (k, t) in transform.iter_mut().enumerate() {
                *t = bnd2::f32_at(m, o + 0x10 + k * 4);
            }
            let zone = bnd2::u16_at(m, o + 4) as i16;
            let at = [12, 13, 14].map(|k| (transform[k] * 10.0).round() as i32);
            let i = *index.entry((model, at)).or_insert_with(|| {
                list.push(BackdropInst { model, zone, transform, listed_by: Vec::new() });
                list.len() - 1
            });
            list[i].listed_by.push(unit);
        }
    }
    Ok(list)
}

/// PVS zone bounds (Burnout x, z: min, max) by zone id, which is the unit number.
fn zone_bounds(game: &Path) -> HashMap<u32, ([f32; 2], [f32; 2])> {
    let mut out = HashMap::new();
    let Ok(b) = Bundle::open(game.join("PVS.BNDL")) else { return out };
    let Some(r) = b.entries.first().and_then(|e| Res::load(&b, e).ok()) else { return out };
    let m = &r.main;
    let (zones, n) = (bnd2::u32_at(m, 4) as usize, bnd2::u32_at(m, 0x10) as usize);
    for i in 0..n {
        let z = zones + i * 0x30;
        if z + 0x30 > m.len() {
            break;
        }
        let (pts, np) = (bnd2::u32_at(m, z) as usize, bnd2::u16_at(m, z + 0x1A) as usize);
        if np == 0 || pts + np * 16 > m.len() {
            continue;
        }
        // Points are padded Vector2s (x, z).
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for k in 0..np {
            for c in 0..2 {
                let v = bnd2::f32_at(m, pts + k * 16 + c * 4);
                lo[c] = lo[c].min(v);
                hi[c] = hi[c].max(v);
            }
        }
        out.insert(bnd2::u64_at(m, z + 0x10) as u32, (lo, hi));
    }
    out
}

/// A backdrop piece's top-LOD geometry (in its own space).
fn piece_geometry(bdb: &Bundle, model: u64) -> Vec<bw::MeshGeometry> {
    (|| -> Option<Vec<bw::MeshGeometry>> {
        let m = bw::parse_model(&Res::load(bdb, bdb.find(model)?).ok()?);
        bw::decode_renderable(bdb, bdb.find(m.lod0()?)?).ok()
    })()
    .unwrap_or_default()
}

/// Exports the backdrops and decides what each is:
/// - the stand-in for a unit (its LOD parent): `BdZone<n>`;
/// - a big stand-in (Big Surf Island) for the units it covers that have none;
/// - scenery, always drawn: what Burnout still draws from the units next to
///   it (the mountains), and the other pieces with no full-detail model
///   anywhere (the Paradise City sign: Burnout hides it close up, but only
///   from where it can't be seen);
/// - left out: water planes, lights, the whole-world proxy, the patches
///   Burnout puts over holes in its distant view.
fn export_backdrops(game: &Path, out: &Path, ex: &mut Exporter, by_unit: &std::collections::BTreeMap<u32, Vec<UnitModel>>, opts: &Options) -> Result<Vec<LodModel>> {
    let Ok(bdb) = Bundle::open(game.join("GLOBALBACKDROPS.BNDL")) else {
        println!("backdrops: no GLOBALBACKDROPS.BNDL");
        return Ok(Vec::new());
    };
    let list = collect_backdrops(game, &bdb)?;
    let zones = zone_bounds(game);
    let debug = std::env::var_os("BPX_DEBUG_BACKDROPS").is_some();
    let name_of = |inst: &BackdropInst| bdb.name_of(inst.model).map(crate::export::short_name).unwrap_or_else(|| format!("{:08x}", inst.model as u32));

    // The drawables to make: their pieces, and the units they stand in for.
    let mut parts: Vec<(&BackdropInst, Vec<u32>)> = Vec::new();
    let mut part_of_unit: HashMap<u32, usize> = HashMap::new();
    let mut others: Vec<&BackdropInst> = Vec::new();
    let (mut scenery, mut skipped) = (0, 0);
    for inst in &list {
        let raw_name = name_of(inst);
        let lower = raw_name.to_ascii_lowercase();
        let role = if ["water", "lake", "light", "cheap", "hole", "terrain_fix", "pvsfix"].iter().any(|w| lower.contains(w)) {
            skipped += 1;
            "left out"
        } else if inst.zone >= 0 {
            let unit = inst.zone as u32;
            if by_unit.contains_key(&unit) && !part_of_unit.contains_key(&unit) {
                part_of_unit.insert(unit, parts.len());
                parts.push((inst, vec![unit]));
                "stand-in"
            } else {
                skipped += 1;
                "left out (no unit)"
            }
        } else {
            // Scenery: still drawn from the units nearest it.
            let at = [inst.transform[12], inst.transform[14]];
            let mut near: Vec<(f32, u32)> = by_unit
                .keys()
                .filter_map(|u| zones.get(u).map(|(lo, hi)| (((lo[0] + hi[0]) * 0.5 - at[0]).hypot((lo[1] + hi[1]) * 0.5 - at[1]), *u)))
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            if near.iter().take(6).filter(|(_, u)| inst.listed_by.contains(u)).count() >= SCENERY_LISTED {
                scenery += 1;
                parts.push((inst, Vec::new()));
                "scenery"
            } else {
                others.push(inst);
                "other"
            }
        };
        if debug {
            eprintln!("backdrop {raw_name} zone {} at {:?} {role}, listed by {:?}", inst.zone, &inst.transform[12..15], inst.listed_by);
        }
    }

    // The rest (GTA x/y bounds), biggest first: stand-ins for the units they
    // cover that have none, else scenery.
    let mut big: Vec<(&BackdropInst, [f32; 2], [f32; 2])> = Vec::new();
    for inst in others {
        let pos: Vec<[f32; 3]> = piece_geometry(&bdb, inst.model).iter().flat_map(|g| g.positions.iter().map(|&p| opts.placement.apply(transform_point(&inst.transform, p)))).collect();
        let (lo, hi) = bbox(pos.iter());
        // Empty (BD_airport01 is two triangles with no area).
        if pos.is_empty() || (hi[0] - lo[0]).max(hi[1] - lo[1]) < 1.0 {
            skipped += 1;
            continue;
        }
        big.push((inst, [lo[0], lo[1]], [hi[0], hi[1]]));
    }
    // Footprint (GTA x/y) of each unit's drawables.
    let footprint: HashMap<u32, ([f32; 2], [f32; 2])> = by_unit
        .iter()
        .map(|(&u, ms)| {
            let lo = ms.iter().fold([f32::MAX; 2], |a, m| [a[0].min(m.origin[0] + m.bb_min[0]), a[1].min(m.origin[1] + m.bb_min[1])]);
            let hi = ms.iter().fold([f32::MIN; 2], |a, m| [a[0].max(m.origin[0] + m.bb_max[0]), a[1].max(m.origin[1] + m.bb_max[1])]);
            (u, (lo, hi))
        })
        .collect();
    big.sort_by(|a, b| ((b.2[0] - b.1[0]) * (b.2[1] - b.1[1])).total_cmp(&((a.2[0] - a.1[0]) * (a.2[1] - a.1[1]))));
    let mut covering = 0;
    for (inst, lo, hi) in big {
        let mut children: Vec<u32> = footprint
            .iter()
            .filter(|(u, (ulo, uhi))| {
                let w = (hi[0].min(uhi[0]) - lo[0].max(ulo[0])).max(0.0);
                let h = (hi[1].min(uhi[1]) - lo[1].max(ulo[1])).max(0.0);
                !part_of_unit.contains_key(*u) && w * h / ((uhi[0] - ulo[0]) * (uhi[1] - ulo[1])).max(1.0) >= STAND_IN_COVER
            })
            .map(|(u, _)| *u)
            .collect();
        if debug {
            eprintln!("backdrop {} covers units {children:?}", name_of(inst));
        }
        if children.is_empty() {
            scenery += 1;
        } else {
            children.sort();
            for &u in &children {
                part_of_unit.insert(u, parts.len());
            }
            covering += 1;
        }
        parts.push((inst, children));
    }

    // One drawable per part.
    let lod_txd = format!("{}txd_lod", opts.prefix);
    let mut materials: HashMap<u64, Option<(ShaderKey, bool)>> = HashMap::new();
    let mut stems: Vec<String> = Vec::new();
    let mut lods: Vec<LodModel> = Vec::new();
    let mut cut = 0;
    for (inst, children) in parts {
        // Lower case: the game finds a drawable by its lower-cased file name,
        // while CodeWalker hashes archetype names as written.
        let name = format!("{}lod_{}_{:08x}", opts.prefix, name_of(inst).chars().take(32).collect::<String>(), joaat(&format!("{:?}", &inst.transform[12..15]))).to_ascii_lowercase();
        // A stand-in covering several units is cut into one piece per unit,
        // each triangle going to the unit whose footprint is nearest: GTA
        // only hides a LOD once every child is in range, and Big Surf Island
        // is wider than that range, so one parent for all of it stayed drawn
        // over the island's full detail.
        let pieces: Vec<(String, Vec<u32>)> = if children.len() > 1 { children.iter().map(|&u| (format!("{name}_{u}"), vec![u])).collect() } else { vec![(name, children.clone())] };
        let nearest = |c: [f32; 3]| -> Option<u32> {
            let gap = |u: &u32| {
                let (lo, hi) = footprint[u];
                let d = [(lo[0] - c[0]).max(c[0] - hi[0]).max(0.0), (lo[1] - c[1]).max(c[1] - hi[1]).max(0.0)];
                let mid = ((lo[0] + hi[0]) * 0.5 - c[0]).hypot((lo[1] + hi[1]) * 0.5 - c[1]);
                (d[0].hypot(d[1]), mid)
            };
            children.iter().min_by(|a, b| gap(a).partial_cmp(&gap(b)).unwrap_or(std::cmp::Ordering::Equal)).copied()
        };
        for (piece, units) in pieces {
            let keep = |c: [f32; 3]| units.first().is_some_and(|&u| nearest(c) == Some(u));
            let Some((mut model, used)) = export_lod_model(&bdb, inst, &piece, &lod_txd, out, ex, &mut materials, opts, (children.len() > 1).then_some(&keep as &dyn Fn([f32; 3]) -> bool))? else {
                if children.len() > 1 {
                    println!("backdrops: {piece}: no triangles nearest unit {units:?} (left without a stand-in)");
                }
                skipped += 1;
                continue;
            };
            if children.len() > 1 {
                cut += 1;
            }
            model.lod_dist = if units.is_empty() { opts.lod_far } else { STAND_IN_DIST };
            stems.extend(used);
            lods.push(LodModel { model, children: units });
        }
    }
    stems.sort();
    stems.dedup();
    write_ytd(out, &lod_txd, &stems)?;
    let mb = stems.iter().map(|t| texture_bytes(out, t)).sum::<u64>() as f64 / 1048576.0;
    let units = lods.iter().map(|l| l.children.len()).sum::<usize>();
    println!(
        "backdrops: {} placed; {} stand-ins for {units} units ({covering} covering several, cut into {cut} pieces), {scenery} scenery, {skipped} left out; {} textures ({mb:.1} MB)",
        list.len(),
        lods.len() - scenery,
        stems.len()
    );
    Ok(lods)
}

/// A backdrop piece as a drawable with cut-down textures, and the textures
/// it uses; overlays, glass and water are left out (they don't read from afar).
/// With `keep`, only the triangles whose centre (GTA coordinates) it accepts.
#[allow(clippy::too_many_arguments)]
fn export_lod_model(
    bdb: &Bundle,
    inst: &BackdropInst,
    name: &str,
    txd: &str,
    out: &Path,
    ex: &mut Exporter,
    materials: &mut HashMap<u64, Option<(ShaderKey, bool)>>,
    opts: &Options,
    keep: Option<&dyn Fn([f32; 3]) -> bool>,
) -> Result<Option<(UnitModel, Vec<String>)>> {
    let mut shaders: Vec<ShaderKey> = Vec::new();
    let mut shader_of: HashMap<ShaderKey, usize> = HashMap::new();
    let mut open: HashMap<usize, Geom> = HashMap::new();
    let mut done: Vec<Geom> = Vec::new();
    for g in &piece_geometry(bdb, inst.model) {
        let Some(mat) = g.material else { continue };
        if g.indices.is_empty() {
            continue;
        }
        let pos: Vec<[f32; 3]> = g.positions.iter().map(|&p| opts.placement.apply(transform_point(&inst.transform, p))).collect();
        // The triangles kept, and the vertices they use (in first-use order;
        // all of them, in order, without `keep`).
        let mut tris: Vec<u32> = Vec::new();
        let mut remap: HashMap<u32, u32> = HashMap::new();
        let mut used: Vec<usize> = Vec::new();
        if let Some(keep) = keep {
            for t in g.indices.chunks_exact(3) {
                let c = [0, 1, 2].map(|k| t.iter().map(|&i| pos[i as usize][k]).sum::<f32>() / 3.0);
                if keep(c) {
                    for &i in t {
                        let at = *remap.entry(i).or_insert_with(|| {
                            used.push(i as usize);
                            used.len() as u32 - 1
                        });
                        tris.push(at);
                    }
                }
            }
            if tris.is_empty() {
                continue;
            }
        } else {
            tris = g.indices.clone();
            used = (0..pos.len()).collect();
        }
        let key = materials
            .entry(mat)
            .or_insert_with(|| {
                let info = ex.material_info(bdb, mat);
                if info.greyscale() || info.transparent() || info.shader.starts_with("Water") {
                    return None;
                }
                let (stem, _) = ex.texture_lod(info.diffuse?, LOD_TEXTURE_MAX, out)?;
                Some(((stem, if info.alpha_tested() { Kind::Cutout } else { Kind::Opaque }), info.double_sided()))
            })
            .clone();
        let Some((key, double)) = key else { continue };
        let si = *shader_of.entry(key.clone()).or_insert_with(|| {
            shaders.push(key.clone());
            shaders.len() - 1
        });
        let nrm: Vec<[f32; 3]> = used
            .iter()
            .map(|&i| normalize(opts.placement.apply_dir(normalize(transform_dir(&inst.transform, g.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]))))))
            .collect();
        let sides: &[bool] = if double { &[false, true] } else { &[false] };
        for &back in sides {
            let geom = open.entry(si).or_insert_with(|| Geom { shader: si, ..Default::default() });
            if geom.pos.len() + used.len() > MAX_GEOM_VERTS {
                done.push(std::mem::replace(geom, Geom { shader: si, ..Default::default() }));
            }
            let base = geom.pos.len() as u32;
            for (j, &i) in used.iter().enumerate() {
                geom.pos.push(pos[i]);
                geom.nrm.push(if back { nrm[j].map(|c| -c) } else { nrm[j] });
                geom.uv.push(g.uvs.get(i).copied().unwrap_or([0.0, 0.0]));
            }
            for t in tris.chunks(3) {
                if back {
                    geom.idx.extend([base + t[0], base + t[2], base + t[1]]);
                } else {
                    geom.idx.extend(t.iter().map(|&i| base + i));
                }
            }
        }
    }
    done.extend(open.into_values());
    done.retain(|g| !g.idx.is_empty());
    if done.is_empty() {
        return Ok(None);
    }
    done.sort_by_key(|g| g.shader);
    let (lo, hi) = bbox(done.iter().flat_map(|g| g.pos.iter()));
    let origin = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
    for g in &mut done {
        for p in &mut g.pos {
            *p = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
        }
    }
    let (glo, ghi) = bbox(done.iter().flat_map(|g| g.pos.iter()));
    let triangles = done.iter().map(|g| g.idx.len() / 3).sum();
    write_ydr(out, name, &shaders, &done)?;
    let stems = shaders.iter().map(|k| k.0.clone()).collect();
    Ok(Some((UnitModel { name: name.to_string(), txd: txd.to_string(), origin, bb_min: glo, bb_max: ghi, triangles, lod_dist: STAND_IN_DIST }, stems)))
}

/// The LOD map: every backdrop entity; a stand-in's children are its units'
/// drawables (in their own map files, which name this one as parent).
fn write_lod_ymap(out: &Path, ymap: &str, lods: &[LodModel], child_counts: &[usize], opts: &Options) -> Result<()> {
    let mut wmin = [f32::MAX; 3];
    let mut wmax = [f32::MIN; 3];
    let mut far = 0f32;
    let mut entities = String::new();
    for (i, l) in lods.iter().enumerate() {
        let m = &l.model;
        for k in 0..3 {
            wmin[k] = wmin[k].min(m.origin[k] + m.bb_min[k]);
            wmax[k] = wmax[k].max(m.origin[k] + m.bb_max[k]);
        }
        far = far.max(m.lod_dist);
        let num = child_counts[i];
        let child_dist = if num > 0 { opts.lod_dist } else { 0.0 };
        let _ = write!(
            entities,
            "  <Item type=\"CEntityDef\">\n   <archetypeName>{}</archetypeName>\n   <flags value=\"1572864\" />\n   <guid value=\"{}\" />\n   {}\n   <rotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n   <scaleXY value=\"1\" />\n   <scaleZ value=\"1\" />\n   <parentIndex value=\"-1\" />\n   <lodDist value=\"{}\" />\n   <childLodDist value=\"{child_dist}\" />\n   <lodLevel>LODTYPES_DEPTH_LOD</lodLevel>\n   <numChildren value=\"{num}\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
            m.name,
            joaat(&m.name),
            v3("position", m.origin),
            m.lod_dist
        );
    }
    let smin = [wmin[0] - far, wmin[1] - far, wmin[2] - far];
    let smax = [wmax[0] + far, wmax[1] + far, wmax[2] + far];
    let head = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>{ymap}</name>\n <parent />\n <flags value=\"2\" />\n <contentFlags value=\"2\" />\n {}\n {}\n {}\n {}\n <entities>\n",
        v3("streamingExtentsMin", smin),
        v3("streamingExtentsMax", smax),
        v3("entitiesExtentsMin", wmin),
        v3("entitiesExtentsMax", wmax)
    );
    let tail = YMAP_TAIL.replace("{ymap}", ymap);
    std::fs::write(out.join(format!("{ymap}.ymap.xml")), head + &entities + &tail)?;
    Ok(())
}

fn texture_bytes(out: &Path, stem: &str) -> u64 {
    std::fs::metadata(out.join(format!("{stem}.dds"))).map(|m| m.len()).unwrap_or(0)
}

/// Pass 1 for one unit: the texture stems its materials use and its centre
/// (GTA x/y). Reads material references only, no vertex data.
fn unit_textures(
    game: &Path,
    out: &Path,
    ex: &mut Exporter,
    cache: &mut MaterialCache,
    unit: u32,
    opts: &Options,
) -> Result<Option<(u32, [f32; 2], Vec<String>)>> {
    let b = Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
    let Some(list) = b.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else {
        return Ok(None);
    };
    let instances = bw::parse_instance_list(&Res::load(&b, list)?);
    if instances.is_empty() {
        return Ok(None);
    }
    let mut centre = [0f32; 2];
    for inst in &instances {
        let p = opts.placement.apply([inst.transform[12], inst.transform[13], inst.transform[14]]);
        centre[0] += p[0] / instances.len() as f32;
        centre[1] += p[1] / instances.len() as f32;
    }
    let mut stems: Vec<String> = Vec::new();
    let mut seen_models = std::collections::HashSet::new();
    for inst in &instances {
        if !seen_models.insert(inst.model) {
            continue;
        }
        let mats: Vec<u64> = (|| -> Option<Vec<u64>> {
            let m = bw::parse_model(&Res::load(&b, b.find(inst.model)?).ok()?);
            let r = bw::parse_renderable(&Res::load(&b, b.find(m.lod0()?)?).ok()?).ok()?;
            Some(r.meshes.iter().filter_map(|mesh| mesh.material).collect())
        })()
        .unwrap_or_default();
        for mat in mats {
            let (base, line, _, _) = material_keys(cache, ex, &b, mat, out);
            stems.push(base.0);
            if let Some(l) = line {
                stems.push(l.0);
            }
        }
    }
    stems.sort();
    stems.dedup();
    Ok(Some((unit, centre, stems)))
}

/// Packs textures into dictionaries of at most GROUP_MAX_BYTES: common
/// textures first (largest first), then the rest in Z-order of where they
/// are used, so each group covers a compact area of the map.
fn group_textures(
    usage: &[(u32, [f32; 2], Vec<String>)],
    out: &Path,
    tex_root: &Path,
) -> Result<(HashMap<String, usize>, Vec<Vec<String>>)> {
    let mut users: HashMap<&str, Vec<[f32; 2]>> = HashMap::new();
    for (_, c, stems) in usage {
        for t in stems {
            users.entry(t.as_str()).or_default().push(*c);
        }
    }
    // Make sure every texture sits next to the dictionary XML for CodeWalker.
    for t in users.keys() {
        let dest = out.join(format!("{t}.dds"));
        if !dest.exists() {
            std::fs::copy(tex_root.join("textures").join(format!("{t}.dds")), &dest)
                .with_context(|| format!("copying texture {t}"))?;
        }
    }
    let common_min = COMMON_MIN_UNITS.min((usage.len() / 4).max(2));
    let (lo, hi) = usage.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), (_, c, _)| {
        ([lo[0].min(c[0]), lo[1].min(c[1])], [hi[0].max(c[0]), hi[1].max(c[1])])
    });
    let morton = |c: [f32; 2]| -> u64 {
        let q = |k: usize| (((c[k] - lo[k]) / (hi[k] - lo[k]).max(1.0)) * 65535.0).clamp(0.0, 65535.0) as u64;
        let (x, y) = (q(0), q(1));
        (0..16).fold(0u64, |m, bit| m | ((x >> bit) & 1) << (2 * bit) | ((y >> bit) & 1) << (2 * bit + 1))
    };
    let mut common: Vec<(&str, u64)> = Vec::new();
    let mut regional: Vec<(&str, u64, u64)> = Vec::new();
    for (t, cs) in &users {
        let size = texture_bytes(out, t);
        if cs.len() >= common_min {
            common.push((t, size));
        } else {
            let c = [cs.iter().map(|c| c[0]).sum::<f32>() / cs.len() as f32, cs.iter().map(|c| c[1]).sum::<f32>() / cs.len() as f32];
            regional.push((t, size, morton(c)));
        }
    }
    common.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    regional.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(b.0)));

    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut pack = |list: &mut dyn Iterator<Item = (&str, u64)>| {
        let mut cur: Vec<String> = Vec::new();
        let mut cur_size = 0u64;
        for (t, size) in list {
            if !cur.is_empty() && cur_size + size > GROUP_MAX_BYTES {
                groups.push(std::mem::take(&mut cur));
                cur_size = 0;
            }
            cur.push(t.to_string());
            cur_size += size;
        }
        if !cur.is_empty() {
            groups.push(cur);
        }
    };
    pack(&mut common.iter().map(|c| (c.0, c.1)));
    pack(&mut regional.iter().map(|r| (r.0, r.1)));
    let group_of = groups.iter().enumerate().flat_map(|(gi, g)| g.iter().map(move |t| (t.clone(), gi))).collect();
    Ok((group_of, groups))
}

/// Writes an uncompressed 32-bit texture with a classic (non-DX10) DDS header
/// (A8R8G8B8, single mip), which CodeWalker's importer reads reliably.
pub(crate) fn write_rgba_dds(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<()> {
    let mut b = Vec::with_capacity(128 + rgba.len());
    let put = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
    b.extend_from_slice(b"DDS ");
    put(&mut b, 124);
    put(&mut b, 0x1 | 0x2 | 0x4 | 0x8 | 0x1000); // CAPS HEIGHT WIDTH PITCH PIXELFORMAT
    put(&mut b, h);
    put(&mut b, w);
    put(&mut b, w * 4);
    put(&mut b, 0);
    put(&mut b, 1);
    for _ in 0..11 {
        put(&mut b, 0);
    }
    // DDS_PIXELFORMAT: RGB | ALPHAPIXELS, 32 bpp, BGRA masks.
    for v in [32, 0x41, 0, 32, 0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
        put(&mut b, v);
    }
    put(&mut b, 0x1000);
    for _ in 0..4 {
        put(&mut b, 0);
    }
    for px in rgba.chunks(4) {
        b.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    std::fs::write(path, b)?;
    Ok(())
}

fn write_white_texture(dir: &Path) -> Result<()> {
    // One BC1 block of opaque white, 4x4, no mips.
    let t = bw::Texture { dxgi_format: 71, width: 4, height: 4, depth: 1, mips: 1, data: vec![0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0] };
    tex::write_dds(&t, &dir.join(format!("{WHITE_TEXTURE}.dds")))?;
    // One BC1 block of opaque black.
    let t = bw::Texture { dxgi_format: 71, width: 4, height: 4, depth: 1, mips: 1, data: vec![0, 0, 0, 0, 0, 0, 0, 0] };
    tex::write_dds(&t, &dir.join(format!("{BLACK_TEXTURE}.dds")))?;
    // Pale blue-grey, ~35% opaque, uncompressed RGBA 4x4.
    write_rgba_dds(&dir.join(format!("{GLASS_TEXTURE}.dds")), 4, 4, &[150u8, 170, 185, 90].repeat(16))
}

/// Converts a multiplicative greyscale overlay into an alpha-blended one:
/// black, with alpha = 1 - brightness (blending black at that opacity
/// darkens the surface below by the same factor a multiply would).
fn write_darkening_texture(ex: &Exporter, id: u64, out: &Path, stem: &str) -> Option<String> {
    let name = format!("{stem}_mul");
    let dest = out.join(format!("{name}.dds"));
    if !dest.exists() {
        let (mut w, mut h, mut rgba) = ex.texture_rgba(id)?;
        // Stored uncompressed, so keep them small: soft grime masks lose
        // little at 1024 px (a 4096 px one would be 64 MB).
        while w > 1024 || h > 1024 {
            (w, h, rgba) = halve(w, h, &rgba);
        }
        let data: Vec<u8> = rgba
            .chunks(4)
            .flat_map(|p| {
                let lum = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
                let a = ((255.0 - lum) * p[3] as f32 / 255.0).clamp(0.0, 255.0) as u8;
                [0, 0, 0, a]
            })
            .collect();
        write_rgba_dds(&dest, w, h, &data).ok()?;
    }
    Some(name)
}

/// 2x2 box-filter downscale of an RGBA8 image.
fn halve(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let mut sum = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(w - 1);
                    let sy = (y * 2 + dy).min(h - 1);
                    sum += rgba[((sy * w + sx) * 4 + c) as usize] as u32;
                }
                out.push((sum / 4) as u8);
            }
        }
    }
    (nw, nh, out)
}

/// Base-layer and optional line-layer shader for a material.
fn shaders_for(ex: &mut Exporter, info: &MaterialInfo, out: &Path) -> (ShaderKey, Option<ShaderKey>) {
    let diffuse = info.diffuse.and_then(|id| ex.texture(id).map(|t| (id, t)));
    let base = if info.shader.starts_with("Cable") && diffuse.is_none() {
        (BLACK_TEXTURE.into(), Kind::Opaque)
    } else if info.greyscale() {
        match diffuse.and_then(|(id, (stem, _))| write_darkening_texture(ex, id, out, &stem)) {
            Some(name) => (name, Kind::Decal),
            None => (WHITE_TEXTURE.into(), Kind::Decal),
        }
    } else if info.transparent() {
        match diffuse {
            Some((_, (stem, true))) => (stem, Kind::Decal),
            _ => (GLASS_TEXTURE.into(), Kind::Decal),
        }
    } else {
        match diffuse {
            // Burnout's shader name is authoritative for alpha testing
            // ("1Bit"); BC1 textures of opaque materials can still contain
            // stray punch-through texels, which would punch holes.
            Some((_, (stem, _))) => (stem, if info.alpha_tested() { Kind::Cutout } else { Kind::Opaque }),
            None => (WHITE_TEXTURE.into(), Kind::Opaque),
        }
    };
    let line = info.line.and_then(|id| ex.texture(id)).map(|(stem, _)| (stem, Kind::Decal));
    (base, line)
}

fn export_unit(
    game: &Path,
    out: &Path,
    ex: &mut Exporter,
    cache: &mut MaterialCache,
    group_of: &HashMap<String, usize>,
    unit: u32,
    opts: &Options,
) -> Result<Vec<UnitModel>> {
    let b = Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
    let Some(list) = b.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else {
        return Ok(Vec::new());
    };
    let instances = bw::parse_instance_list(&Res::load(&b, list)?);

    let mut shaders: Vec<ShaderKey> = Vec::new();
    let mut shader_of: HashMap<ShaderKey, usize> = HashMap::new();
    let mut geom_cache: HashMap<u64, Vec<bw::MeshGeometry>> = HashMap::new();
    // Open geometry per shader (full ones are moved to `done`).
    let mut open: HashMap<usize, Geom> = HashMap::new();
    let mut done: Vec<Geom> = Vec::new();

    for inst in &instances {
        if !geom_cache.contains_key(&inst.model) {
            let geo = (|| -> Option<Vec<bw::MeshGeometry>> {
                let m = bw::parse_model(&Res::load(&b, b.find(inst.model)?).ok()?);
                bw::decode_renderable(&b, b.find(m.lod0()?)?).ok()
            })()
            .unwrap_or_default();
            if geo.is_empty() {
                NO_GEOMETRY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if std::env::var_os("BPX_DEBUG_SHADERS").is_some() {
                    eprintln!("no geometry: unit model {:x} in bundle {}", inst.model, b.find(inst.model).is_some());
                }
            }
            geom_cache.insert(inst.model, geo);
        }
        for g in &geom_cache[&inst.model] {
            if g.indices.is_empty() {
                continue;
            }
            let (base, line, double, line_uv1) = match g.material {
                Some(mat) => material_keys(cache, ex, &b, mat, out),
                None => ((WHITE_TEXTURE.into(), Kind::Opaque), None, false, false),
            };
            if base.0 == WHITE_TEXTURE && std::env::var_os("BPX_DEBUG_WHITE").is_some() {
                eprintln!("white base: unit {} model {:x} {} material {:x?} {} ({} triangles, {} uvs, {} normals)", unit, inst.model, b.name_of(inst.model).map(crate::export::short_name).unwrap_or_default(), g.material, g.material.and_then(|m| b.name_of(m)).map(crate::export::short_name).unwrap_or_default(), g.indices.len() / 3, g.uvs.len(), g.normals.len());
            }
            if g.material.is_none() && std::env::var_os("BPX_DEBUG_WHITE").is_some() {
                eprintln!("no material: unit {} model {:x} {} ({} triangles)", unit, inst.model, b.name_of(inst.model).map(crate::export::short_name).unwrap_or_default(), g.indices.len() / 3);
            }
            // World-space vertices and normals for this instance.
            let pos: Vec<[f32; 3]> = g.positions.iter().map(|&p| opts.placement.apply(transform_point(&inst.transform, p))).collect();
            let nrm: Vec<[f32; 3]> = (0..g.positions.len())
                .map(|i| {
                    let n = g.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                    normalize(opts.placement.apply_dir(normalize(transform_dir(&inst.transform, n))))
                })
                .collect();
            let uv0: Vec<[f32; 2]> = (0..g.positions.len()).map(|i| g.uvs.get(i).copied().unwrap_or([0.0, 0.0])).collect();

            let mut emit = |key: &ShaderKey, uv: &[[f32; 2]], lift: f32| {
                let si = *shader_of.entry(key.clone()).or_insert_with(|| {
                    shaders.push(key.clone());
                    shaders.len() - 1
                });
                let sides: &[bool] = if double { &[false, true] } else { &[false] };
                for &back in sides {
                    let geom = open.entry(si).or_insert_with(|| Geom { shader: si, ..Default::default() });
                    if geom.pos.len() + pos.len() > MAX_GEOM_VERTS {
                        let full = std::mem::replace(geom, Geom { shader: si, ..Default::default() });
                        done.push(full);
                    }
                    let base_i = geom.pos.len() as u32;
                    for i in 0..pos.len() {
                        let n = if back { nrm[i].map(|c| -c) } else { nrm[i] };
                        geom.pos.push([pos[i][0] + n[0] * lift, pos[i][1] + n[1] * lift, pos[i][2] + n[2] * lift]);
                        geom.nrm.push(n);
                        geom.uv.push(uv[i]);
                    }
                    for t in g.indices.chunks(3) {
                        if back {
                            geom.idx.extend([base_i + t[0], base_i + t[2], base_i + t[1]]);
                        } else {
                            geom.idx.extend(t.iter().map(|&i| base_i + i));
                        }
                    }
                }
            };
            let base_lift = if base.1 == Kind::Decal { DECAL_LIFT } else { 0.0 };
            emit(&base, &uv0, base_lift);
            if let Some(line) = &line {
                // Road meshes usually carry a single UV set shared by the
                // asphalt and its line layer; use a second set if present,
                // unless it's the light map's (lightmapped road shaders: on
                // it the line texture came out as one solid colour).
                let uv = if line_uv1 && g.uvs1.len() == g.positions.len() { &g.uvs1 } else { &uv0 };
                emit(line, uv, DECAL_LIFT);
            }
        }
    }
    done.extend(open.into_values());
    done.retain(|g| !g.idx.is_empty());
    if done.is_empty() {
        return Ok(Vec::new());
    }

    // Entity origin at the unit's centre; geometry is stored relative to it.
    let (lo, hi) = bbox(done.iter().flat_map(|g| g.pos.iter()));
    let origin = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
    for g in &mut done {
        for p in &mut g.pos {
            *p = [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]];
        }
    }

    // One drawable per texture group, with its own local shader list.
    let mut by_group: std::collections::BTreeMap<usize, Vec<Geom>> = std::collections::BTreeMap::new();
    for g in done {
        let gi = *group_of.get(&shaders[g.shader].0).context("texture missing from groups")?;
        by_group.entry(gi).or_default().push(g);
    }
    let mut models = Vec::new();
    for (gi, mut geoms) in by_group {
        let mut local: Vec<ShaderKey> = Vec::new();
        let mut remap: HashMap<usize, usize> = HashMap::new();
        geoms.sort_by_key(|g| g.shader);
        for g in &mut geoms {
            let li = *remap.entry(g.shader).or_insert_with(|| {
                local.push(shaders[g.shader].clone());
                local.len() - 1
            });
            g.shader = li;
        }
        let name = format!("{}vis_{unit}_{gi}", opts.prefix);
        let (glo, ghi) = bbox(geoms.iter().flat_map(|g| g.pos.iter()));
        let triangles = geoms.iter().map(|g| g.idx.len() / 3).sum();
        write_ydr(out, &name, &local, &geoms)?;
        models.push(UnitModel { name, txd: format!("{}txd_{gi}", opts.prefix), origin, bb_min: glo, bb_max: ghi, triangles, lod_dist: opts.lod_dist });
    }
    Ok(models)
}

fn write_ydr(out: &Path, name: &str, shaders: &[ShaderKey], geoms: &[Geom]) -> Result<()> {
    let (lo, hi) = bbox(geoms.iter().flat_map(|g| g.pos.iter()));
    let centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
    let radius = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f32>().sqrt() * 0.5;
    let mut s = String::with_capacity(geoms.iter().map(|g| g.pos.len() * 90 + g.idx.len() * 7).sum::<usize>() + 4096);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Drawable>\n");
    let _ = writeln!(s, " <Name>{name}</Name>");
    let _ = writeln!(s, " {}", v3("BoundingSphereCenter", centre));
    let _ = writeln!(s, " <BoundingSphereRadius value=\"{radius}\" />");
    let _ = writeln!(s, " {}", v3("BoundingBoxMin", lo));
    let _ = writeln!(s, " {}", v3("BoundingBoxMax", hi));
    for lod in ["High", "Med", "Low", "Vlow"] {
        let _ = writeln!(s, " <LodDist{lod} value=\"9998\" />");
    }
    s.push_str(" <FlagsHigh value=\"1\" />\n <FlagsMed value=\"0\" />\n <FlagsLow value=\"0\" />\n <FlagsVlow value=\"0\" />\n");
    s.push_str(" <ShaderGroup>\n  <Shaders>\n");
    for (stem, kind) in shaders {
        if *kind == Kind::Decal {
            let _ = write!(s, "   <Item>\n    <Name>hash_F08729D4</Name>\n    <FileName>decal.sps</FileName>\n    <RenderBucket value=\"2\" />\n    <Parameters>\n     <Item name=\"DiffuseSampler\" type=\"Texture\">\n      <Name>{stem}</Name>\n     </Item>\n     <Item name=\"useTessellation\" type=\"Vector\" x=\"0\" y=\"0\" z=\"0\" w=\"0\" />\n     <Item name=\"wetnessMultiplier\" type=\"Vector\" x=\"1\" y=\"0\" z=\"0\" w=\"0\" />\n     <Item name=\"specularIntensityMult\" type=\"Vector\" x=\"0.125\" y=\"0\" z=\"0\" w=\"0\" />\n     <Item name=\"specularFalloffMult\" type=\"Vector\" x=\"100\" y=\"0\" z=\"0\" w=\"0\" />\n     <Item name=\"specularFresnel\" type=\"Vector\" x=\"0.97\" y=\"0\" z=\"0\" w=\"0\" />\n    </Parameters>\n   </Item>\n");
            continue;
        }
        let (file, bucket, hard_alpha) = if *kind == Kind::Cutout { ("cutout.sps", 3, 0) } else { ("default.sps", 0, 1) };
        let _ = write!(
            s,
            "   <Item>\n    <Name>hash_E4DF46D5</Name>\n    <FileName>{file}</FileName>\n    <RenderBucket value=\"{bucket}\" />\n    <Parameters>\n     <Item name=\"DiffuseSampler\" type=\"Texture\">\n      <Name>{stem}</Name>\n     </Item>\n     <Item name=\"matMaterialColorScale\" type=\"Vector\" x=\"1\" y=\"0\" z=\"0\" w=\"1\" />\n     <Item name=\"HardAlphaBlend\" type=\"Vector\" x=\"{hard_alpha}\" y=\"0\" z=\"0\" w=\"0\" />\n     <Item name=\"useTessellation\" type=\"Vector\" x=\"0\" y=\"0\" z=\"0\" w=\"0\" />\n     <Item name=\"wetnessMultiplier\" type=\"Vector\" x=\"1\" y=\"0\" z=\"0\" w=\"0\" />\n     <Item name=\"globalAnimUV1\" type=\"Vector\" x=\"0\" y=\"1\" z=\"0\" w=\"0\" />\n     <Item name=\"globalAnimUV0\" type=\"Vector\" x=\"1\" y=\"0\" z=\"0\" w=\"0\" />\n    </Parameters>\n   </Item>\n"
        );
    }
    s.push_str("  </Shaders>\n </ShaderGroup>\n <DrawableModelsHigh>\n  <Item>\n   <RenderMask value=\"255\" />\n   <Flags value=\"0\" />\n   <HasSkin value=\"0\" />\n   <BoneIndex value=\"0\" />\n   <Unknown1 value=\"0\" />\n   <Geometries>\n");
    for g in geoms {
        let (glo, ghi) = bbox(g.pos.iter());
        let _ = writeln!(s, "    <Item>\n     <ShaderIndex value=\"{}\" />", g.shader);
        let _ = writeln!(s, "     {}\n     {}", v4w("BoundingBoxMin", glo), v4w("BoundingBoxMax", ghi));
        s.push_str("     <VertexBuffer>\n      <Flags value=\"0\" />\n      <Layout type=\"GTAV1\">\n       <Position />\n       <Normal />\n       <Colour0 />\n       <TexCoord0 />\n      </Layout>\n      <Data>\n");
        for i in 0..g.pos.len() {
            let (p, n, t) = (g.pos[i], g.nrm[i], g.uv[i]);
            let _ = writeln!(
                s,
                "       {:.4} {:.4} {:.4}   {:.4} {:.4} {:.4}   255 255 255 255   {:.5} {:.5}",
                p[0], p[1], p[2], n[0], n[1], n[2], t[0], t[1]
            );
        }
        s.push_str("      </Data>\n     </VertexBuffer>\n     <IndexBuffer>\n      <Data>\n");
        for chunk in g.idx.chunks(24) {
            s.push_str("       ");
            for (k, i) in chunk.iter().enumerate() {
                if k > 0 {
                    s.push(' ');
                }
                let _ = write!(s, "{i}");
            }
            s.push('\n');
        }
        s.push_str("      </Data>\n     </IndexBuffer>\n    </Item>\n");
    }
    s.push_str("   </Geometries>\n  </Item>\n </DrawableModelsHigh>\n <Lights />\n</Drawable>\n");
    std::fs::write(out.join(format!("{name}.ydr.xml")), s)?;
    Ok(())
}

/// Texture dictionary XML for the given textures; their DDS files are next
/// to it, where CodeWalker's importer looks for them.
fn write_ytd(out: &Path, txd: &str, stems: &[String]) -> Result<()> {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<TextureDictionary>\n");
    for stem in stems {
        let (w, h, mips, format) = dds_info(&out.join(format!("{stem}.dds")))?;
        let _ = write!(
            s,
            " <Item>\n  <Name>{stem}</Name>\n  <Unk32 value=\"128\" />\n  <Usage>DIFFUSE</Usage>\n  <UsageFlags>UNK24</UsageFlags>\n  <ExtraFlags value=\"0\" />\n  <Width value=\"{w}\" />\n  <Height value=\"{h}\" />\n  <MipLevels value=\"{mips}\" />\n  <Format>{format}</Format>\n  <FileName>{stem}.dds</FileName>\n </Item>\n"
        );
    }
    s.push_str("</TextureDictionary>\n");
    std::fs::write(out.join(format!("{txd}.ytd.xml")), s)?;
    Ok(())
}

fn write_ytyp(out: &Path, models: &[UnitModel], opts: &Options) -> Result<()> {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapTypes>\n <extensions />\n <archetypes>\n");
    for m in models {
        let centre = [0, 1, 2].map(|k| (m.bb_min[k] + m.bb_max[k]) * 0.5);
        let radius = (0..3).map(|k| (m.bb_max[k] - m.bb_min[k]).powi(2)).sum::<f32>().sqrt() * 0.5;
        let _ = write!(
            s,
            "  <Item type=\"CBaseArchetypeDef\">\n   <lodDist value=\"{}\" />\n   <flags value=\"0\" />\n   <specialAttribute value=\"0\" />\n   {}\n   {}\n   {}\n   <bsRadius value=\"{radius}\" />\n   <hdTextureDist value=\"200\" />\n   <name>{}</name>\n   <textureDictionary>{}</textureDictionary>\n   <clipDictionary />\n   <drawableDictionary />\n   <physicsDictionary />\n   <assetType>ASSET_TYPE_DRAWABLE</assetType>\n   <assetName>{}</assetName>\n   <extensions />\n  </Item>\n",
            m.lod_dist,
            v3("bbMin", m.bb_min),
            v3("bbMax", m.bb_max),
            v3("bsCentre", centre),
            m.name,
            m.txd,
            m.name
        );
    }
    let _ = write!(
        s,
        " </archetypes>\n <name>{}paradise</name>\n <dependencies />\n <compositeEntityTypes itemType=\"CCompositeEntityType\" />\n</CMapTypes>\n",
        opts.prefix
    );
    std::fs::write(out.join(format!("{}paradise.ytyp.xml", opts.prefix)), s)?;
    Ok(())
}

/// A unit's map file. With `parent` (the LOD map's name and the index of the
/// unit's stand-in there) its drawables are that stand-in's children.
fn write_ymap(out: &Path, unit: u32, models: &[UnitModel], parent: Option<(&str, usize)>, opts: &Options) -> Result<()> {
    let ymap = format!("{}map_{unit}", opts.prefix);
    let d = opts.lod_dist;
    let (parent_tag, flags, parent_index, level) = match parent {
        Some((name, i)) => (format!("<parent>{name}</parent>"), 1572872, i as i64, "LODTYPES_DEPTH_HD"),
        None => ("<parent />".to_string(), 1572864, -1, "LODTYPES_DEPTH_ORPHANHD"),
    };
    let mut wmin = [f32::MAX; 3];
    let mut wmax = [f32::MIN; 3];
    let mut entities = String::new();
    for m in models {
        for k in 0..3 {
            wmin[k] = wmin[k].min(m.origin[k] + m.bb_min[k]);
            wmax[k] = wmax[k].max(m.origin[k] + m.bb_max[k]);
        }
        let _ = write!(
            entities,
            "  <Item type=\"CEntityDef\">\n   <archetypeName>{}</archetypeName>\n   <flags value=\"{flags}\" />\n   <guid value=\"{}\" />\n   {}\n   <rotation x=\"0\" y=\"0\" z=\"0\" w=\"1\" />\n   <scaleXY value=\"1\" />\n   <scaleZ value=\"1\" />\n   <parentIndex value=\"{parent_index}\" />\n   <lodDist value=\"{d}\" />\n   <childLodDist value=\"0\" />\n   <lodLevel>{level}</lodLevel>\n   <numChildren value=\"0\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
            m.name,
            joaat(&m.name),
            v3("position", m.origin)
        );
    }
    let smin = [wmin[0] - d, wmin[1] - d, wmin[2] - d];
    let smax = [wmax[0] + d, wmax[1] + d, wmax[2] + d];
    let head = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>{ymap}</name>\n {parent_tag}\n <flags value=\"0\" />\n <contentFlags value=\"1\" />\n {}\n {}\n {}\n {}\n <entities>\n",
        v3("streamingExtentsMin", smin),
        v3("streamingExtentsMax", smax),
        v3("entitiesExtentsMin", wmin),
        v3("entitiesExtentsMax", wmax)
    );
    let tail = YMAP_TAIL.replace("{ymap}", &ymap);
    std::fs::write(out.join(format!("{ymap}.ymap.xml")), head + &entities + &tail)?;
    Ok(())
}

/// Static remainder of the CMapData document after the entity list
/// (empty occluders, instance data, LOD lights, block info).
pub(crate) const YMAP_TAIL: &str = "</entities>\n <containerLods itemType=\"rage__fwContainerLodDef\" />\n <boxOccluders itemType=\"BoxOccluder\" />\n <occludeModels itemType=\"OccludeModel\" />\n <physicsDictionaries />\n <instancedData>\n  <ImapLink />\n  <PropInstanceList itemType=\"rage__fwPropInstanceListDef\" />\n  <GrassInstanceList itemType=\"rage__fwGrassInstanceListDef\" />\n </instancedData>\n <timeCycleModifiers itemType=\"CTimeCycleModifier\" />\n <carGenerators itemType=\"CCarGen\" />\n <LODLightsSOA>\n  <direction itemType=\"FloatXYZ\" />\n  <falloff />\n  <falloffExponent />\n  <timeAndStateFlags />\n  <hash />\n  <coneInnerAngle />\n  <coneOuterAngleOrCapExt />\n  <coronaIntensity />\n </LODLightsSOA>\n <DistantLODLightsSOA>\n  <position itemType=\"FloatXYZ\" />\n  <RGBI />\n  <numStreetLights value=\"0\" />\n  <category value=\"0\" />\n </DistantLODLightsSOA>\n <block>\n  <version value=\"0\" />\n  <flags value=\"0\" />\n  <name>{ymap}</name>\n  <exportedBy>gta-paradise</exportedBy>\n  <owner></owner>\n  <time>10 October 2026 00:00</time>\n </block>\n</CMapData>\n";

/// Writes every placed triangle of Burnout's visible water meshes (materials
/// named `Gen_Water*`) as `material,x1,y1,z1,x2,y2,z2,x3,y3,z3` CSV rows:
/// real water levels for lakes and rivers (Burnout's "water" collision is
/// the seabed). `bpx gta-water --lakes` reads it.
pub fn water_meshes(game: &Path, out: &Path, placement: Placement) -> Result<()> {
    let mut csv = String::new();
    let mut units: Vec<PathBuf> = std::fs::read_dir(game)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("TRK_UNIT") && n.ends_with("_GR.BNDL")))
        .collect();
    units.sort();
    for path in &units {
        let b = Bundle::open(path)?;
        let Some(list) = b.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else { continue };
        for inst in bw::parse_instance_list(&Res::load(&b, list)?) {
            let geo = (|| -> Option<Vec<bw::MeshGeometry>> {
                let m = bw::parse_model(&Res::load(&b, b.find(inst.model)?).ok()?);
                bw::decode_renderable(&b, b.find(m.lod0()?)?).ok()
            })()
            .unwrap_or_default();
            for g in &geo {
                let Some(name) = g.material.and_then(|m| b.name_of(m)).map(crate::export::short_name) else { continue };
                if !name.starts_with("Gen_Water") {
                    continue;
                }
                let placed: Vec<[f32; 3]> = g.positions.iter().map(|&p| placement.apply(transform_point(&inst.transform, p))).collect();
                for t in g.indices.chunks_exact(3) {
                    let _ = write!(csv, "{name}");
                    for &k in t {
                        let q = placed[k as usize];
                        let _ = write!(csv, ",{:.2},{:.2},{:.2}", q[0], q[1], q[2]);
                    }
                    csv.push('\n');
                }
            }
        }
    }
    std::fs::write(out, csv)?;
    Ok(())
}
