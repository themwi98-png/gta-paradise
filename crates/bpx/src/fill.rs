//! Hole fill. Burnout's collision only covers where cars can go, and the
//! pieces added for this project (colsrc) don't meet it everywhere: the ones
//! that cut into roads were deleted by hand (tools/edits), and on a modded
//! map the ones that clash with its new collision are dropped. Peds fall
//! through the gaps.
//!
//! `bpx col-fill` fills every patch of Burnout's visible ground (terrain,
//! roads, junctions) that has no collision floor near its height with that
//! ground itself, cut exactly to the patch:
//! - a collision floor from COVER_BELOW under the ground to COVER_ABOVE over
//!   it covers the ground: no fill there, so nothing new is laid on, just
//!   over or just under an existing floor; over Burnout's own floors (roads)
//!   COVER_BELOW_OWN, so no fill makes a ceiling a vehicle could hit;
//! - where fill meets a floor within COVER_ABOVE of it, its edge is tucked
//!   TUCK under that floor, never above;
//! - fill is ground only, never walls;
//! - fill within NEAR of Burnout's own floors that rises more than RISE above
//!   them (a possible bump beside a road) is marked KIND_RAISED when a wall
//!   (solid or invisible) stands within NEAR of it, else KIND_OPEN, to review.
//!
//! The fill is kept in its own file (ENV, else DEFAULT_PATH when it exists)
//! and colsrc::load adds it to every unit, with edit ids FILL_ID + its index.

use crate::colsrc::MarkedUnit;
use anyhow::{bail, Context, Result};
use bnd2::Bundle;
use bpworld::{self as bw, types, CollisionPoly, CollisionTag, Res};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::scale::FixedScaleFloatOverlay;
use i_overlay::float::single::SingleFloatOverlay;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Environment variable naming the fill file ("none": no fill).
pub const ENV: &str = "BPX_FILL";
pub const DEFAULT_PATH: &str = "out/collision_fill/paradise_fill.bin";
/// Collision edit ids of fill triangles: FILL_ID + index in the unit's fill.
pub const FILL_ID: u32 = 0x4000_0000;
const MAGIC: &[u8; 4] = b"PFIL";

pub const KIND_FILL: u8 = 0;
/// Beside Burnout's own floor and higher than it, with a wall (solid or
/// invisible) within NEAR: cars can't get onto it.
pub const KIND_RAISED: u8 = 1;
/// Beside Burnout's own floor and higher than it, no wall near: to review.
pub const KIND_OPEN: u8 = 2;
/// `bpx col-gen` only: steep ground (cliffs, banks), buildings and other
/// structures, and the mountains around the city (backdrops).
pub const KIND_CLIFF: u8 = 3;
pub const KIND_STRUCTURE: u8 = 4;
pub const KIND_MOUNTAIN: u8 = 5;

/// Floors: |unit normal . up| at least this (as gta.rs FLOOR_MIN_NZ).
const MIN_UP: f64 = 0.25;
const COVER_BELOW: f64 = 3.0;
/// Over Burnout's own floors: clearance for any vehicle under fill.
const COVER_BELOW_OWN: f64 = 6.0;
const COVER_ABOVE: f64 = 1.0;
const TUCK: f64 = 0.05;
const NEAR: f64 = 1.0;
const RISE: f64 = 0.1;
/// Fill this far above a floor is a level of its own (a bridge over a road).
const LEVEL: f64 = COVER_BELOW_OWN;
/// Shape maths on a 1 cm grid (units per metre): notches under a centimetre
/// where floors meet vanish instead of making thousands of slivers.
const SNAP_SCALE: f64 = 100.0;
/// Fill triangles narrower than this (area / longest edge) are dropped.
const MIN_WIDTH: f64 = 0.02;
/// Burnout's sea level (GTA's z 0 at the map's 9.9 m offset): ground
/// entirely below it (the sea bed) isn't filled.
const SEA_Y: f64 = -9.9;
/// Model name prefixes (lower case) of Burnout's ground.
const GROUND: &[&str] = &["terrain", "terrian", "road_", "junction_", "t_", "bd_terrain"];
/// Models that get no collision of their own (`bpx col-gen`): plants, cables,
/// lights, cloth, water. Every other model that isn't ground is a structure.
const NOT_SOLID: &[&str] = &["tree", "bush", "shrub", "grass", "fol_", "cabledef", "pol_cable", "banner", "sho_", "water", "ponr", "dlc_"];
/// Generated walls, structures and mountains stay out of where a vehicle on
/// Burnout's own floors can be: within KEEP_OUT of such a floor (across), from
/// KEEP_BELOW under it to COVER_BELOW_OWN over it (its plane carried on).
const KEEP_OUT: f64 = 1.0;
const KEEP_BELOW: f64 = 0.5;
/// Burnout's model ids, ground or not (`bpx col-fill-ground-ids`), for a
/// modded map's rebuilt units, which have no names (BPX_GROUND_MODELS
/// overrides).
pub const DEFAULT_GROUND_MODELS: &str = "tools/collision/ground_models.txt";
/// A model stock Burnout doesn't have (a modded map's own) counts as ground
/// when collision already covers at least this share of its walkable surface:
/// ground has collision where cars go, a roof or a tree doesn't.
const NEW_MIN_COVERED: f64 = 0.5;

type P2 = [f64; 2];
type P3 = [f64; 3];

pub struct FillTri {
    pub tag: u32,
    pub kind: u8,
    pub verts: [[f32; 3]; 3],
}

// --- The fill file ----------------------------------------------------------

fn path() -> Option<(PathBuf, bool)> {
    match std::env::var_os(ENV) {
        Some(v) if v == "none" => None,
        Some(v) => Some((PathBuf::from(v), true)),
        None => Some((PathBuf::from(DEFAULT_PATH), false)),
    }
}

pub fn read(path: &Path) -> Result<Vec<(u32, Vec<FillTri>)>> {
    let mut data = Vec::new();
    ZlibDecoder::new(std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?).read_to_end(&mut data)?;
    let mut at = 0usize;
    let mut take = |n: usize| -> Result<&[u8]> {
        let s = data.get(at..at + n).context("hole fill file is cut short")?;
        at += n;
        Ok(s)
    };
    if take(4)? != MAGIC {
        bail!("{} is not a hole fill file", path.display());
    }
    let u32_of = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap());
    let version = u32_of(take(4)?);
    if version != 1 {
        bail!("{}: unknown hole fill version {version}", path.display());
    }
    let n = u32_of(take(4)?);
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let unit = u32_of(take(4)?);
        let count = u32_of(take(4)?);
        let mut tris = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let tag = u32_of(take(4)?);
            let kind = take(1)?[0];
            let b = take(36)?;
            let f = |i: usize| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
            tris.push(FillTri { tag, kind, verts: [[f(0), f(1), f(2)], [f(3), f(4), f(5)], [f(6), f(7), f(8)]] });
        }
        out.push((unit, tris));
    }
    Ok(out)
}

fn write(path: &Path, units: &[(u32, Vec<FillTri>)]) -> Result<()> {
    let mut w = Vec::new();
    w.extend_from_slice(MAGIC);
    w.extend_from_slice(&1u32.to_le_bytes());
    w.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for (unit, tris) in units {
        w.extend_from_slice(&unit.to_le_bytes());
        w.extend_from_slice(&(tris.len() as u32).to_le_bytes());
        for t in tris {
            w.extend_from_slice(&t.tag.to_le_bytes());
            w.push(t.kind);
            for v in &t.verts {
                v.iter().for_each(|c| w.extend_from_slice(&c.to_le_bytes()));
            }
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut z = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&w)?;
    std::fs::write(path, z.finish()?)?;
    Ok(())
}

/// Adds the hole fill (if any) to loaded collision units.
pub fn append(units: &mut Vec<MarkedUnit>) -> Result<()> {
    let Some((path, named)) = path() else { return Ok(()) };
    if !path.is_file() {
        if named {
            bail!("hole fill {} not found ({ENV})", path.display());
        }
        return Ok(());
    }
    let mut total = 0usize;
    for (unit, tris) in read(&path)? {
        total += tris.len();
        let i = match units.iter().position(|u| u.0 == unit) {
            Some(i) => i,
            None => {
                units.push((unit, Vec::new(), Vec::new(), Vec::new()));
                units.len() - 1
            }
        };
        let u = &mut units[i];
        for (k, t) in tris.into_iter().enumerate() {
            u.1.push(CollisionPoly { verts: t.verts.to_vec(), tag: t.tag });
            u.2.push(FILL_ID + k as u32);
            u.3.push(false);
        }
    }
    units.sort_by_key(|u| u.0);
    eprintln!("collision: {total} hole fill triangles from {}", path.display());
    Ok(())
}

/// `bpx col-fill-view <fill.bin> <out-dir>`: the fill as the viewer's
/// "Hole fill" layer, out-dir/unit<n>/unit<n>.gltf, one material per kind and
/// surface ("fill_", "fillwall_", "fillopen_", "cliff_", "struct_" or "mount_"
/// + "<surface id>_<name>").
pub fn export_viewer(fill: &Path, out: &Path) -> Result<()> {
    let units = read(fill)?;
    for (unit, tris) in &units {
        let mut groups: std::collections::BTreeMap<(u8, u8), (Vec<[f32; 3]>, Vec<u32>, Vec<u32>)> = Default::default();
        for (k, t) in tris.iter().enumerate() {
            let s = CollisionTag::decode(t.tag).surface_id;
            let (pos, idx, ids) = groups.entry((t.kind, s)).or_default();
            let base = pos.len() as u32;
            pos.extend_from_slice(&t.verts);
            idx.extend([base, base + 1, base + 2]);
            ids.push(FILL_ID + k as u32);
        }
        let mut g = crate::export::Gltf::default();
        let mut prims = Vec::new();
        for (&(kind, s), (pos, idx, ids)) in &groups {
            let (prefix, colour) = match kind {
                KIND_RAISED => ("fillwall_", [0.95, 0.8, 0.1, 1.0]),
                KIND_OPEN => ("fillopen_", [1.0, 0.2, 0.05, 1.0]),
                KIND_CLIFF => ("cliff_", [0.6, 0.42, 0.25, 1.0]),
                KIND_STRUCTURE => ("struct_", [0.35, 0.55, 0.95, 1.0]),
                KIND_MOUNTAIN => ("mount_", [0.62, 0.45, 0.82, 1.0]),
                _ => ("fill_", [0.15, 0.8, 0.35, 1.0]),
            };
            g.materials.push(serde_json::json!({
                "name": format!("{prefix}{s}_{}", bw::surface_name(s)),
                "doubleSided": true,
                "pbrMetallicRoughness": {"baseColorFactor": colour, "metallicFactor": 0.0, "roughnessFactor": 1.0}
            }));
            let mat = g.materials.len() - 1;
            let p = g.push_vec3(pos);
            let i = g.push_indices(idx);
            prims.push(serde_json::json!({"attributes": {"POSITION": p}, "indices": i, "material": mat, "extras": {"polygonIds": ids}}));
        }
        let name = format!("FILL_{unit}");
        g.meshes.push(serde_json::json!({"name": name, "primitives": prims}));
        g.nodes.push(serde_json::json!({"name": name, "mesh": 0}));
        let dir = out.join(format!("unit{unit}"));
        std::fs::create_dir_all(&dir)?;
        let stem = format!("unit{unit}");
        std::fs::write(dir.join(format!("{stem}.bin")), &g.bin)?;
        let doc = serde_json::json!({
            "asset": {"version": "2.0", "generator": "gta-paradise bpx"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": g.nodes,
            "meshes": g.meshes,
            "materials": g.materials,
            "accessors": g.accessors,
            "bufferViews": g.buffer_views,
            "buffers": [{"uri": format!("{stem}.bin"), "byteLength": g.bin.len()}],
        });
        std::fs::write(dir.join(format!("{stem}.gltf")), serde_json::to_vec(&doc)?)?;
    }
    println!("hole fill: viewer layer for {} units -> {}", units.len(), out.display());
    Ok(())
}

// --- Geometry ---------------------------------------------------------------

/// Height plane h = a x + b z + c (Burnout is y-up) through a triangle.
fn plane(p: &[P3; 3]) -> Option<[f64; 3]> {
    let (dx1, dy1, dz1) = (p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]);
    let (dx2, dy2, dz2) = (p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]);
    let det = dx1 * dz2 - dx2 * dz1;
    if det.abs() < 1e-9 {
        return None;
    }
    let a = (dy1 * dz2 - dy2 * dz1) / det;
    let b = (dx1 * dy2 - dx2 * dy1) / det;
    Some([a, b, p[0][1] - a * p[0][0] - b * p[0][2]])
}

