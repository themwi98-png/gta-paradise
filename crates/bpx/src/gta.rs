//! Converts Burnout Paradise world collision into GTA V static collision as
//! CodeWalker XML (`.ybn.xml`), one bounds file per Burnout track unit.
//! `cwconv convert` (CodeWalker.Core) then builds the binary `.ybn`, including
//! quantisation and BVH trees.

use crate::repair;
use anyhow::Result;
use bpworld::{self as bw, CollisionTag};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

/// Where and how big the Burnout map is placed in GTA's world.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    /// GTA world position of Burnout's origin.
    pub offset: [f32; 3],
    pub scale: f32,
    /// Rotation about GTA's up axis, degrees.
    pub heading: f32,
}

impl Placement {
    /// Burnout is right-handed Y-up, GTA right-handed Z-up:
    /// (x, y, z) -> (x, -z, y), then scale, rotate, offset.
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let (x, y, z) = (p[0] * self.scale, -p[2] * self.scale, p[1] * self.scale);
        let (s, c) = self.heading.to_radians().sin_cos();
        [x * c - y * s + self.offset[0], x * s + y * c + self.offset[1], z + self.offset[2]]
    }

    /// Same axis change and heading rotation for directions (normals): no
    /// offset or scale.
    pub fn apply_dir(&self, d: [f32; 3]) -> [f32; 3] {
        let (x, y, z) = (d[0], -d[2], d[1]);
        let (s, c) = self.heading.to_radians().sin_cos();
        [x * c - y * s, x * s + y * c, z]
    }
}

/// What becomes of Burnout's invisible walls (the barriers that keep cars on
/// the playable map).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Walls {
    Skip,
    /// Only vehicles hit them; peds, bullets and the camera pass through.
    Vehicles,
    /// Solid to everything, like any other wall.
    Solid,
}

impl Walls {
    fn name(self) -> &'static str {
        match self {
            Walls::Skip => "skip",
            Walls::Vehicles => "vehicles",
            Walls::Solid => "solid",
        }
    }
}

pub struct Options {
    pub placement: Placement,
    pub invisible_walls: Walls,
    /// Prefix for output names; GTA file names must be globally unique.
    pub prefix: String,
}

/// GTA bounds geometry stores 16-bit vertex indices with flag bits, so keep
/// each child comfortably under 32k vertices (and polygons, which BVH nodes
/// index with 16-bit item ids).
const MAX_CHILD_VERTS: usize = 30000;
const MAX_CHILD_POLYS: usize = 32000;

/// Burnout's collision is effectively two-sided and its triangle winding is
/// arbitrary (~40% of floor triangles face down), but GTA's is one-sided: a
/// down-facing floor can't be stood on. Triangles flatter than this (|nz| of
/// the unit normal) are made to face up; steeper ones (walls) are faced by the
/// floors they join or made into slabs (`repair::solidify_walls`).
const FLOOR_MIN_NZ: f32 = 0.25;
/// Thickness of walls that are made into slabs: free-standing walls, fences
/// and barriers, whose tops become ledges peds can climb.
const WALL_THICKNESS: f32 = 0.2;
/// Marks generated collision (crate::fill, from Burnout's visible models) in
/// a triangle's material while the unit is repaired: it keeps the winding it
/// was made with (ground facing up, the rest as drawn), one-sided, instead of
/// being flipped, faced or made a slab.
const GENERATED: u16 = 0x8000;

/// Vertices of different Burnout polygon soups closer than this are merged.
const WELD_TOLERANCE: f32 = 0.03;
/// Vertices within this distance of another triangle's open edge split it.
const TJUNCTION_TOLERANCE: f32 = 0.03;