fn at(f: [f64; 3], p: P2) -> f64 {
    f[0] * p[0] + f[1] * p[1] + f[2]
}

/// The triangle wound to face up (Burnout is y-up): ground pieces from the
/// triangulation have either winding, and the GTA build keeps generated
/// triangles as they are.
fn face_up(t: [P3; 3]) -> [P3; 3] {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    if u[2] * v[0] - u[0] * v[2] < 0.0 {
        [t[0], t[2], t[1]]
    } else {
        t
    }
}

/// |unit normal . up| of a triangle.
fn upness(p: &[P3; 3]) -> f64 {
    let u = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let v = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 0.0 {
        (n[1] / l).abs()
    } else {
        0.0
    }
}

fn cross(a: P2, b: P2, c: P2) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn area2(poly: &[P2]) -> f64 {
    (0..poly.len()).map(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        a[0] * b[1] - b[0] * a[1]
    }).sum()
}

fn ccw(mut poly: Vec<P2>) -> Vec<P2> {
    if area2(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

/// The part of a polygon where the linear function f is <= 0.
fn clip(poly: &[P2], f: [f64; 3]) -> Vec<P2> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (fa, fb) = (at(f, a), at(f, b));
        if fa <= 0.0 {
            out.push(a);
        }
        if (fa <= 0.0) != (fb <= 0.0) {
            let t = fa / (fa - fb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    out
}

/// Clips a polygon to a convex counter-clockwise one.
fn clip_convex(poly: Vec<P2>, by: &[P2]) -> Vec<P2> {
    let mut poly = poly;
    for i in 0..by.len() {
        if poly.len() < 3 {
            break;
        }
        let (a, b) = (by[i], by[(i + 1) % by.len()]);
        // Left of a->b is inside: -cross(a, b, p) <= 0.
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        poly = clip(&poly, [dz, -dx, -(dz * a[0] - dx * a[1])]);
    }
    poly
}

fn inside(t: &[P2; 3], p: P2, eps: f64) -> bool {
    let s = area2(t).signum();
    (0..3).all(|k| {
        let (a, b) = (t[k], t[(k + 1) % 3]);
        let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt().max(1e-12);
        cross(a, b, p) * s / l >= -eps
    })
}

/// Convex hull (counter-clockwise), monotone chain.
fn hull(mut pts: Vec<P2>) -> Vec<P2> {
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let mut h: Vec<P2> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &P2>> = if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in it {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], p) <= 0.0 {
                h.pop();
            }
            h.push(p);
        }
        h.pop();
    }
    h
}

/// A triangle grown by `r` in every direction (octagons round its corners,
/// drawn around the circle so they contain it).
fn grow(t: &[P2; 3], r: f64) -> Vec<P2> {
    let rr = r / (std::f64::consts::PI / 8.0).cos();
    let mut pts = Vec::with_capacity(24);
    for p in t {
        for k in 0..8 {
            let a = k as f64 * std::f64::consts::PI / 4.0;
            pts.push([p[0] + rr * a.cos(), p[1] + rr * a.sin()]);
        }
    }
    hull(pts)
}

// --- Floors -----------------------------------------------------------------

struct Floor {
    /// Footprint (x, z), counter-clockwise.
    t: [P2; 3],
    plane: [f64; 3],
    lo: P2,
    hi: P2,
    /// Lowest and highest point.
    y: [f64; 2],
    native: bool,
}

impl Floor {
    fn new(p: &[P3; 3], native: bool) -> Option<Floor> {
        if upness(p) < MIN_UP {
            return None;
        }
        let plane = plane(p)?;
        let t2 = ccw(p.iter().map(|v| [v[0], v[2]]).collect());
        let t = [t2[0], t2[1], t2[2]];
        let lo = [t.iter().map(|v| v[0]).fold(f64::MAX, f64::min), t.iter().map(|v| v[1]).fold(f64::MAX, f64::min)];
        let hi = [t.iter().map(|v| v[0]).fold(f64::MIN, f64::max), t.iter().map(|v| v[1]).fold(f64::MIN, f64::max)];
        let y = [p.iter().map(|v| v[1]).fold(f64::MAX, f64::min), p.iter().map(|v| v[1]).fold(f64::MIN, f64::max)];
        Some(Floor { t, plane, lo, hi, y, native })
    }

    /// How far under ground this floor still covers it.
    fn below(&self) -> f64 {
        if self.native {
            COVER_BELOW_OWN
        } else {
            COVER_BELOW
        }
    }
}

/// Uniform grid over footprints.
struct Grid {
    cell: f64,
    map: HashMap<(i32, i32), Vec<u32>>,
}