/// Static map collision flags, copied from Rockstar's own ground collision
/// (downtown dt1_00_0.ybn children: type 0x0000003E, include 0x07F3BEC0).
/// TEST_VEHICLE_WHEEL matters: vehicle wheels find the ground with it.
const COMPOSITE_FLAGS1: &str = "MAP_WEAPON, MAP_DYNAMIC, MAP_ANIMAL, MAP_COVER, MAP_VEHICLE";
const COMPOSITE_FLAGS2: &str = "VEHICLE_NOT_BVH, VEHICLE_BVH, PED, RAGDOLL, ANIMAL, ANIMAL_RAGDOLL, OBJECT, PLANT, PROJECTILE, EXPLOSION, FORKLIFT_FORKS, TEST_WEAPON, TEST_CAMERA, TEST_AI, TEST_SCRIPT, TEST_VEHICLE_WHEEL, GLASS";
/// Vehicle-only collision. Rockstar's invisible road barriers (fwy_01_4,
/// fwy_02_1, dt1_rd1_6 children) are MAP_VEHICLE; these are MAP_STAIRS, a
/// type every vehicle includes but meets nowhere else, and include only
/// vehicles: peds, bullets, the camera and wheel probes pass. paradise.asi's
/// wallpass clears MAP_STAIRS from aircraft and boats' include flags in the
/// physics level, so they pass too, while the map (MAP_VEHICLE) still stops
/// them. Collision tests each side's include against the other's type.
const VEHICLE_FLAGS1: &str = "MAP_STAIRS";
const VEHICLE_FLAGS2: &str = "VEHICLE_NOT_BVH, VEHICLE_BVH";
/// Overlay kind of vehicle-only walls (0 floor, 1 wall).
const KIND_VEHICLE_WALL: u8 = 2;

#[derive(Default)]
struct Facing {
    flipped: usize,
    walls: usize,
    generated: usize,
}

/// Z component of a triangle's unit normal (counter-clockwise winding).
fn unit_normal_z(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 {
        n[2] / len
    } else {
        0.0
    }
}

#[derive(Default)]
struct Child {
    verts: Vec<[f32; 3]>,
    index: HashMap<[u32; 3], u32>,
    /// (gta material index, v1, v2, v3)
    tris: Vec<(u16, u32, u32, u32)>,
    /// Vehicle-only collision (VEHICLE_FLAGS1/2).
    vehicle_only: bool,
}

impl Child {
    fn vert(&mut self, p: [f32; 3]) -> u32 {
        let key = p.map(f32::to_bits);
        let next = self.verts.len() as u32;
        *self.index.entry(key).or_insert_with(|| {
            self.verts.push(p);
            next
        })
    }
}

fn bbox(points: impl Iterator<Item = [f32; 3]>) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

fn v3(tag: &str, p: [f32; 3]) -> String {
    format!("<{tag} x=\"{}\" y=\"{}\" z=\"{}\" />", p[0], p[1], p[2])
}

/// Common header fields shared by composite and children.
fn bounds_header(s: &mut String, ind: &str, lo: [f32; 3], hi: [f32; 3], margin: f32) {
    let c = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
    let r = (0..3).map(|k| (hi[k] - lo[k]).powi(2)).sum::<f32>().sqrt() * 0.5;
    for line in [
        v3("BoxMin", lo),
        v3("BoxMax", hi),
        v3("BoxCenter", c),
        v3("SphereCenter", c),
        format!("<SphereRadius value=\"{r}\" />"),
        format!("<Margin value=\"{margin}\" />"),
        "<Volume value=\"1\" />".into(),
        "<Inertia x=\"1\" y=\"1\" z=\"1\" />".into(),
        "<MaterialIndex value=\"0\" />".into(),
        "<MaterialColourIndex value=\"0\" />".into(),
        "<ProceduralID value=\"0\" />".into(),
        "<RoomID value=\"0\" />".into(),
        "<PedDensity value=\"0\" />".into(),
        "<UnkFlags value=\"0\" />".into(),
        "<PolyFlags value=\"0\" />".into(),
        "<UnkType value=\"1\" />".into(),
    ] {
        let _ = writeln!(s, "{ind}{line}");
    }
}