impl Grid {
    fn new(cell: f64) -> Self {
        Grid { cell, map: HashMap::new() }
    }
    fn range(&self, lo: P2, hi: P2) -> impl Iterator<Item = (i32, i32)> {
        let c = self.cell;
        let (x0, x1) = ((lo[0] / c).floor() as i32, (hi[0] / c).floor() as i32);
        let (z0, z1) = ((lo[1] / c).floor() as i32, (hi[1] / c).floor() as i32);
        (x0..=x1).flat_map(move |x| (z0..=z1).map(move |z| (x, z)))
    }
    fn add(&mut self, i: u32, lo: P2, hi: P2) {
        for k in self.range(lo, hi).collect::<Vec<_>>() {
            self.map.entry(k).or_default().push(i);
        }
    }
    fn query(&self, lo: P2, hi: P2, out: &mut Vec<u32>) {
        out.clear();
        for k in self.range(lo, hi) {
            if let Some(l) = self.map.get(&k) {
                out.extend_from_slice(l);
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

fn floors_of(units: &[MarkedUnit], edits: &crate::edits::CollisionEdits) -> Vec<Floor> {
    let mut floors = Vec::new();
    for (unit, polys, ids, native) in units {
        for ((p, &id), &nat) in polys.iter().zip(ids).zip(native) {
            if edits.is_deleted(*unit, id) {
                continue;
            }
            let s = CollisionTag::decode(p.tag).surface_id;
            if s == bw::SURFACE_WATER || s == bw::SURFACE_INVISIBLE_WALL {
                continue;
            }
            let v: Vec<P3> = p.verts.iter().map(|v| v.map(|c| c as f64)).collect();
            for t in [[0, 1, 2], [0, 2, 3]] {
                if t[2] >= v.len() {
                    continue;
                }
                floors.extend(Floor::new(&[v[t[0]], v[t[1]], v[t[2]]], nat));
            }
        }
    }
    floors
}

/// A collision wall's footprint (its triangle's two farthest-apart points)
/// and height range; invisible walls included.
struct Wall {
    a: P2,
    b: P2,
    y: [f64; 2],
}

fn walls_of(units: &[MarkedUnit], edits: &crate::edits::CollisionEdits) -> Vec<Wall> {
    let mut walls = Vec::new();
    for (unit, polys, ids, _) in units {
        for (p, &id) in polys.iter().zip(ids) {
            if edits.is_deleted(*unit, id) || CollisionTag::decode(p.tag).surface_id == bw::SURFACE_WATER {
                continue;
            }
            let v: Vec<P3> = p.verts.iter().map(|v| v.map(|c| c as f64)).collect();
            for t in [[0, 1, 2], [0, 2, 3]] {
                if t[2] >= v.len() {
                    continue;
                }
                let q = [v[t[0]], v[t[1]], v[t[2]]];
                if upness(&q) >= MIN_UP {
                    continue;
                }
                let (mut best, mut a, mut b) = (-1.0, [0.0; 2], [0.0; 2]);
                for (i, j) in [(0, 1), (1, 2), (0, 2)] {
                    let d = (q[i][0] - q[j][0]).powi(2) + (q[i][2] - q[j][2]).powi(2);
                    if d > best {
                        (best, a, b) = (d, [q[i][0], q[i][2]], [q[j][0], q[j][2]]);
                    }
                }
                let y = [q.iter().map(|v| v[1]).fold(f64::MAX, f64::min), q.iter().map(|v| v[1]).fold(f64::MIN, f64::max)];
                walls.push(Wall { a, b, y });
            }
        }
    }
    walls
}

fn point_seg(p: P2, a: P2, b: P2) -> f64 {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dz * dz;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / l2).clamp(0.0, 1.0) } else { 0.0 };
    ((a[0] + dx * t - p[0]).powi(2) + (a[1] + dz * t - p[1]).powi(2)).sqrt()
}

/// Distance from a segment to a triangle's footprint (0 when they meet).
fn seg_tri(a: P2, b: P2, t: &[P2; 3]) -> f64 {
    if inside(t, a, 0.0) || inside(t, b, 0.0) {
        return 0.0;
    }
    let mut d = f64::MAX;
    for k in 0..3 {
        let (c, e) = (t[k], t[(k + 1) % 3]);
        let (d1, d2, d3, d4) = (cross(a, b, c), cross(a, b, e), cross(c, e, a), cross(c, e, b));
        if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
            return 0.0;
        }
        d = d.min(point_seg(a, c, e)).min(point_seg(b, c, e)).min(point_seg(c, a, b));
    }
    d
}

/// Whether a wall reaching at least half a metre above the triangle's lowest
/// point stands within NEAR of it.
fn walled(t: &[P3; 3], walls: &[Wall], grid: &Grid, near: &mut Vec<u32>) -> bool {
    let t2 = [[t[0][0], t[0][2]], [t[1][0], t[1][2]], [t[2][0], t[2][2]]];
    let lo = [t2.iter().map(|p| p[0]).fold(f64::MAX, f64::min) - NEAR, t2.iter().map(|p| p[1]).fold(f64::MAX, f64::min) - NEAR];
    let hi = [t2.iter().map(|p| p[0]).fold(f64::MIN, f64::max) + NEAR, t2.iter().map(|p| p[1]).fold(f64::MIN, f64::max) + NEAR];
    let (ylo, yhi) = (t.iter().map(|p| p[1]).fold(f64::MAX, f64::min), t.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
    grid.query(lo, hi, near);
    near.iter().any(|&i| {
        let w = &walls[i as usize];
        w.y[1] >= ylo + 0.5 && w.y[0] <= yhi + 0.5 && seg_tri(w.a, w.b, &t2) <= NEAR
    })
}

// --- Burnout's visible ground -----------------------------------------------

/// Collision surface for fill on ground with this material and model name.
fn surface_for(material: &str, model: &str) -> u8 {
    let m = material.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| m.contains(w));
    if has(&["grass", "lawn", "turf"]) {
        18
    } else if has(&["sand", "beach"]) {
        11
    } else if has(&["gravel", "shingle"]) {
        4
    } else if has(&["dirt", "mud", "earth", "soil", "rock", "cliff", "stone"]) {
        3
    } else if has(&["pav", "kerb", "curb", "concrete", "conc", "slab"]) {
        19
    } else if has(&["asphalt", "tarmac", "road"]) {
        1
    } else if model.starts_with("road_") || model.starts_with("junction_") {
        2
    } else {
        18
    }
}

fn fill_tag(surface: u8) -> u32 {
    // Material half: always-set bit, surface, no flags or lanes; group half:
    // always-set bit, no AI section.
    (((0x8000 | (surface as u32) << 4) & 0xFFFF) << 16) | 0xFFFF
}

/// Burnout's model ids (true for ground) and water material ids (see
/// DEFAULT_GROUND_MODELS).
#[derive(Default)]
struct Ids {
    /// 'g' ground, 's' structure, 'o' neither.
    models: HashMap<u64, char>,
    water: std::collections::HashSet<u64>,
}

/// What a named model is: 'g' ground, 's' structure, 'o' neither.
fn model_kind(name: &str) -> char {
    if GROUND.iter().any(|g| name.starts_with(g)) {
        'g'
    } else if NOT_SOLID.iter().any(|w| name.starts_with(w)) {
        'o'
    } else {
        's'
    }
}

fn model_ids() -> Result<Ids> {
    let path = std::env::var_os("BPX_GROUND_MODELS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_GROUND_MODELS));
    if !path.is_file() {
        eprintln!("hole fill: no {} (a modded map's rebuilt units will get no fill)", path.display());
        return Ok(Ids::default());
    }
    let mut ids = Ids::default();
    for line in std::fs::read_to_string(&path)?.lines() {
        let mut it = line.split_whitespace();
        let (Some(kind), Some(Ok(id))) = (it.next(), it.next().map(|h| u64::from_str_radix(h, 16))) else { continue };
        match kind {
            "g" | "s" | "o" => {
                ids.models.insert(id, kind.chars().next().unwrap());
            }
            "w" => {
                ids.water.insert(id);
            }
            _ => {}
        }
    }
    Ok(ids)
}

/// Whether a model (lower case name, or "#<hex id>" when unnamed) is ground:
/// Some(true/false), or None for an unnamed model stock Burnout doesn't have.
fn is_ground(model: &str, ids: &Ids) -> Option<bool> {
    kind_of(model, ids).map(|k| k == 'g')
}

/// 'g', 's' or 'o' (see model_kind) for a model name or "#<hex id>"; None for
/// an unnamed model stock Burnout doesn't have.
fn kind_of(model: &str, ids: &Ids) -> Option<char> {
    match model.strip_prefix('#') {
        Some(hex) => u64::from_str_radix(hex, 16).ok().and_then(|id| ids.models.get(&id).copied()),
        None => Some(model_kind(model)),
    }
}

/// A unit's walkable visible ground triangles and their fill surface, with
/// the model's index when it's a model stock Burnout doesn't have (to check
/// it's ground by how much collision covers it), and those models' labels.
fn unit_ground(path: &Path, ids: &Ids) -> Result<(Vec<([P3; 3], u8, Option<u32>)>, Vec<String>)> {
    let (tris, names) = unit_walkable(path, &|m| is_ground(m, ids) != Some(false), &ids.water, false)?;
    let new: Vec<bool> = names.iter().map(|n| is_ground(n.split(' ').next().unwrap_or(""), ids).is_none()).collect();
    Ok((tris.into_iter().map(|(p, s, m)| (p, s, new[m as usize].then_some(m))).collect(), names))
}

/// A unit's walkable visible triangles from models whose (lower case) name
/// `keep` accepts: each with its fill surface and model (index into the
/// returned names).
fn unit_walkable(path: &Path, keep: &dyn Fn(&str) -> bool, water: &std::collections::HashSet<u64>, steep: bool) -> Result<(Vec<([P3; 3], u8, u32)>, Vec<String>)> {
    let b = Bundle::open(path)?;
    let Some(list) = b.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else { return Ok((Vec::new(), Vec::new())) };
    let mut out = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut cache: HashMap<u64, Vec<bw::MeshGeometry>> = HashMap::new();
    for inst in bw::parse_instance_list(&Res::load(&b, list)?) {
        // Unnamed (a modded map's rebuilt units have no names): "#<id>".
        let model = b.name_of(inst.model).map(crate::export::short_name).unwrap_or_else(|| format!("#{:x}", inst.model)).to_ascii_lowercase();
        if !keep(&model) {
            continue;
        }
        let geo = cache.entry(inst.model).or_insert_with(|| {
            (|| -> Option<Vec<bw::MeshGeometry>> {
                let m = bw::parse_model(&Res::load(&b, b.find(inst.model)?).ok()?);
                bw::decode_renderable(&b, b.find(m.lod0()?)?).ok()
            })()
            .unwrap_or_default()
        });
        for g in geo.iter() {
            let material = g.material.and_then(|m| b.name_of(m)).map(crate::export::short_name).unwrap_or_default();
            if material.to_ascii_lowercase().contains("water") || g.material.is_some_and(|m| water.contains(&m)) {
                continue;
            }
            let surface = surface_for(&material, &model);
            let label = format!("{model} #{:x} {}", inst.model, if material.is_empty() { "?" } else { &material });
            let name = match names.iter().position(|n| *n == label) {
                Some(i) => i as u32,
                None => {
                    names.push(label);
                    names.len() as u32 - 1
                }
            };
            let pos: Vec<P3> = g.positions.iter().map(|&p| crate::gtavis::transform_point(&inst.transform, p).map(|c| c as f64)).collect();
            for t in g.indices.chunks_exact(3) {
                let p = [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]];
                if (!steep && upness(&p) < MIN_UP) || p.iter().all(|v| v[1] < SEA_Y) {
                    continue;
                }
                out.push((p, surface, name));
            }
        }
    }
    Ok((out, names))
}

/// Per model in a unit: (resource id, name if the bundle has one, walkable
/// visible area in m2).
fn unit_models(path: &Path, water: &mut Vec<u64>) -> Result<Vec<(u64, Option<String>, f64)>> {
    let b = Bundle::open(path)?;
    let Some(list) = b.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else { return Ok(Vec::new()) };
    let mut per: HashMap<u64, (Option<String>, f64)> = HashMap::new();
    let mut cache: HashMap<u64, Vec<bw::MeshGeometry>> = HashMap::new();
    for inst in bw::parse_instance_list(&Res::load(&b, list)?) {
        let geo = cache.entry(inst.model).or_insert_with(|| {
            (|| -> Option<Vec<bw::MeshGeometry>> {
                let m = bw::parse_model(&Res::load(&b, b.find(inst.model)?).ok()?);
                bw::decode_renderable(&b, b.find(m.lod0()?)?).ok()
            })()
            .unwrap_or_default()
        });
        let mut area = 0.0;
        for g in geo.iter() {
            if let Some(m) = g.material {
                if b.name_of(m).is_some_and(|n| n.to_ascii_lowercase().contains("water")) && !water.contains(&m) {
                    water.push(m);
                }
            }
            let pos: Vec<P3> = g.positions.iter().map(|&p| crate::gtavis::transform_point(&inst.transform, p).map(|c| c as f64)).collect();
            for t in g.indices.chunks_exact(3) {
                let p = [pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]];
                if upness(&p) >= MIN_UP {
                    area += cross([p[0][0], p[0][2]], [p[1][0], p[1][2]], [p[2][0], p[2][2]]).abs() * 0.5;
                }
            }
        }
        let e = per.entry(inst.model).or_insert((b.name_of(inst.model).map(crate::export::short_name), 0.0));
        e.1 += area;
    }
    Ok(per.into_iter().map(|(id, (n, a))| (id, n, a)).collect())
}

fn unit_bundles(game: &Path) -> Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(game)?
        .filter_map(|e| {
            let p = e.ok()?.path();
            let n = p.file_name()?.to_str()?.to_string();
            (n.starts_with("TRK_UNIT") && n.ends_with("_GR.BNDL")).then_some(p)
        })
        .collect();
    v.sort();
    Ok(v)
}

/// `bpx col-fill-ground-ids <stock burnout folder> <out.txt> [<modded folder>]`:
/// the resource ids of Burnout's ground models (ids are hashes of the names,
/// so the list holds no Burnout content). A modded map's rebuilt units have
/// no names but keep the ids, so the fill finds their ground by id. With a
/// modded folder, reports how much unnamed walkable surface it has that the
/// list does and doesn't know.
pub fn ground_ids(stock: &Path, out: &Path, modded: Option<&Path>) -> Result<()> {
    let all: Vec<(Vec<(u64, Option<String>, f64)>, Vec<u64>)> = unit_bundles(stock)?
        .par_iter()
        .map(|p| {
            let mut water = Vec::new();
            (unit_models(p, &mut water).unwrap_or_default(), water)
        })
        .collect();
    let mut water: Vec<u64> = all.iter().flat_map(|a| a.1.iter().copied()).collect();
    water.sort();
    water.dedup();
    let mut names: HashMap<u64, String> = HashMap::new();
    for (id, n, _) in all.into_iter().flat_map(|a| a.0) {
        if let Some(n) = n {
            names.insert(id, n.to_ascii_lowercase());
        }
    }
    let mut ground: Vec<u64> = names.iter().filter(|(_, n)| GROUND.iter().any(|g| n.starts_with(g))).map(|(&id, _)| id).collect();
    ground.sort();
    let mut all_ids: Vec<(u64, char)> = names.iter().map(|(&id, n)| (id, model_kind(n))).collect();
    all_ids.sort();
    let mut text = String::from("# Burnout Paradise's model resource ids (hex; hashes of the names): g = ground\n# (terrain, roads, junctions), s = structures (buildings and the like), o =\n# neither (plants, cables, lights); w = water material ids (bpx\n# col-fill-ground-ids). The hole fill tells the models and water in a modded\n# map's rebuilt units, which have no names, by these.\n");
    for (id, k) in &all_ids {
        text.push_str(&format!("{k} {id:x}\n"));
    }
    for id in &water {
        text.push_str(&format!("w {id:x}\n"));
    }
    std::fs::write(out, text)?;
    println!("{} named models in stock Burnout, {} of them ground; {} water materials -> {}", names.len(), ground.len(), water.len(), out.display());
    if let Some(modded) = modded {
        let all: Vec<Vec<(u64, Option<String>, f64)>> = unit_bundles(modded)?.par_iter().map(|p| unit_models(p, &mut Vec::new()).unwrap_or_default()).collect();
        let (mut named, mut known_ground, mut known_other, mut unknown) = (0.0, 0.0, 0.0, 0.0);
        let mut unknown_ids: Vec<(u64, f64)> = Vec::new();
        for (id, n, a) in all.into_iter().flatten() {
            if n.is_some() {
                named += a;
            } else if ground.binary_search(&id).is_ok() {
                known_ground += a;
            } else if names.contains_key(&id) {
                known_other += a;
            } else {
                unknown += a;
                unknown_ids.push((id, a));
            }
        }
        unknown_ids.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        println!(
            "modded map walkable visible area: named {:.0} m2; unnamed: ground by id {:.0} m2, other stock models {:.0} m2, models stock doesn't have {:.0} m2 ({} models; largest: {:?})",
            named, known_ground, known_other, unknown, unknown_ids.len(),
            unknown_ids.iter().take(8).map(|(id, a)| format!("{id:x} {a:.0} m2")).collect::<Vec<_>>()
        );
    }
    Ok(())
}