fn write_ybn_xml(children: &[Child]) -> String {
    let all = bbox(children.iter().flat_map(|c| c.verts.iter().copied()));
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BoundsFile>\n <Bounds type=\"Composite\">\n");
    bounds_header(&mut s, "  ", all.0, all.1, 0.0);
    s.push_str("  <Children>\n");
    for ch in children {
        let (lo, hi) = bbox(ch.verts.iter().copied());
        let centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
        s.push_str("   <Item type=\"GeometryBVH\">\n");
        bounds_header(&mut s, "    ", lo, hi, 0.005);
        s.push_str("    <CompositeTransform>\n     1 0 0 0\n     0 1 0 0\n     0 0 1 0\n     0 0 0 1\n    </CompositeTransform>\n");
        let (flags1, flags2) = if ch.vehicle_only { (VEHICLE_FLAGS1, VEHICLE_FLAGS2) } else { (COMPOSITE_FLAGS1, COMPOSITE_FLAGS2) };
        let _ = writeln!(s, "    <CompositeFlags1>{flags1}</CompositeFlags1>");
        let _ = writeln!(s, "    <CompositeFlags2>{flags2}</CompositeFlags2>");
        let _ = writeln!(s, "    {}", v3("GeometryCenter", centre));
        s.push_str("    <UnkFloat1 value=\"0\" />\n    <UnkFloat2 value=\"0.0025\" />\n");

        // Per-child material table; triangles index into it.
        let mut mats: Vec<u16> = ch.tris.iter().map(|t| t.0).collect();
        mats.sort();
        mats.dedup();
        s.push_str("    <Materials>\n");
        for m in &mats {
            let _ = writeln!(
                s,
                "     <Item>\n      <Type value=\"{m}\" />\n      <ProceduralID value=\"0\" />\n      <RoomID value=\"0\" />\n      <PedDensity value=\"0\" />\n      <Flags>NONE</Flags>\n      <MaterialColourIndex value=\"0\" />\n      <Unk value=\"0\" />\n     </Item>"
            );
        }
        s.push_str("    </Materials>\n    <Vertices>\n");
        for v in &ch.verts {
            let _ = writeln!(s, "     {}, {}, {}", v[0] - centre[0], v[1] - centre[1], v[2] - centre[2]);
        }
        s.push_str("    </Vertices>\n    <Polygons>\n");
        for &(m, a, b, c) in &ch.tris {
            let mi = mats.binary_search(&m).unwrap();
            let _ = writeln!(s, "     <Triangle m=\"{mi}\" v1=\"{a}\" v2=\"{b}\" v3=\"{c}\" f1=\"0\" f2=\"0\" f3=\"0\" />");
        }
        s.push_str("    </Polygons>\n   </Item>\n");
    }
    s.push_str("  </Children>\n </Bounds>\n</BoundsFile>\n");
    s
}

/// Appends `tris` to `children`, starting a new child when the last one is
/// full or of the other kind.
fn fill_children(children: &mut Vec<Child>, verts: &[[f32; 3]], tris: &[repair::Tri], vehicle_only: bool) {
    for t in tris {
        let full = children.last().map_or(true, |c| {
            c.vehicle_only != vehicle_only || c.verts.len() + 3 > MAX_CHILD_VERTS || c.tris.len() + 1 > MAX_CHILD_POLYS
        });
        if full {
            children.push(Child { vehicle_only, ..Child::default() });
        }
        let ch = children.last_mut().unwrap();
        let [a, b, c] = t.v.map(|i| ch.vert(verts[i as usize]));
        ch.tris.push((t.mat, a, b, c));
    }
}