/// `bpx col-fill-survey <collision> <burnout folder> <fill.bin> <x> <z> <r>`:
/// within r of a Burnout point, every model's walkable visible surface, and
/// how much of it neither collision nor the fill covers (0.5 m samples).
pub fn survey(col: &Path, game: &Path, fill: &Path, x: f64, z: f64, r: f64) -> Result<()> {
    std::env::set_var(ENV, "none");
    let units = crate::colsrc::load_marked(col)?;
    let edits = crate::edits::CollisionEdits::load()?;
    let floors: Vec<Floor> = floors_of(&units, &edits).into_iter().filter(|f| f.hi[0] >= x - r && f.lo[0] <= x + r && f.hi[1] >= z - r && f.lo[1] <= z + r).collect();
    let fills: Vec<[P3; 3]> = read(fill)?
        .into_iter()
        .flat_map(|(_, t)| t.into_iter().map(|t| t.verts.map(|v| v.map(|c| c as f64))))
        .filter(|t| t.iter().any(|v| (v[0] - x).abs() < r + 50.0 && (v[2] - z).abs() < r + 50.0))
        .collect();
    let mut bundles: Vec<PathBuf> = std::fs::read_dir(game)?
        .filter_map(|e| {
            let p = e.ok()?.path();
            let n = p.file_name()?.to_str()?.to_string();
            (n.starts_with("TRK_UNIT") && n.ends_with("_GR.BNDL")).then_some(p)
        })
        .collect();
    bundles.sort();
    // model -> (samples, uncovered, filled, an uncovered point)
    let found: Vec<Vec<(String, usize, usize, usize, Option<P3>)>> = bundles
        .par_iter()
        .map(|path| -> Vec<(String, usize, usize, usize, Option<P3>)> {
            let water = model_ids().map(|i| i.water).unwrap_or_default();
            let Ok((tris, names)) = unit_walkable(path, &|_| true, &water, false) else { return Vec::new() };
            let mut per: HashMap<u32, (usize, usize, usize, Option<P3>)> = HashMap::new();
            for (p, _, m) in &tris {
                if !p.iter().any(|v| ((v[0] - x).powi(2) + (v[2] - z).powi(2)).sqrt() < r + 30.0) {
                    continue;
                }
                let Some(pv) = plane(p) else { continue };
                let t = [[p[0][0], p[0][2]], [p[1][0], p[1][2]], [p[2][0], p[2][2]]];
                let (x0, x1) = (t.iter().map(|q| q[0]).fold(f64::MAX, f64::min), t.iter().map(|q| q[0]).fold(f64::MIN, f64::max));
                let (z0, z1) = (t.iter().map(|q| q[1]).fold(f64::MAX, f64::min), t.iter().map(|q| q[1]).fold(f64::MIN, f64::max));
                let mut sx = (x0 / 0.5).ceil() * 0.5;
                while sx <= x1 {
                    let mut sz = (z0 / 0.5).ceil() * 0.5;
                    while sz <= z1 {
                        let q = [sx, sz];
                        if ((sx - x).powi(2) + (sz - z).powi(2)).sqrt() <= r && inside(&t, q, 0.0) {
                            let y = at(pv, q);
                            let col = floors.iter().any(|f| inside(&f.t, q, 0.0) && { let h = at(f.plane, q); h >= y - f.below() && h <= y + COVER_ABOVE });
                            let fil = !col && fills.iter().any(|f| {
                                let t2 = [[f[0][0], f[0][2]], [f[1][0], f[1][2]], [f[2][0], f[2][2]]];
                                inside(&t2, q, 0.0) && plane(f).is_some_and(|pl| { let h = at(pl, q); h >= y - COVER_BELOW && h <= y + COVER_ABOVE })
                            });
                            let e = per.entry(*m).or_insert((0, 0, 0, None));
                            e.0 += 1;
                            if fil {
                                e.2 += 1;
                            } else if !col {
                                e.1 += 1;
                                e.3.get_or_insert([sx, y, sz]);
                            }
                        }
                        sz += 0.5;
                    }
                    sx += 0.5;
                }
            }
            per.into_iter().map(|(m, (n, u, f, ex))| (names[m as usize].clone(), n, u, f, ex)).collect()
        })
        .collect();
    let mut rows: Vec<(String, usize, usize, usize, Option<P3>)> = found.into_iter().flatten().collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2));
    println!("walkable visible surfaces within {r} m of Burnout ({x}, {z}), 0.5 m samples (m2 = samples / 4):");
    for (name, n, u, f, ex) in rows {
        let ex = ex.map_or(String::new(), |p| format!("  e.g. uncovered at Burnout ({:.1}, {:.1}, {:.1})", p[0], p[1], p[2]));
        println!("  {name:<40} {:>8.0} m2, uncovered {:>7.0} m2, filled {:>7.0} m2{ex}", n as f64 / 4.0, u as f64 / 4.0, f as f64 / 4.0);
    }
    Ok(())
}

// --- Cliffs, structures, mountains (bpx col-gen) ---------------------------