pub fn export_collision(col: &Path, out: &Path, opts: &Options) -> Result<()> {
    let units = crate::colsrc::load(col)?;
    std::fs::create_dir_all(out)?;
    let mut manifest = Vec::new();
    let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut files, mut tris_total) = (0usize, 0usize);


    let edits = crate::edits::CollisionEdits::load()?;
    let mut facing = Facing::default();
    let mut repair_stats = repair::Stats::default();
    let mut wall_stats = repair::WallStats::default();
    let mut overlay = Overlay::default();
    let mut vehicle_wall_stats = repair::WallStats::default();
    let (mut vehicle_walls_in, mut vehicle_walls_out) = (0usize, 0usize);
    for (unit, polys, ids) in units {
        let generated: Vec<bool> = ids.iter().map(|&id| (crate::fill::FILL_ID..crate::colsrc::MODDED_ID).contains(&id)).collect();
        let (polys, generated): (Vec<_>, Vec<bool>) = polys.into_iter().zip(ids.iter().zip(generated)).filter(|(_, (&id, _))| !edits.is_deleted(unit, id)).map(|(p, (_, g))| (p, g)).unzip();
        // Gather the unit's triangles into one welded vertex pool, so seams
        // between Burnout's separate polygon soups can be repaired.
        let mut pool = repair::Pool::new(WELD_TOLERANCE);
        let mut tris: Vec<repair::Tri> = Vec::new();
        // Vehicle-only walls are kept apart: their own pool, so they don't
        // join the map's surfaces, and their own children.
        let mut wall_pool = repair::Pool::new(WELD_TOLERANCE);
        let mut walls: Vec<repair::Tri> = Vec::new();
        for (p, &gen) in polys.iter().zip(&generated) {
            let tag = CollisionTag::decode(p.tag);
            if tag.surface_id == bw::SURFACE_WATER {
                *skipped.entry("water").or_default() += 1;
                continue;
            }
            let mat = bw::surface_gta_material_index(tag.surface_id);
            if tag.surface_id == bw::SURFACE_INVISIBLE_WALL {
                match opts.invisible_walls {
                    Walls::Skip => {
                        *skipped.entry("invisible_wall").or_default() += 1;
                        continue;
                    }
                    Walls::Vehicles => {
                        let idx: Vec<u32> = p.verts.iter().map(|&v| wall_pool.add(opts.placement.apply(v))).collect();
                        walls.push(repair::Tri { mat, v: [idx[0], idx[1], idx[2]] });
                        if idx.len() == 4 {
                            walls.push(repair::Tri { mat, v: [idx[0], idx[2], idx[3]] });
                        }
                        continue;
                    }
                    Walls::Solid => {}
                }
            }
            let world: Vec<[f32; 3]> = p.verts.iter().map(|&v| opts.placement.apply(v)).collect();
            let idx: Vec<u32> = world.iter().map(|&v| pool.add(v)).collect();
            let mut push = |a: usize, b: usize, c: usize| {
                let nz = unit_normal_z(world[a], world[b], world[c]);
                let (ia, ib, ic) = (idx[a], idx[b], idx[c]);
                if gen {
                    tris.push(repair::Tri { mat: mat | GENERATED, v: [ia, ib, ic] });
                    facing.generated += 1;
                } else if nz.abs() >= FLOOR_MIN_NZ {
                    if nz > 0.0 {
                        tris.push(repair::Tri { mat, v: [ia, ib, ic] });
                    } else {
                        tris.push(repair::Tri { mat, v: [ia, ic, ib] });
                        facing.flipped += 1;
                    }
                } else {
                    tris.push(repair::Tri { mat, v: [ia, ib, ic] });
                    facing.walls += 1;
                }
            };
            push(0, 1, 2);
            if idx.len() == 4 {
                push(0, 2, 3);
            }
        }
        repair_stats.welded += std::mem::take(&mut pool.welded);
        // Degenerate triangles break GTA's BVH/edge building; cleanup drops
        // them (including ones collapsed by welding).
        repair::cleanup(&pool.verts, &mut tris, &mut repair_stats);
        repair::fix_tjunctions(&pool.verts, &mut tris, TJUNCTION_TOLERANCE, &mut repair_stats);
        repair::cleanup(&pool.verts, &mut tris, &mut repair_stats);
        let (kept, rest): (Vec<repair::Tri>, Vec<repair::Tri>) = tris.into_iter().partition(|t| t.mat & GENERATED != 0);
        tris = rest;
        let kept: Vec<repair::Tri> = kept.into_iter().map(|t| repair::Tri { mat: t.mat & !GENERATED, v: t.v }).collect();
        overlay.add(&pool.verts, &tris);
        overlay.add(&pool.verts, &kept);
        let is_wall = |v: &[[f32; 3]], t: &repair::Tri| {
            let [a, b, c] = t.v.map(|i| v[i as usize]);
            unit_normal_z(a, b, c).abs() < FLOOR_MIN_NZ
        };
        repair::solidify_walls(&mut pool.verts, &mut tris, is_wall, WALL_THICKNESS, &mut wall_stats);
        tris.extend(kept);
        repair::spatial_sort(&pool.verts, &mut tris);

        // No floors share their edges, so every one becomes a centred slab:
        // solid from both sides (GTA's polygons are one-sided).
        repair_stats.welded += std::mem::take(&mut wall_pool.welded);
        repair::cleanup(&wall_pool.verts, &mut walls, &mut repair_stats);
        overlay.add_kind(&wall_pool.verts, &walls, KIND_VEHICLE_WALL);
        vehicle_walls_in += walls.len();
        repair::solidify_walls(&mut wall_pool.verts, &mut walls, |_, _| true, WALL_THICKNESS, &mut vehicle_wall_stats);
        repair::spatial_sort(&wall_pool.verts, &mut walls);
        vehicle_walls_out += walls.len();

        let mut children = Vec::new();
        fill_children(&mut children, &pool.verts, &tris, false);
        fill_children(&mut children, &wall_pool.verts, &walls, true);
        if children.is_empty() {
            continue;
        }
        let name = format!("{}col_{unit}", opts.prefix);
        std::fs::write(out.join(format!("{name}.ybn.xml")), write_ybn_xml(&children))?;
        let (lo, hi) = bbox(children.iter().flat_map(|c| c.verts.iter().copied()));
        let tris: usize = children.iter().map(|c| c.tris.len()).sum();
        let vehicle_only: usize = children.iter().filter(|c| c.vehicle_only).map(|c| c.tris.len()).sum();
        manifest.push(json!({"name": name, "unit": unit, "min": lo, "max": hi, "children": children.len(), "triangles": tris, "vehicle_only_triangles": vehicle_only}));
        files += 1;
        tris_total += tris;
    }

    let p = opts.placement;
    overlay.write(&out.join("paradise_col.bin"))?;
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "source": col.display().to_string(),
            "placement": {"offset": p.offset, "scale": p.scale, "heading": p.heading},
            "invisible_walls": opts.invisible_walls.name(),
            "files": manifest,
        }))?,
    )?;
    println!(
        "wrote {files} .ybn.xml files, {tris_total} triangles; floors flipped to face up {}, walls {}; {} generated triangles kept as made; skipped {skipped:?}",
        facing.flipped, facing.walls, facing.generated
    );
    println!(
        "seam repair: {} vertices welded, {} T-junction splits, {} degenerate and {} duplicate triangles removed",
        repair_stats.welded, repair_stats.tjunction_splits, repair_stats.degenerate_removed, repair_stats.duplicates_removed
    );
    let w = &wall_stats;
    println!(
        "walls: {} triangles one-sided (closed, faced by their floors), {} made {WALL_THICKNESS} m slabs behind their face, {} centred slabs (no face known); {} cap triangles; {} surfaces with disagreeing floors",
        w.one_sided, w.slab_behind, w.slab_centred, w.caps, w.conflicts
    );
    println!(
        "invisible walls ({}): {vehicle_walls_in} triangles made vehicle-only, {vehicle_walls_out} after making them {WALL_THICKNESS} m slabs ({} cap triangles)",
        opts.invisible_walls.name(),
        vehicle_wall_stats.caps
    );
    Ok(())
}

/// Writes a flat tarmac slab (`size` x `size` m, 2 m thick, top face at
/// `top`) as a standalone static bounds file, for in-game streaming tests
/// independent of the Burnout data.
pub fn export_test_platform(out: &Path, name: &str, top: [f32; 3], size: f32) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let (h, t) = (size * 0.5, 2.0);
    let mut ch = Child::default();
    let corners: Vec<u32> = (0..8)
        .map(|i| {
            let x = top[0] + if i & 1 == 0 { -h } else { h };
            let y = top[1] + if i & 2 == 0 { -h } else { h };
            let z = top[2] - if i & 4 == 0 { 0.0 } else { t };
            ch.vert([x, y, z])
        })
        .collect();
    // Two triangles per face, wound outward.
    let faces = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    for f in faces {
        let m = bw::surface_gta_material_index(1);
        ch.tris.push((m, corners[f[0]], corners[f[1]], corners[f[2]]));
        ch.tris.push((m, corners[f[0]], corners[f[2]], corners[f[3]]));
    }
    std::fs::write(out.join(format!("{name}.ybn.xml")), write_ybn_xml(&[ch]))?;
    println!("wrote test platform {name} at {top:?}, {size} m");
    Ok(())
}

/// Collision preview for paradise.asi's in-game overlay: the same repaired
/// triangles (walls as Burnout has them, before they're made into slabs),
/// bucketed into square tiles.
///
/// Layout (little-endian): "PCOL", u32 version (1), f32 tile size, u32 tile
/// count, then per tile {i32 tx, i32 ty, u32 first, u32 count}, then per
/// triangle {9 x f32 vertex coords, u16 GTA material index, u8 kind
/// (0 floor, 1 wall, 2 vehicle-only wall), u8 pad}.
#[derive(Default)]
struct Overlay {
    tris: Vec<([[f32; 3]; 3], u16, u8)>,
}