/// The parts of a triangle outside the keep-out space of Burnout's own
/// floors, cut exactly: in the triangle's own plane, each nearby floor's
/// space (its footprint grown by KEEP_OUT, between KEEP_BELOW under and
/// COVER_BELOW_OWN over its plane) is a convex polygon; the rest is
/// triangulated. Returns the area dropped (m2).
fn keep_clear(t: [P3; 3], floors: &[Floor], grid: &Grid, near: &mut Vec<u32>, out: &mut Vec<[P3; 3]>) -> f64 {
    let lo = [t.iter().map(|p| p[0]).fold(f64::MAX, f64::min) - KEEP_OUT, t.iter().map(|p| p[2]).fold(f64::MAX, f64::min) - KEEP_OUT];
    let hi = [t.iter().map(|p| p[0]).fold(f64::MIN, f64::max) + KEEP_OUT, t.iter().map(|p| p[2]).fold(f64::MIN, f64::max) + KEEP_OUT];
    let (ylo, yhi) = (t.iter().map(|p| p[1]).fold(f64::MAX, f64::min), t.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
    grid.query(lo, hi, near);
    let cand: Vec<u32> = near
        .iter()
        .copied()
        .filter(|&i| {
            let f = &floors[i as usize];
            f.native && f.lo[0] <= hi[0] && f.hi[0] >= lo[0] && f.lo[1] <= hi[1] && f.hi[1] >= lo[1] && f.y[0] - KEEP_BELOW <= yhi && f.y[1] + COVER_BELOW_OWN >= ylo
        })
        .collect();
    if cand.is_empty() {
        out.push(t);
        return 0.0;
    }
    // Orthonormal frame in the triangle's plane: p = a + s e1 + r e2.
    let sub3 = |a: P3, b: P3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot3 = |a: P3, b: P3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let a = t[0];
    let (ab, ac) = (sub3(t[1], a), sub3(t[2], a));
    let n = [ab[1] * ac[2] - ab[2] * ac[1], ab[2] * ac[0] - ab[0] * ac[2], ab[0] * ac[1] - ab[1] * ac[0]];
    let (ln, lab) = (dot3(n, n).sqrt(), dot3(ab, ab).sqrt());
    if ln < 1e-9 || lab < 1e-9 {
        return 0.0;
    }
    let e1 = ab.map(|c| c / lab);
    let nn = n.map(|c| c / ln);
    let e2 = [nn[1] * e1[2] - nn[2] * e1[1], nn[2] * e1[0] - nn[0] * e1[2], nn[0] * e1[1] - nn[1] * e1[0]];
    let to2 = |p: P3| -> P2 { let d = sub3(p, a); [dot3(d, e1), dot3(d, e2)] };
    let tri2 = ccw(t.iter().map(|&p| to2(p)).collect());
    // A function g(x, y, z) = gx x + gy y + gz z + g0, as an affine function of (s, r).
    let affine = |g: [f64; 4]| -> [f64; 3] {
        [g[0] * e1[0] + g[1] * e1[1] + g[2] * e1[2], g[0] * e2[0] + g[1] * e2[1] + g[2] * e2[2], g[0] * a[0] + g[1] * a[1] + g[2] * a[2] + g[3]]
    };
    let mut clips: Vec<Vec<P2>> = Vec::new();
    for &i in &cand {
        let f = &floors[i as usize];
        let mut k = tri2.clone();
        // Inside the grown footprint: left of each edge of the convex hull.
        let g = grow(&f.t, KEEP_OUT);
        for j in 0..g.len() {
            if k.len() < 3 {
                break;
            }
            let (p, q) = (g[j], g[(j + 1) % g.len()]);
            let (dx, dz) = (q[0] - p[0], q[1] - p[1]);
            // -cross(p, q, (x, z)) <= 0, with (x, z) the world x and z.
            k = clip(&k, affine([dz, 0.0, -dx, -(dz * p[0] - dx * p[1])]));
        }
        // Height over the floor's plane (y - a x - b z - c) within the band.
        let (pa, pb, pc) = (f.plane[0], f.plane[1], f.plane[2]);
        k = clip(&k, affine([pa, -1.0, pb, pc - KEEP_BELOW]));
        k = clip(&k, affine([-pa, 1.0, -pb, -pc - COVER_BELOW_OWN]));
        if k.len() >= 3 && area2(&k).abs() > 1e-6 {
            clips.push(ccw(k));
        }
    }
    let full_area = area2(&tri2).abs() * 0.5;
    if clips.is_empty() {
        out.push(t);
        return 0.0;
    }
    let shapes = tri2
        .overlay_with_fixed_scale(&clips, OverlayRule::Difference, FillRule::NonZero, SNAP_SCALE)
        .unwrap_or_else(|_| tri2.overlay(&clips, OverlayRule::Difference, FillRule::NonZero));
    let to3 = |q: P2| -> P3 { [a[0] + q[0] * e1[0] + q[1] * e2[0], a[1] + q[0] * e1[1] + q[1] * e2[1], a[2] + q[0] * e1[2] + q[1] * e2[2]] };
    let mut kept = 0.0;
    for shape in shapes {
        let mut flat: Vec<f64> = Vec::new();
        let mut holes: Vec<usize> = Vec::new();
        for (ci, c) in shape.iter().enumerate() {
            if ci > 0 {
                holes.push(flat.len() / 2);
            }
            c.iter().for_each(|p| flat.extend_from_slice(p));
        }
        let Ok(idx) = earcutr::earcut(&flat, &holes, 2) else { continue };
        let pts: Vec<P2> = flat.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
        for tr in idx.chunks_exact(3) {
            let (p, q, r) = (pts[tr[0]], pts[tr[1]], pts[tr[2]]);
            let area = cross(p, q, r).abs() * 0.5;
            let longest = [(p, q), (q, r), (r, p)].iter().map(|(u, v)| ((v[0] - u[0]).powi(2) + (v[1] - u[1]).powi(2)).sqrt()).fold(0.0, f64::max);
            if area / longest.max(1e-9) < MIN_WIDTH {
                continue;
            }
            kept += area;
            // Keep the visible model's winding (its outside): collision
            // made from it stays one-sided, facing the way it's drawn.
            let piece = [to3(p), to3(q), to3(r)];
            let (u, v) = (sub3(piece[1], piece[0]), sub3(piece[2], piece[0]));
            let pn = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            out.push(if dot3(pn, n) < 0.0 { [piece[0], piece[2], piece[1]] } else { piece });
        }
    }
    (full_area - kept).max(0.0)
}

/// A unit's solid visible geometry for `bpx col-gen`: steep ground (cliffs)
/// and every triangle of its structures, each with a fill surface and kind.
fn unit_solids(path: &Path, ids: &Ids) -> Result<Vec<([P3; 3], u8, u8)>> {
    let (tris, names) = unit_walkable(path, &|m| matches!(kind_of(m, ids), Some('g') | Some('s')), &ids.water, true)?;
    let kinds: Vec<Option<char>> = names.iter().map(|n| kind_of(n.split(' ').next().unwrap_or(""), ids)).collect();
    let mut out = Vec::new();
    for (p, surface, m) in tris {
        match kinds[m as usize] {
            Some('g') if upness(&p) < MIN_UP => out.push((p, 3, KIND_CLIFF)),
            Some('s') => {
                let material_surface = if surface == 18 { 2 } else { surface };
                out.push((p, material_surface, KIND_STRUCTURE));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The mountains around the city: Burnout's backdrop scenery, each triangle
/// with the unit nearest it (by zone). Units' zones reach over the mountains,
/// but their own models don't, so what to keep is decided by `mountain_pieces`.
fn mountains(game: &Path) -> Result<Vec<(u32, [P3; 3])>> {
    let zones: Vec<(u32, Vec<[f32; 2]>)> = crate::gtavis::zone_polygons(game).into_iter().filter(|(_, p)| p.len() >= 3).collect();
    let centres: Vec<(u32, [f64; 2])> = zones
        .iter()
        .map(|(u, p)| (*u, [p.iter().map(|q| q[0] as f64).sum::<f64>() / p.len() as f64, p.iter().map(|q| q[1] as f64).sum::<f64>() / p.len() as f64]))
        .collect();
    let mut out = Vec::new();
    let scenery = crate::gtavis::backdrop_scenery(game)?;
    if std::env::var_os("BPX_MOUNTAIN_REPORT").is_some() {
        for (name, tris) in &scenery {
            let pts: Vec<[f32; 3]> = tris.iter().flatten().copied().collect();
            let lo = pts.iter().fold([f32::MAX; 3], |a, p| [a[0].min(p[0]), a[1].min(p[1]), a[2].min(p[2])]);
            let hi = pts.iter().fold([f32::MIN; 3], |a, p| [a[0].max(p[0]), a[1].max(p[1]), a[2].max(p[2])]);
            eprintln!("  backdrop {name}: {} triangles, GTA x {:.0}..{:.0} y {:.0}..{:.0} z {:.0}..{:.0}", tris.len(), lo[0], hi[0], -hi[2], -lo[2], lo[1] + 9.9, hi[1] + 9.9);
        }
    }
    for (name, tris) in scenery.iter() {
        // Stand-ins for units' own models (Big Surf Island, distant buildings)
        // and specks, not scenery.
        let lower = name.to_ascii_lowercase();
        if ["island", "temp_buildings", "airport"].iter().any(|w| lower.contains(w)) {
            continue;
        }
        for &t in tris {
            let p = t.map(|v| v.map(|c| c as f64));
            if p.iter().all(|v| v[1] < SEA_Y) {
                continue;
            }
            let c = [(p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][2] + p[1][2] + p[2][2]) / 3.0];
            let unit = centres.iter().min_by(|a, b| ((a.1[0] - c[0]).hypot(a.1[1] - c[1])).total_cmp(&(b.1[0] - c[0]).hypot(b.1[1] - c[1]))).map_or(0, |u| u.0);
            out.push((unit, p));
        }
    }
    eprintln!("mountains: {} backdrop scenery pieces, {} triangles", scenery.len(), out.len());
    Ok(out)
}

/// The part of a mountain triangle with none of the units' own ground under
/// or over it (at any height): there, the detailed ground is the ground. A
/// walkable triangle is cut exactly by the ground's footprints; a steep one
/// is kept whole unless ground lies under its centre.
fn mountain_pieces(t: [P3; 3], ground: &[[P2; 3]], gidx: &Grid, cand: &mut Vec<u32>, out: &mut Vec<[P3; 3]>) {
    let t2 = [[t[0][0], t[0][2]], [t[1][0], t[1][2]], [t[2][0], t[2][2]]];
    let lo = [t2.iter().map(|p| p[0]).fold(f64::MAX, f64::min), t2.iter().map(|p| p[1]).fold(f64::MAX, f64::min)];
    let hi = [t2.iter().map(|p| p[0]).fold(f64::MIN, f64::max), t2.iter().map(|p| p[1]).fold(f64::MIN, f64::max)];
    let pl = if upness(&t) >= MIN_UP { plane(&t) } else { None };
    let Some(pl) = pl else {
        let c = [(t2[0][0] + t2[1][0] + t2[2][0]) / 3.0, (t2[0][1] + t2[1][1] + t2[2][1]) / 3.0];
        gidx.query(c, c, cand);
        if !cand.iter().any(|&i| inside(&ground[i as usize], c, 0.0)) {
            out.push(t);
        }
        return;
    };
    gidx.query(lo, hi, cand);
    let tri = ccw(t2.to_vec());
    let clips: Vec<Vec<P2>> = cand
        .iter()
        .filter_map(|&i| {
            let k = clip_convex(ground[i as usize].to_vec(), &tri);
            (k.len() >= 3 && area2(&k).abs() > 1e-6).then(|| ccw(k))
        })
        .collect();
    if clips.is_empty() {
        out.push(t);
        return;
    }
    let shapes = tri
        .overlay_with_fixed_scale(&clips, OverlayRule::Difference, FillRule::NonZero, SNAP_SCALE)
        .unwrap_or_else(|_| tri.overlay(&clips, OverlayRule::Difference, FillRule::NonZero));
    for shape in shapes {
        let mut flat: Vec<f64> = Vec::new();
        let mut holes: Vec<usize> = Vec::new();
        for (ci, c) in shape.iter().enumerate() {
            if ci > 0 {
                holes.push(flat.len() / 2);
            }
            c.iter().for_each(|p| flat.extend_from_slice(p));
        }
        let Ok(idx) = earcutr::earcut(&flat, &holes, 2) else { continue };
        let pts: Vec<P2> = flat.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
        for tr in idx.chunks_exact(3) {
            let (a, b, c) = (pts[tr[0]], pts[tr[1]], pts[tr[2]]);
            let area = cross(a, b, c).abs() * 0.5;
            let longest = [(a, b), (b, c), (c, a)].iter().map(|(u, v)| ((v[0] - u[0]).powi(2) + (v[1] - u[1]).powi(2)).sqrt()).fold(0.0, f64::max);
            if area / longest.max(1e-9) < MIN_WIDTH {
                continue;
            }
            out.push(face_up([a, b, c].map(|q| [q[0], at(pl, q), q[1]])));
        }
    }
}

// --- Filling ----------------------------------------------------------------

#[derive(Default)]
struct Stats {
    ground: usize,
    untouched: usize,
    cut: usize,
    covered: usize,
    tris: usize,
    raised: usize,
    slivers: usize,
    sliver_area: f64,
    failed: usize,
    area: f64,
    raised_area: f64,
    open: usize,
    /// Area by how far the fill rises over Burnout's own floor beside it:
    /// [walled, open] x RISE_BUCKETS.
    rise_area: [[f64; 5]; 2],
    /// col-gen: cliff, structure and mountain triangles (after keep_clear),
    /// and the area cut away near Burnout's own floors (m2).
    solid: [usize; 3],
    kept_out: f64,
}

const RISE_BUCKETS: [f64; 5] = [RISE, 0.25, 0.5, 1.0, 2.0];

/// The fill for one ground triangle: the part no floor covers.
#[allow(clippy::too_many_arguments)]
fn fill_ground(v: &[P3; 3], surface: u8, floors: &[Floor], grid: &Grid, walls: (&[Wall], &Grid), cand: &mut Vec<u32>, near: &mut Vec<u32>, stats: &mut Stats, out: &mut Vec<FillTri>) {
    let Some(pv) = plane(v) else { return };
    let vt = ccw(v.iter().map(|p| [p[0], p[2]]).collect());
    let lo = [vt.iter().map(|p| p[0]).fold(f64::MAX, f64::min), vt.iter().map(|p| p[1]).fold(f64::MAX, f64::min)];
    let hi = [vt.iter().map(|p| p[0]).fold(f64::MIN, f64::max), vt.iter().map(|p| p[1]).fold(f64::MIN, f64::max)];
    let (ylo, yhi) = (v.iter().map(|p| p[1]).fold(f64::MAX, f64::min), v.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
    grid.query(lo, hi, cand);
    cand.retain(|&i| {
        let f = &floors[i as usize];
        f.lo[0] <= hi[0] && f.hi[0] >= lo[0] && f.lo[1] <= hi[1] && f.hi[1] >= lo[1] && f.y[0] <= yhi + COVER_ABOVE && f.y[1] >= ylo - f.below()
    });

    // Covered parts: each floor's footprint over the ground, where it lies
    // from below() under to COVER_ABOVE over the ground's plane.
    let mut clips: Vec<Vec<P2>> = Vec::new();
    for &i in cand.iter() {
        let f = &floors[i as usize];
        let mut k = clip_convex(f.t.to_vec(), &vt);
        let d = [pv[0] - f.plane[0], pv[1] - f.plane[1], pv[2] - f.plane[2]];
        k = clip(&k, [d[0], d[1], d[2] - f.below()]);
        k = clip(&k, [-d[0], -d[1], -d[2] - COVER_ABOVE]);
        if k.len() >= 3 && area2(&k).abs() > 1e-8 {
            clips.push(ccw(k));
        }
    }
    let shapes: Vec<Vec<Vec<P2>>> = if clips.is_empty() {
        stats.untouched += 1;
        vec![vec![vt.clone()]]
    } else {
        let s = vt
            .overlay_with_fixed_scale(&clips, OverlayRule::Difference, FillRule::NonZero, SNAP_SCALE)
            .unwrap_or_else(|_| vt.overlay(&clips, OverlayRule::Difference, FillRule::NonZero));
        if s.is_empty() {
            stats.covered += 1;
            return;
        }
        stats.cut += 1;
        s
    };

    // Heights: the ground's, but tucked under any floor within COVER_ABOVE
    // that the point lies on (the fill's edge against it).
    let height = |p: P2| -> f64 {
        let mut y = at(pv, p);
        for &i in cand.iter() {
            let f = &floors[i as usize];
            if inside(&f.t, p, 1.0 / SNAP_SCALE) {
                let hf = at(f.plane, p);
                if (y - hf).abs() <= COVER_ABOVE {
                    y = y.min(hf - TUCK);
                }
            }
        }
        y
    };
    let tag = fill_tag(surface);
    for shape in shapes {
        let mut flat: Vec<f64> = Vec::new();
        let mut holes: Vec<usize> = Vec::new();
        for (ci, c) in shape.iter().enumerate() {
            if ci > 0 {
                holes.push(flat.len() / 2);
            }
            c.iter().for_each(|p| flat.extend_from_slice(p));
        }
        let Ok(idx) = earcutr::earcut(&flat, &holes, 2) else {
            stats.failed += 1;
            continue;
        };
        let pts: Vec<P2> = flat.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
        let ys: Vec<f64> = pts.iter().map(|&p| height(p)).collect();
        for t in idx.chunks_exact(3) {
            let (a, b, c) = (pts[t[0]], pts[t[1]], pts[t[2]]);
            let area = cross(a, b, c).abs() * 0.5;
            let longest = [(a, b), (b, c), (c, a)].iter().map(|(p, q)| ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt()).fold(0.0, f64::max);
            if area / longest.max(1e-9) < MIN_WIDTH {
                stats.slivers += 1;
                stats.sliver_area += area;
                continue;
            }
            let p3 = face_up([[a[0], ys[t[0]], a[1]], [b[0], ys[t[1]], b[1]], [c[0], ys[t[2]], c[1]]]);
            let rise = raised(&p3, floors, grid, near);
            let kind = match rise {
                None => KIND_FILL,
                Some(_) if walled(&p3, walls.0, walls.1, near) => KIND_RAISED,
                Some(_) => KIND_OPEN,
            };
            stats.tris += 1;
            stats.area += area;
            if let Some(r) = rise {
                stats.raised += 1;
                stats.raised_area += area;
                stats.open += (kind == KIND_OPEN) as usize;
                let b = RISE_BUCKETS.iter().rposition(|&x| r > x).unwrap_or(0);
                stats.rise_area[(kind == KIND_OPEN) as usize][b] += area;
            }
            out.push(FillTri { tag, kind, verts: p3.map(|p| p.map(|c| c as f32)) });
        }
    }
}

/// How far a fill triangle rises above Burnout's own floor within NEAR of it
/// (that floor's plane carried on), when more than RISE and short of LEVEL.
fn raised(t: &[P3; 3], floors: &[Floor], grid: &Grid, near: &mut Vec<u32>) -> Option<f64> {
    let pt = plane(t)?;
    let t2 = ccw(t.iter().map(|p| [p[0], p[2]]).collect());
    let lo = [t2.iter().map(|p| p[0]).fold(f64::MAX, f64::min) - NEAR, t2.iter().map(|p| p[1]).fold(f64::MAX, f64::min) - NEAR];
    let hi = [t2.iter().map(|p| p[0]).fold(f64::MIN, f64::max) + NEAR, t2.iter().map(|p| p[1]).fold(f64::MIN, f64::max) + NEAR];
    let (ylo, yhi) = (t.iter().map(|p| p[1]).fold(f64::MAX, f64::min), t.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
    grid.query(lo, hi, near);
    let mut rise: Option<f64> = None;
    for &i in near.iter() {
        let f = &floors[i as usize];
        if !f.native || f.lo[0] > hi[0] || f.hi[0] < lo[0] || f.lo[1] > hi[1] || f.hi[1] < lo[1] {
            continue;
        }
        if f.y[1] < ylo - LEVEL - 2.0 || f.y[0] > yhi + 2.0 {
            continue;
        }
        let r = clip_convex(t2.clone(), &grow(&f.t, NEAR));
        if r.len() < 3 {
            continue;
        }
        let g: Vec<f64> = r.iter().map(|&p| at(pt, p) - at(f.plane, p)).collect();
        let (gmin, gmax) = (g.iter().cloned().fold(f64::MAX, f64::min), g.iter().cloned().fold(f64::MIN, f64::max));
        if gmax > RISE && gmin < LEVEL {
            let r = gmax.min(LEVEL);
            rise = Some(rise.map_or(r, |x: f64| x.max(r)));
        }
    }
    rise
}

/// `bpx col-fill <collision> <burnout folder> <out.bin> [--png <overview.png>]`:
/// ground fill over the collision as it is. `bpx col-gen` (`full`): all
/// collision beyond the map's own, generated from Burnout's visible models,
/// replacing the added pieces: ground, cliffs, structures, mountains.
pub fn generate(col: &Path, game: &Path, out: &Path, png: Option<&Path>, full: bool) -> Result<()> {
    std::env::set_var(ENV, "none");
    let mut units = crate::colsrc::load_marked(col)?;
    // BPX_FILL_BASE=native: fill against the map's own collision only, as if
    // there were no added pieces (to compare).
    if full || std::env::var("BPX_FILL_BASE").as_deref() == Ok("native") {
        for (_, polys, ids, native) in units.iter_mut() {
            let mut k = native.iter();
            polys.retain(|_| *k.next().unwrap());
            let mut k = native.iter();
            ids.retain(|_| *k.next().unwrap());
            native.retain(|&n| n);
        }
        eprintln!("hole fill: against the map's own collision only (BPX_FILL_BASE=native)");
    }
    let edits = crate::edits::CollisionEdits::load()?;
    let floors = floors_of(&units, &edits);
    let mut grid = Grid::new(16.0);
    for (i, f) in floors.iter().enumerate() {
        grid.add(i as u32, f.lo, f.hi);
    }
    let walls = walls_of(&units, &edits);
    let mut wall_grid = Grid::new(16.0);
    for (i, w) in walls.iter().enumerate() {
        wall_grid.add(i as u32, [w.a[0].min(w.b[0]), w.a[1].min(w.b[1])], [w.a[0].max(w.b[0]), w.a[1].max(w.b[1])]);
    }
    eprintln!("hole fill: {} collision floor triangles ({} Burnout's own)", floors.len(), floors.iter().filter(|f| f.native).count());

    let mut bundles: Vec<(u32, PathBuf)> = std::fs::read_dir(game)?
        .filter_map(|e| {
            let e = e.ok()?;
            let n = e.file_name().into_string().ok()?;
            Some((n.strip_prefix("TRK_UNIT")?.strip_suffix("_GR.BNDL")?.parse().ok()?, e.path()))
        })
        .collect();
    bundles.sort();
    // BPX_FILL_UNITS=a,b,...: only those units' ground (for testing).
    if let Some(only) = std::env::var("BPX_FILL_UNITS").ok() {
        let only: Vec<u32> = only.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        bundles.retain(|(u, _)| only.contains(u));
    }
    let started = std::time::Instant::now();
    let ids = model_ids()?;

    type Done = (u32, Vec<FillTri>, Stats, Vec<[P3; 3]>, Vec<(String, f64, f64, bool)>);
    let results: Vec<Result<Done>> = bundles
        .par_iter()
        .map(|(unit, path)| {
            let (ground, names) = unit_ground(path, &ids).with_context(|| format!("reading {}", path.display()))?;
            let (mut cand, mut near) = (Vec::new(), Vec::new());
            let mut stats = Stats { ground: ground.len(), ..Default::default() };
            let mut tris = Vec::new();
            // Per model stock Burnout doesn't have: walkable area, fill area
            // and the fill's ranges in `tris`.
            let mut new: HashMap<u32, (f64, f64, Vec<std::ops::Range<usize>>, Option<P3>)> = HashMap::new();
            for (v, s, m) in &ground {
                let start = tris.len();
                fill_ground(v, *s, &floors, &grid, (&walls, &wall_grid), &mut cand, &mut near, &mut stats, &mut tris);
                if let Some(m) = m {
                    let e = new.entry(*m).or_default();
                    e.0 += cross([v[0][0], v[0][2]], [v[1][0], v[1][2]], [v[2][0], v[2][2]]).abs() * 0.5;
                    e.1 += tris[start..].iter().map(|t| cross([t.verts[0][0] as f64, t.verts[0][2] as f64], [t.verts[1][0] as f64, t.verts[1][2] as f64], [t.verts[2][0] as f64, t.verts[2][2] as f64]).abs() * 0.5).sum::<f64>();
                    e.2.push(start..tris.len());
                    e.3.get_or_insert([(v[0][0] + v[1][0] + v[2][0]) / 3.0, (v[0][1] + v[1][1] + v[2][1]) / 3.0, (v[0][2] + v[1][2] + v[2][2]) / 3.0]);
                }
            }
            // A new model mostly without collision isn't ground: drop its fill.
            let mut report = Vec::new();
            let mut drop = vec![false; tris.len()];
            for (m, (area, filled, ranges, at)) in new {
                let keep = area <= 0.0 || 1.0 - filled / area >= NEW_MIN_COVERED;
                if !keep {
                    ranges.into_iter().flatten().for_each(|i| drop[i] = true);
                }
                let at = at.map_or(String::new(), |p| format!(" at GTA ({:.0}, {:.0}, {:.0})", p[0], -p[2], p[1] + 9.9));
                report.push((format!("unit {unit} {}{at}", names[m as usize]), area, filled, keep));
            }
            let mut k = drop.iter();
            tris.retain(|_| !*k.next().unwrap());
            if full {
                let mut pieces = Vec::new();
                for (p, surface, kind) in unit_solids(path, &ids)? {
                    pieces.clear();
                    stats.kept_out += keep_clear(p, &floors, &grid, &mut near, &mut pieces);
                    let tag = fill_tag(surface);
                    for q in &pieces {
                        stats.solid[(kind - KIND_CLIFF) as usize] += 1;
                        tris.push(FillTri { tag, kind, verts: q.map(|v| v.map(|c| c as f32)) });
                    }
                }
            }
            Ok((*unit, tris, stats, ground.into_iter().map(|g| g.0).collect(), report))
        })
        .collect();

    let mut fill: Vec<(u32, Vec<FillTri>)> = Vec::new();
    let mut all_ground: Vec<[P3; 3]> = Vec::new();
    let mut new_models: Vec<(String, f64, f64, bool)> = Vec::new();
    let mut t = Stats::default();
    for r in results {
        let (unit, tris, s, ground, report) = r?;
        new_models.extend(report);
        t.ground += s.ground;
        t.untouched += s.untouched;
        t.cut += s.cut;
        t.covered += s.covered;
        t.tris += s.tris;
        t.raised += s.raised;
        t.slivers += s.slivers;
        t.sliver_area += s.sliver_area;
        t.failed += s.failed;
        t.area += s.area;
        t.raised_area += s.raised_area;
        t.open += s.open;
        for k in 0..3 {
            t.solid[k] += s.solid[k];
        }
        t.kept_out += s.kept_out;
        for k in 0..2 {
            for b in 0..5 {
                t.rise_area[k][b] += s.rise_area[k][b];
            }
        }
        all_ground.extend(ground);
        if !tris.is_empty() {
            fill.push((unit, tris));
        }
    }
    if full {
        let (mut near, mut pieces, mut parts) = (Vec::new(), Vec::new(), Vec::new());
        let mut by_unit: HashMap<u32, Vec<FillTri>> = HashMap::new();
        // The units' own walkable ground, by footprint.
        let ground2: Vec<[P2; 3]> = all_ground.iter().map(|g| [[g[0][0], g[0][2]], [g[1][0], g[1][2]], [g[2][0], g[2][2]]]).collect();
        let mut gidx = Grid::new(16.0);
        for (i, g) in ground2.iter().enumerate() {
            let lo = [g.iter().map(|p| p[0]).fold(f64::MAX, f64::min), g.iter().map(|p| p[1]).fold(f64::MAX, f64::min)];
            let hi = [g.iter().map(|p| p[0]).fold(f64::MIN, f64::max), g.iter().map(|p| p[1]).fold(f64::MIN, f64::max)];
            gidx.add(i as u32, lo, hi);
        }
        for (unit, p) in mountains(game)? {
            parts.clear();
            mountain_pieces(p, &ground2, &gidx, &mut near, &mut parts);
            for &part in &parts {
                pieces.clear();
                t.kept_out += keep_clear(part, &floors, &grid, &mut near, &mut pieces);
                let tag = fill_tag(if upness(&part) >= MIN_UP { 18 } else { 3 });
                for q in &pieces {
                    t.solid[2] += 1;
                    by_unit.entry(unit).or_default().push(FillTri { tag, kind: KIND_MOUNTAIN, verts: q.map(|v| v.map(|c| c as f32)) });
                }
            }
        }
        for (unit, tris) in by_unit {
            match fill.iter_mut().find(|f| f.0 == unit) {
                Some(f) => f.1.extend(tris),
                None => fill.push((unit, tris)),
            }
        }
        fill.sort_by_key(|f| f.0);
        println!(
            "collision generated: {} cliff, {} structure, {} mountain triangles; {:.0} m2 cut away where a vehicle on Burnout's own floors can be",
            t.solid[0], t.solid[1], t.solid[2], t.kept_out
        );
    }
    write(out, &fill)?;
    eprintln!("hole fill: done in {:.0} s", started.elapsed().as_secs_f64());
    for (k, what) in ["walled off", "open (no wall within NEAR)"].iter().enumerate() {
        let parts: Vec<String> = (0..5)
            .map(|b| {
                let top = RISE_BUCKETS.get(b + 1).map_or(format!("{LEVEL}"), |x| format!("{x}"));
                format!("{}-{top} m: {:.0} m2", RISE_BUCKETS[b], t.rise_area[k][b])
            })
            .collect();
        println!("hole fill: rising beside Burnout's own floors, {what}: {}", parts.join(", "));
    }
    println!("hole fill: {} open triangles to review", t.open);
    if !new_models.is_empty() {
        new_models.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let (kept, dropped): (Vec<_>, Vec<_>) = new_models.iter().partition(|m| m.3);
        println!(
            "hole fill: models stock Burnout doesn't have: {} kept as ground ({:.0} m2 filled), {} dropped as mostly without collision ({:.0} m2 not filled)",
            kept.len(), kept.iter().map(|m| m.2).sum::<f64>(), dropped.len(), dropped.iter().map(|m| m.2).sum::<f64>()
        );
        if std::env::var_os("BPX_FILL_NEW_REPORT").is_some() {
            for (name, area, filled, keep) in &new_models {
                eprintln!("  {name}: walkable {area:.0} m2, uncovered {filled:.0} m2 ({:.0}%) -> {}", 100.0 * filled / area.max(1e-9), if *keep { "ground" } else { "dropped" });
            }
        }
    }
    // BPX_FILL_PROBE=x,z;x,z...: what lies at those Burnout points.
    if let Ok(probe) = std::env::var("BPX_FILL_PROBE") {
        for q in probe.split(';') {
            let c: Vec<f64> = q.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            if c.len() != 2 {
                continue;
            }
            let p = [c[0], c[1]];
            eprintln!("probe {p:?}:");
            for g in all_ground.iter().filter(|g| inside(&[[g[0][0], g[0][2]], [g[1][0], g[1][2]], [g[2][0], g[2][2]]], p, 0.0)) {
                eprintln!("  ground y {:.2}", plane(g).map_or(f64::NAN, |pl| at(pl, p)));
            }
            for f in floors.iter().filter(|f| inside(&f.t, p, 0.0)) {
                eprintln!("  floor y {:.2} {} (up {:.2})", at(f.plane, p), if f.native { "Burnout's own" } else { "added" }, upness(&f.t.map(|q| [q[0], at(f.plane, q), q[1]])));
            }
            for t in fill.iter().flat_map(|(_, t)| t) {
                let v = t.verts.map(|v| v.map(|c| c as f64));
                if inside(&[[v[0][0], v[0][2]], [v[1][0], v[1][2]], [v[2][0], v[2][2]]], p, 0.0) {
                    eprintln!("  fill y {:.2} kind {}", plane(&v).map_or(f64::NAN, |pl| at(pl, p)), t.kind);
                }
            }
        }
    }
    println!(
        "hole fill: {} ground triangles: {} fully covered by collision, {} partly, {} not at all; {} fill triangles, {:.0} m2 ({} raised beside Burnout's own floors, {:.0} m2); {} slivers under {MIN_WIDTH} m dropped ({:.0} m2); {} shapes failed to triangulate -> {}",
        t.ground, t.covered, t.cut, t.untouched, t.tris, t.area, t.raised, t.raised_area, t.slivers, t.sliver_area, t.failed, out.display()
    );
    if let Some(png) = png {
        check::overview(&floors, &fill, &all_ground, png)?;
    }
    Ok(())
}

/// `bpx col-gen-compare <collision> <gen.bin> <out.png>`: the old collision
/// (the added pieces, with the hand edits) against the generated one, as a
/// top-down map: where each has floors that the other hasn't.
pub fn compare_gen(col: &Path, gen: &Path, png: &Path) -> Result<()> {
    std::env::set_var(ENV, "none");
    let units = crate::colsrc::load_marked(col)?;
    let edits = crate::edits::CollisionEdits::load()?;
    let (mut native, mut old) = (Vec::new(), Vec::new());
    for f in floors_of(&units, &edits) {
        let t = f.t.map(|q| [q[0], at(f.plane, q), q[1]]);
        if f.native {
            native.push(t);
        } else {
            old.push(t);
        }
    }
    let new: Vec<[P3; 3]> = read(gen)?
        .into_iter()
        .flat_map(|(_, t)| t.into_iter().map(|t| t.verts.map(|v| v.map(|c| c as f64))))
        .filter(|t| upness(t) >= MIN_UP)
        .collect();
    eprintln!("compare: {} own floor, {} old added floor, {} generated floor triangles", native.len(), old.len(), new.len());
    check::compare(&native, &old, &new, png)
}

/// An independent check, by sampling rather than polygons: which visible
/// ground has no floor covering it (as for the fill), before and after the
/// fill, drawn as a top-down overview (north up).
mod check {
    use super::*;

    const SAMPLE: f64 = 0.5;
    const PIXEL: usize = 4; // samples per pixel side (2 m pixels)
    const TILE: usize = 256; // samples per tile side
    const LAYERS: usize = 24;

    /// Per sample, each floor's height and how far under ground it covers.
    struct Heights {
        h: Vec<[[f32; 2]; LAYERS]>,
        n: Vec<u8>,
    }

    impl Heights {
        fn new() -> Self {
            Heights { h: vec![[[0.0; 2]; LAYERS]; TILE * TILE], n: vec![0; TILE * TILE] }
        }
        fn add(&mut self, i: usize, y: f32, below: f64) {
            let n = self.n[i] as usize;
            if n < LAYERS {
                self.h[i][n] = [y, below as f32];
                self.n[i] += 1;
            }
        }
        fn covers(&self, i: usize, y: f32) -> bool {
            self.h[i][..self.n[i] as usize].iter().any(|&[h, below]| h >= y - below && h <= y + COVER_ABOVE as f32)
        }
    }

    /// Calls f(sample index, height) for each sample centre of the tile
    /// inside the triangle.
    fn raster(p: &[P3; 3], x0: f64, z0: f64, mut f: impl FnMut(usize, f32)) {
        let Some(pl) = plane(p) else { return };
        let t = [[p[0][0], p[0][2]], [p[1][0], p[1][2]], [p[2][0], p[2][2]]];
        let lo = |k: usize| t.iter().map(|v| v[k]).fold(f64::MAX, f64::min);
        let hi = |k: usize| t.iter().map(|v| v[k]).fold(f64::MIN, f64::max);
        let i0 = (((lo(0) - x0) / SAMPLE - 0.5).ceil().max(0.0)) as usize;
        let i1 = (((hi(0) - x0) / SAMPLE - 0.5).floor().min(TILE as f64 - 1.0)) as i64;
        let j0 = (((lo(1) - z0) / SAMPLE - 0.5).ceil().max(0.0)) as usize;
        let j1 = (((hi(1) - z0) / SAMPLE - 0.5).floor().min(TILE as f64 - 1.0)) as i64;
        if i1 < i0 as i64 || j1 < j0 as i64 {
            return;
        }
        for j in j0..=j1 as usize {
            for i in i0..=i1 as usize {
                let q = [x0 + (i as f64 + 0.5) * SAMPLE, z0 + (j as f64 + 0.5) * SAMPLE];
                if inside(&t, q, 0.0) {
                    f(j * TILE + i, at(pl, q) as f32);
                }
            }
        }
    }

    /// Floor heights per sample (up to COMPARE_LAYERS).
    const COMPARE_LAYERS: usize = 12;
    struct Layers {
        h: Vec<[f32; COMPARE_LAYERS]>,
        n: Vec<u8>,
    }
    impl Layers {
        fn new() -> Self {
            Layers { h: vec![[0.0; COMPARE_LAYERS]; TILE * TILE], n: vec![0; TILE * TILE] }
        }
        fn add(&mut self, i: usize, y: f32) {
            let n = self.n[i] as usize;
            if n < COMPARE_LAYERS {
                self.h[i][n] = y;
                self.n[i] += 1;
            }
        }
        fn at(&self, i: usize) -> &[f32] {
            &self.h[i][..self.n[i] as usize]
        }
    }

    pub(super) fn compare(native: &[[P3; 3]], old: &[[P3; 3]], new: &[[P3; 3]], png: &Path) -> Result<()> {
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in native.iter().chain(old).chain(new).flatten() {
            lo = [lo[0].min(p[0]), lo[1].min(p[2])];
            hi = [hi[0].max(p[0]), hi[1].max(p[2])];
        }
        let span = SAMPLE * TILE as f64;
        let (tx, tz) = (((hi[0] - lo[0]) / span).ceil() as usize, ((hi[1] - lo[1]) / span).ceil() as usize);
        let index = |items: &[[P3; 3]]| {
            let mut g: HashMap<(usize, usize), Vec<u32>> = HashMap::new();
            for (k, p) in items.iter().enumerate() {
                let (a, b) = ([p.iter().map(|v| v[0]).fold(f64::MAX, f64::min), p.iter().map(|v| v[2]).fold(f64::MAX, f64::min)], [p.iter().map(|v| v[0]).fold(f64::MIN, f64::max), p.iter().map(|v| v[2]).fold(f64::MIN, f64::max)]);
                let i0 = ((a[0] - lo[0]) / span).floor().max(0.0) as usize;
                let i1 = (((b[0] - lo[0]) / span).floor().max(0.0) as usize).min(tx - 1);
                let j0 = ((a[1] - lo[1]) / span).floor().max(0.0) as usize;
                let j1 = (((b[1] - lo[1]) / span).floor().max(0.0) as usize).min(tz - 1);
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        g.entry((i, j)).or_default().push(k as u32);
                    }
                }
            }
            g
        };
        let (nix, oix, wix) = (index(native), index(old), index(new));
        let tiles: Vec<(usize, usize)> = (0..tz).flat_map(|j| (0..tx).map(move |i| (i, j))).collect();
        let px = TILE / PIXEL;
        // Areas (samples): both, old only, new only, different heights.
        let done: Vec<(Vec<[u8; 3]>, [usize; 6])> = tiles
            .par_iter()
            .map(|&(ti, tj)| {
                let (x0, z0) = (lo[0] + ti as f64 * span, lo[1] + tj as f64 * span);
                let fill = |items: &[[P3; 3]], ix: &HashMap<(usize, usize), Vec<u32>>| {
                    let mut l = Layers::new();
                    for &k in ix.get(&(ti, tj)).into_iter().flatten() {
                        raster(&items[k as usize], x0, z0, |i, y| l.add(i, y));
                    }
                    l
                };
                let (n, o, w) = (fill(native, &nix), fill(old, &oix), fill(new, &wix));
                let close = |x: f32, b: &[f32]| b.iter().any(|&y| (x - y).abs() <= 2.0);
                // 0 nothing, 1 own floor, 2 both, 3 new only, 4 old under the
                // ground (a stray layer), 5 old above anything new, 6 old only.
                let mut state = vec![0u8; TILE * TILE];
                let mut counts = [0usize; 6];
                for i in 0..TILE * TILE {
                    let (on, oo, ow) = (n.at(i), o.at(i), w.at(i));
                    let lost: Vec<f32> = oo.iter().copied().filter(|&x| !close(x, ow) && !close(x, on)).collect();
                    let top = on.iter().chain(ow).copied().fold(f32::MIN, f32::max);
                    state[i] = if !lost.is_empty() {
                        if on.is_empty() && ow.is_empty() {
                            counts[5] += 1;
                            6
                        } else if lost.iter().any(|&x| x > top) {
                            counts[4] += 1;
                            5
                        } else {
                            counts[3] += 1;
                            4
                        }
                    } else if !oo.is_empty() {
                        counts[0] += 1;
                        2
                    } else if !ow.is_empty() && !ow.iter().any(|&x| close(x, on)) {
                        counts[2] += 1;
                        3
                    } else if !on.is_empty() || !ow.is_empty() {
                        1
                    } else {
                        0
                    };
                }
                let mut pixels = vec![[0u8; 3]; px * px];
                for pj in 0..px {
                    for pi in 0..px {
                        let mut s = 0u8;
                        for j in 0..PIXEL {
                            for i in 0..PIXEL {
                                s = s.max(state[(pj * PIXEL + j) * TILE + pi * PIXEL + i]);
                            }
                        }
                        pixels[pj * px + pi] = match s {
                            1 => [150, 150, 150],
                            2 => [40, 150, 70],
                            3 => [60, 170, 255],
                            4 => [150, 60, 200],
                            5 => [255, 140, 0],
                            6 => [255, 30, 30],
                            _ => [0, 0, 0],
                        };
                    }
                }
                (pixels, counts)
            })
            .collect();
        let (w, h) = (tx * px, tz * px);
        let mut img = vec![0u8; w * h * 3];
        let mut totals = [0usize; 6];
        for (&(ti, tj), (pixels, c)) in tiles.iter().zip(&done) {
            for k in 0..6 {
                totals[k] += c[k];
            }
            for pj in 0..px {
                for pi in 0..px {
                    let o = ((tj * px + pj) * w + ti * px + pi) * 3;
                    img[o..o + 3].copy_from_slice(&pixels[pj * px + pi]);
                }
            }
        }
        let file = std::fs::File::create(png)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header()?.write_image_data(&img)?;
        let km2 = |n: usize| n as f64 * SAMPLE * SAMPLE / 1e6;
        println!(
            "compare (beyond Burnout's own floors): old matched by new {:.2} km2; new only {:.2} km2; old floor under the ground (a stray layer) {:.2} km2; old floor above anything new {:.2} km2; old only, nothing new there {:.2} km2; {}x{} px of {} m, top-left Burnout x {:.0} z {:.0} -> {}",
            km2(totals[0]), km2(totals[2]), km2(totals[3]), km2(totals[4]), km2(totals[5]), w, h, SAMPLE * PIXEL as f64, lo[0], lo[1], png.display()
        );
        Ok(())
    }

    pub(super) fn overview(floors: &[Floor], fill: &[(u32, Vec<FillTri>)], ground: &[[P3; 3]], png: &Path) -> Result<()> {
        let fills: Vec<([P3; 3], u8)> = fill.iter().flat_map(|(_, t)| t.iter().map(|t| (t.verts.map(|v| v.map(|c| c as f64)), t.kind))).collect();
        // Map bounds from the ground and floors.
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in ground.iter().flatten() {
            lo = [lo[0].min(p[0]), lo[1].min(p[2])];
            hi = [hi[0].max(p[0]), hi[1].max(p[2])];
        }
        let span = SAMPLE * TILE as f64;
        let (tx, tz) = (((hi[0] - lo[0]) / span).ceil() as usize, ((hi[1] - lo[1]) / span).ceil() as usize);
        let index = |items: &mut dyn Iterator<Item = (usize, P2, P2)>| {
            let mut g: HashMap<(usize, usize), Vec<u32>> = HashMap::new();
            for (k, a, b) in items {
                let i0 = ((a[0] - lo[0]) / span).floor().max(0.0) as usize;
                let i1 = (((b[0] - lo[0]) / span).floor().max(0.0) as usize).min(tx - 1);
                let j0 = ((a[1] - lo[1]) / span).floor().max(0.0) as usize;
                let j1 = (((b[1] - lo[1]) / span).floor().max(0.0) as usize).min(tz - 1);
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        g.entry((i, j)).or_default().push(k as u32);
                    }
                }
            }
            g
        };
        let bb = |p: &[P3; 3]| ([p.iter().map(|v| v[0]).fold(f64::MAX, f64::min), p.iter().map(|v| v[2]).fold(f64::MAX, f64::min)], [p.iter().map(|v| v[0]).fold(f64::MIN, f64::max), p.iter().map(|v| v[2]).fold(f64::MIN, f64::max)]);
        let ground_ix = index(&mut ground.iter().enumerate().map(|(k, p)| { let (a, b) = bb(p); (k, a, b) }));
        let fill_ix = index(&mut fills.iter().enumerate().map(|(k, p)| { let (a, b) = bb(&p.0); (k, a, b) }));
        let floor_ix = index(&mut floors.iter().enumerate().map(|(k, f)| (k, f.lo, f.hi)));
        // Floors as 3D triangles back from their planes.
        let floor3 = |f: &Floor| -> [P3; 3] { f.t.map(|q| [q[0], at(f.plane, q), q[1]]) };

        // Per tile: pixel colours and hole counts (before, after).
        let tiles: Vec<(usize, usize)> = (0..tz).flat_map(|j| (0..tx).map(move |i| (i, j))).collect();
        let px = TILE / PIXEL;
        let done: Vec<(Vec<[u8; 3]>, usize, usize)> = tiles
            .par_iter()
            .map(|&(ti, tj)| {
                let (x0, z0) = (lo[0] + ti as f64 * span, lo[1] + tj as f64 * span);
                let mut col = Heights::new();
                let mut native = vec![false; TILE * TILE];
                let mut fil = Heights::new();
                let mut raised = vec![false; TILE * TILE];
                for &k in floor_ix.get(&(ti, tj)).into_iter().flatten() {
                    let f = &floors[k as usize];
                    raster(&floor3(f), x0, z0, |i, y| {
                        col.add(i, y, f.below());
                        native[i] |= f.native;
                    });
                }
                for &k in fill_ix.get(&(ti, tj)).into_iter().flatten() {
                    let (p, kind) = &fills[k as usize];
                    raster(p, x0, z0, |i, y| {
                        fil.add(i, y, COVER_BELOW);
                        raised[i] |= *kind == KIND_OPEN;
                    });
                }
                // 0 nothing, 1 added floor, 2 own floor, 3 filled, 4 raised fill, 5 hole.
                let mut state = vec![0u8; TILE * TILE];
                for i in 0..TILE * TILE {
                    if col.n[i] > 0 {
                        state[i] = if native[i] { 2 } else { 1 };
                    }
                }
                let (mut before, mut after) = (0usize, 0usize);
                for &k in ground_ix.get(&(ti, tj)).into_iter().flatten() {
                    raster(&ground[k as usize], x0, z0, |i, y| {
                        if col.covers(i, y) {
                            return;
                        }
                        before += 1;
                        if fil.covers(i, y) {
                            state[i] = state[i].max(if raised[i] { 4 } else { 3 });
                        } else {
                            after += 1;
                            state[i] = 5;
                        }
                    });
                }
                // Sample counts are per ground triangle hit, so a sample under
                // two ground layers counts twice; fine for a report.
                let mut pixels = vec![[0u8; 3]; px * px];
                for pj in 0..px {
                    for pi in 0..px {
                        let mut s = 0u8;
                        for j in 0..PIXEL {
                            for i in 0..PIXEL {
                                s = s.max(state[(pj * PIXEL + j) * TILE + pi * PIXEL + i]);
                            }
                        }
                        pixels[pj * px + pi] = match s {
                            1 => [80, 80, 90],
                            2 => [150, 150, 150],
                            3 => [40, 200, 90],
                            4 => [255, 150, 0],
                            5 => [255, 30, 30],
                            _ => [0, 0, 0],
                        };
                    }
                }
                (pixels, before, after)
            })
            .collect();

        let (w, h) = (tx * px, tz * px);
        let mut img = vec![0u8; w * h * 3];
        let (mut before, mut after) = (0usize, 0usize);
        for (&(ti, tj), (pixels, b, a)) in tiles.iter().zip(&done) {
            before += b;
            after += a;
            for pj in 0..px {
                for pi in 0..px {
                    // Burnout z grows southwards (GTA y = -z): north at the top.
                    let (x, y) = (ti * px + pi, tj * px + pj);
                    let o = (y * w + x) * 3;
                    img[o..o + 3].copy_from_slice(&pixels[pj * px + pi]);
                }
            }
        }
        let file = std::fs::File::create(png)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header()?.write_image_data(&img)?;
        let a = SAMPLE * SAMPLE;
        println!(
            "hole check ({SAMPLE} m samples): ground without collision {:.0} m2, still without after the fill {:.0} m2; overview {}x{} px of {} m, top-left Burnout x {:.0} z {:.0} (GTA x {:.0} y {:.0}) -> {}",
            before as f64 * a, after as f64 * a, w, h, SAMPLE * PIXEL as f64, lo[0], lo[1], lo[0], -lo[1], png.display()
        );
        Ok(())
    }
}