const OVERLAY_TILE: f32 = 64.0;

impl Overlay {
    fn add(&mut self, verts: &[[f32; 3]], tris: &[repair::Tri]) {
        for t in tris {
            let p = t.v.map(|i| verts[i as usize]);
            let kind = if unit_normal_z(p[0], p[1], p[2]).abs() >= FLOOR_MIN_NZ { 0 } else { 1 };
            self.tris.push((p, t.mat, kind));
        }
    }

    fn add_kind(&mut self, verts: &[[f32; 3]], tris: &[repair::Tri], kind: u8) {
        self.tris.extend(tris.iter().map(|t| (t.v.map(|i| verts[i as usize]), t.mat, kind)));
    }

    fn write(&self, path: &Path) -> Result<()> {
        let tile_of = |t: &([[f32; 3]; 3], u16, u8)| {
            let c = [0, 1].map(|k| (t.0[0][k] + t.0[1][k] + t.0[2][k]) / 3.0);
            [(c[0] / OVERLAY_TILE).floor() as i32, (c[1] / OVERLAY_TILE).floor() as i32]
        };
        let mut order: Vec<usize> = (0..self.tris.len()).collect();
        order.sort_by_key(|&i| tile_of(&self.tris[i]));
        let mut tiles: Vec<([i32; 2], u32, u32)> = Vec::new();
        for (n, &i) in order.iter().enumerate() {
            let k = tile_of(&self.tris[i]);
            match tiles.last_mut() {
                Some(t) if t.0 == k => t.2 += 1,
                _ => tiles.push((k, n as u32, 1)),
            }
        }
        let mut b = Vec::with_capacity(16 + tiles.len() * 16 + self.tris.len() * 40);
        b.extend_from_slice(b"PCOL");
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&OVERLAY_TILE.to_le_bytes());
        b.extend_from_slice(&(tiles.len() as u32).to_le_bytes());
        for (k, first, count) in &tiles {
            for v in [k[0].to_le_bytes(), k[1].to_le_bytes(), first.to_le_bytes(), count.to_le_bytes()] {
                b.extend_from_slice(&v);
            }
        }
        for &i in &order {
            let (p, mat, kind) = &self.tris[i];
            for v in p.iter().flatten() {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.extend_from_slice(&mat.to_le_bytes());
            b.push(*kind);
            b.push(0);
        }
        std::fs::write(path, &b)?;
        println!("overlay: {} triangles in {} tiles -> {}", self.tris.len(), tiles.len(), path.display());
        Ok(())
    }
}
