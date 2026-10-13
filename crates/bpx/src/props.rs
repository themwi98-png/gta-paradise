//! Burnout's placed props (traffic lights, lamp posts, bins, signs, ...), and
//! GTA props standing in for them.
//!
//! Each track unit bundle has a PropInstances resource (type 0x10011) and a
//! PropGraphicsList (0x10010). Layouts, worked out from the files (32-bit):
//!
//! PropInstances: header u32[8] = [end of instance array, ?, offset of
//! instances (0x20), ?, ?, instance count, unit, 0], then per instance 0x50
//! bytes: a 4x4 matrix (rows: local x, y (up), z, position; Burnout space),
//! u32 prop type, u32 instance id, u32 flags, u32 0.
//!
//! PropGraphicsList: header u32[6] = [import table, unit, type count, part
//! count, offset of types, offset of parts], then per type 12 bytes: u32
//! prop type, Model pointer (an import; its name gives the prop, e.g.
//! ".../pol_trafficlight_10a_L.PropScene"), parts pointer.
//!
//! Burnout prop models are centred on their middle; GTA props stand on
//! their origin (a lamp post or traffic light on its pole). With the
//! collision file given, each GTA prop is set down on the floor under it
//! (props placed from the model extents alone sat about a metre deep).

use crate::gta::Placement;
use crate::gtavis::{joaat, v3, YMAP_TAIL};
use anyhow::{Context, Result};
use bnd2::{f32_at, u32_at, Bundle};
use bpworld::Res;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

const PROP_GRAPHICS_LIST: u32 = 0x10010;
const PROP_INSTANCES: u32 = 0x10011;

pub struct Prop {
    pub unit: u32,
    pub kind: u32,
    pub name: String,
    /// The prop's Model resource (in the unit bundle or GLOBALPROPS.BIN).
    pub model: Option<u64>,
    /// Burnout-space matrix rows: local x, up, local z, position.
    pub m: [[f32; 3]; 4],
}

/// "gamedb://.../pol_trafficlight_10a_L.PropScene?ID=416986" -> "pol_trafficlight_10a_L".
fn prop_name(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.split(['.', '?']).next().unwrap_or(file).to_string()
}

pub fn read_unit(game: &Path, unit: u32) -> Result<Vec<Prop>> {
    let b = Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
    let mut names: HashMap<u32, (String, Option<u64>)> = HashMap::new();
    for e in b.entries.iter().filter(|e| e.type_id == PROP_GRAPHICS_LIST) {
        let r = Res::load(&b, e)?;
        let (count, at) = (u32_at(&r.main, 8) as usize, u32_at(&r.main, 16) as usize);
        for k in 0..count {
            let o = at + k * 12;
            if o + 12 > r.main.len() {
                break;
            }
            let kind = u32_at(&r.main, o);
            let model = r.import(o + 4);
            if let Some(name) = model.and_then(|id| b.name_of(id)) {
                names.insert(kind, (prop_name(name), model));
            }
        }
    }
    let mut out = Vec::new();
    for e in b.entries.iter().filter(|e| e.type_id == PROP_INSTANCES) {
        let r = Res::load(&b, e)?;
        let (end, at, count) = (u32_at(&r.main, 0) as usize, u32_at(&r.main, 8) as usize, u32_at(&r.main, 20) as usize);
        anyhow::ensure!(at + count * 0x50 == end, "unit {unit}: unexpected PropInstances layout");
        for k in 0..count {
            let o = at + k * 0x50;
            let f = |i: usize| f32_at(&r.main, o + i * 4);
            let m = [[f(0), f(1), f(2)], [f(4), f(5), f(6)], [f(8), f(9), f(10)], [f(12), f(13), f(14)]];
            let kind = u32_at(&r.main, o + 0x40);
            let (name, model) = names.get(&kind).cloned().unwrap_or_else(|| (format!("type_{kind}"), None));
            out.push(Prop { unit, kind, name, model, m });
        }
    }
    Ok(out)
}

pub fn read_all(game: &Path) -> Result<Vec<Prop>> {
    let mut units: Vec<u32> = std::fs::read_dir(game)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok()?.strip_prefix("TRK_UNIT")?.strip_suffix("_GR.BNDL")?.parse().ok())
        .collect();
    units.sort();
    let mut all = Vec::new();
    for u in units {
        all.extend(read_unit(game, u)?);
    }
    Ok(all)
}

/// A prop model's extent in GTA-style local axes (x, y = Burnout -z, z up).
#[derive(Clone, Copy)]
pub struct Shape {
    pub lo: [f32; 3],
    pub hi: [f32; 3],
    /// Middle of the model's footprint at ground level (the pole of a lamp
    /// post or traffic light), x / y.
    pub base: [f32; 2],
}

/// Shape of each prop kind's model, keyed by prop name. Models are looked up
/// in the unit bundles first, then GLOBALPROPS.BIN.
pub fn model_shapes(game: &Path, props: &[Prop]) -> Result<HashMap<String, Shape>> {
    let global = Bundle::open(game.join("GLOBALPROPS.BIN"))?;
    let mut by_unit: BTreeMap<u32, Vec<(&str, u64)>> = BTreeMap::new();
    let mut seen = HashSet::new();
    for p in props {
        if let Some(m) = p.model {
            if seen.insert(p.name.as_str()) {
                by_unit.entry(p.unit).or_default().push((p.name.as_str(), m));
            }
        }
    }
    let mut out = HashMap::new();
    for (unit, list) in by_unit {
        let local = Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
        for (name, model) in list {
            let bundle_of = |id: u64| {
                if local.find(id).is_some() {
                    Some(&local)
                } else if global.find(id).is_some() {
                    Some(&global)
                } else {
                    None
                }
            };
            let Some(mb) = bundle_of(model) else { continue };
            let m = bpworld::parse_model(&Res::load(mb, mb.find(model).unwrap())?);
            let Some(rid) = m.lod0() else { continue };
            let Some(rb) = bundle_of(rid) else { continue };
            let Ok(meshes) = bpworld::decode_renderable(rb, rb.find(rid).unwrap()) else { continue };
            let pts: Vec<[f32; 3]> = meshes.iter().flat_map(|g| &g.positions).map(|v| [v[0], -v[2], v[1]]).collect();
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for q in &pts {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            if lo[0] > hi[0] {
                continue;
            }
            // Footprint: vertices in the bottom 15 % of the height.
            let cut = lo[2] + 0.15 * (hi[2] - lo[2]);
            let low: Vec<&[f32; 3]> = pts.iter().filter(|q| q[2] <= cut).collect();
            let (fx0, fx1) = low.iter().fold((f32::MAX, f32::MIN), |a, q| (a.0.min(q[0]), a.1.max(q[0])));
            let (fy0, fy1) = low.iter().fold((f32::MAX, f32::MIN), |a, q| (a.0.min(q[1]), a.1.max(q[1])));
            let base = if low.is_empty() { [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0] } else { [(fx0 + fx1) / 2.0, (fy0 + fy1) / 2.0] };
            out.insert(name.to_string(), Shape { lo, hi, base });
        }
    }
    Ok(out)
}

/// Heading (degrees, GTA convention: 0 = +y, counter-clockwise) of a prop's
/// local Burnout axis `row` (0 = x, 2 = z) once placed in GTA.
pub fn heading_of(p: &Prop, row: usize, placement: &Placement) -> f32 {
    let d = placement.apply_dir(p.m[row]);
    (-d[0]).atan2(d[1]).to_degrees().rem_euclid(360.0)
}

/// `bpx props <game-dir> <out.csv> [--offset x,y,z]`: every placed prop,
/// plus prop_kinds.csv (count and model extent per kind) next to it.
pub fn export_csv(game: &Path, out: &Path, placement: Placement) -> Result<()> {
    let props = read_all(game)?;
    let mut csv = String::from("unit,type,name,x,y,z,heading_x,heading_z,up_z\n");
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for p in &props {
        let g = placement.apply(p.m[3]);
        let up = placement.apply_dir(p.m[1]);
        let _ = writeln!(
            csv,
            "{},{},{},{:.2},{:.2},{:.2},{:.1},{:.1},{:.3}",
            p.unit,
            p.kind,
            p.name,
            g[0],
            g[1],
            g[2],
            heading_of(p, 0, &placement),
            heading_of(p, 2, &placement),
            up[2]
        );
        *counts.entry(p.name.clone()).or_default() += 1;
    }
    std::fs::write(out, csv)?;
    let shapes = model_shapes(game, &props)?;
    let mut kinds = String::from("name,count,minx,miny,minz,maxx,maxy,maxz,basex,basey\n");
    for (name, n) in &counts {
        match shapes.get(name) {
            Some(s) => {
                let _ = writeln!(
                    kinds,
                    "{name},{n},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
                    s.lo[0], s.lo[1], s.lo[2], s.hi[0], s.hi[1], s.hi[2], s.base[0], s.base[1]
                );
            }
            None => {
                let _ = writeln!(kinds, "{name},{n},,,,,,,,");
            }
        }
    }
    std::fs::write(out.with_file_name("prop_kinds.csv"), kinds)?;
    let mut by_count: Vec<_> = counts.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1));
    println!("props: {} placed, {} kinds -> {}", props.len(), by_count.len(), out.display());
    for (name, n) in by_count {
        println!("  {n:6}  {name}");
    }
    Ok(())
}

/// GTA prop archetypes: name hash -> (bbMin, bbMax), from the CSV written by
/// `cwconv archetypes` (names it could not resolve are already hashes).
fn gta_archetypes(path: &Path) -> Result<HashMap<u32, ([f32; 3], [f32; 3])>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {} (run: cwconv archetypes)", path.display()))?;
    let mut out = HashMap::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 8 {
            continue;
        }
        let v = |i: usize| f[i].parse::<f32>().unwrap_or(0.0);
        let hash = f[1].parse::<u32>().unwrap_or_else(|_| joaat(f[1]));
        out.insert(hash, ([v(2), v(3), v(4)], [v(5), v(6), v(7)]));
    }
    Ok(out)
}

/// The Burnout name -> (GTA model, yaw offset) table (tools/props_map.csv).
fn read_map(path: &Path) -> Result<HashMap<String, (String, f32)>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut out = HashMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() >= 2 {
            out.insert(f[0].to_string(), (f[1].to_string(), f.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0)));
        }
    }
    Ok(out)
}

/// Yaw (radians, about local z) turning the GTA model's frame onto the
/// Burnout model's: arms (footprint to middle of the box) point the same
/// way; otherwise long horizontal sides line up.
fn match_yaw(b: &Shape, g: ([f32; 3], [f32; 3])) -> f32 {
    let mid = |lo: [f32; 3], hi: [f32; 3]| [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let bm = mid(b.lo, b.hi);
    let arm_b = [bm[0] - b.base[0], bm[1] - b.base[1]];
    // GTA props stand on their origin.
    let arm_g = mid(g.0, g.1);
    let len = |v: [f32; 2]| v[0].hypot(v[1]);
    if len(arm_b) > 0.5 && len(arm_g) > 0.5 {
        return arm_b[1].atan2(arm_b[0]) - arm_g[1].atan2(arm_g[0]);
    }
    let (bw, bd) = (b.hi[0] - b.lo[0], b.hi[1] - b.lo[1]);
    let (gw, gd) = (g.1[0] - g.0[0], g.1[1] - g.0[1]);
    if bw.max(bd) > 1.25 * bw.min(bd) && gw.max(gd) > 1.25 * gw.min(gd) {
        let long = |w: f32, d: f32| if w >= d { 0.0 } else { std::f32::consts::FRAC_PI_2 };
        long(bw, bd) - long(gw, gd)
    } else {
        0.0
    }
}

/// Rotation (columns = local x, y, z axes in world) to quaternion (x, y, z, w).
fn quat(c: [[f32; 3]; 3]) -> [f32; 4] {
    // m[row][col]
    let m = |r: usize, k: usize| c[k][r];
    let t = m(0, 0) + m(1, 1) + m(2, 2);
    let q = if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        [(m(2, 1) - m(1, 2)) / s, (m(0, 2) - m(2, 0)) / s, (m(1, 0) - m(0, 1)) / s, 0.25 * s]
    } else if m(0, 0) > m(1, 1) && m(0, 0) > m(2, 2) {
        let s = (1.0 + m(0, 0) - m(1, 1) - m(2, 2)).sqrt() * 2.0;
        [0.25 * s, (m(0, 1) + m(1, 0)) / s, (m(0, 2) + m(2, 0)) / s, (m(2, 1) - m(1, 2)) / s]
    } else if m(1, 1) > m(2, 2) {
        let s = (1.0 + m(1, 1) - m(0, 0) - m(2, 2)).sqrt() * 2.0;
        [(m(0, 1) + m(1, 0)) / s, 0.25 * s, (m(1, 2) + m(2, 1)) / s, (m(0, 2) - m(2, 0)) / s]
    } else {
        let s = (1.0 + m(2, 2) - m(0, 0) - m(1, 1)).sqrt() * 2.0;
        [(m(0, 2) + m(2, 0)) / s, (m(1, 2) + m(2, 1)) / s, 0.25 * s, (m(1, 0) - m(0, 1)) / s]
    };
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-6);
    q.map(|v| v / l)
}

fn norm3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    v.map(|c| c / l)
}

/// Floor triangles (placed, GTA space) in an 8 m grid, for setting props
/// down on the ground.
pub(crate) struct Floors {
    cells: HashMap<(i32, i32), Vec<[[f32; 3]; 3]>>,
}

const FLOOR_CELL: f32 = 8.0;

impl Floors {
    pub(crate) fn load(col: &Path, placement: &Placement) -> Result<Self> {
        let b = Bundle::open(col)?;
        let edits = crate::edits::CollisionEdits::load()?;
        let mut cells: HashMap<(i32, i32), Vec<[[f32; 3]; 3]>> = HashMap::new();
        for e in b.entries.iter().filter(|e| e.type_id == bpworld::types::POLYGON_SOUP_LIST) {
            for p in edits.polys(&b, e)? {
                if bpworld::CollisionTag::decode(p.tag).surface_id == bpworld::SURFACE_INVISIBLE_WALL {
                    continue;
                }
                let v: Vec<[f32; 3]> = p.verts.iter().map(|&v| placement.apply(v)).collect();
                let mut tris = vec![[v[0], v[1], v[2]]];
                if v.len() == 4 {
                    tris.push([v[0], v[2], v[3]]);
                }
                for t in tris {
                    let (x0, x1) = (t.iter().map(|q| q[0]).fold(f32::MAX, f32::min), t.iter().map(|q| q[0]).fold(f32::MIN, f32::max));
                    let (y0, y1) = (t.iter().map(|q| q[1]).fold(f32::MAX, f32::min), t.iter().map(|q| q[1]).fold(f32::MIN, f32::max));
                    for cx in (x0 / FLOOR_CELL).floor() as i32..=(x1 / FLOOR_CELL).floor() as i32 {
                        for cy in (y0 / FLOOR_CELL).floor() as i32..=(y1 / FLOOR_CELL).floor() as i32 {
                            cells.entry((cx, cy)).or_default().push(t);
                        }
                    }
                }
            }
        }
        Ok(Self { cells })
    }

    /// Heights of the upward-facing surfaces at (x, y).
    pub(crate) fn heights(&self, x: f32, y: f32) -> Vec<f32> {
        let mut out = Vec::new();
        for t in self.cells.get(&((x / FLOOR_CELL).floor() as i32, (y / FLOOR_CELL).floor() as i32)).into_iter().flatten() {
            let (a, b, c) = (t[0], t[1], t[2]);
            let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if d.abs() < 1e-6 {
                continue;
            }
            let l1 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
            let l2 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
            let l3 = 1.0 - l1 - l2;
            if l1 < -1e-4 || l2 < -1e-4 || l3 < -1e-4 {
                continue;
            }
            // Floors only (normal pointing up, either winding).
            let n = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            let len = {
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let cr = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt()
            };
            if len > 0.0 && n.abs() / len > 0.6 {
                out.push(l1 * a[2] + l2 * b[2] + l3 * c[2]);
            }
        }
        out
    }

    /// The floor under a prop: the highest surface below its centre `top`
    /// (Burnout props are placed by their middle), no lower than 1 m under
    /// its model's bottom `bottom`.
    fn ground(&self, x: f32, y: f32, bottom: f32, top: f32) -> Option<f32> {
        self.heights(x, y).into_iter().filter(|&h| h >= bottom - 1.0 && h <= top + 0.3).max_by(f32::total_cmp)
    }
}

/// `bpx gta-props <game-dir> <meta-out> [--offset x,y,z] [--col <WORLDCOL file>]`: Burnout's props
/// replaced by similar GTA props (tools/props_map.csv), one ymap per unit
/// (bp_props_<unit>.ymap.xml) in <meta-out>. Needs out/props/gta_archetypes.csv
/// (`cwconv archetypes`) for the GTA props' extents.
pub fn export_gta(game: &Path, out_dir: &Path, placement: Placement, col: Option<&Path>) -> Result<()> {
    let floors = col.map(|c| Floors::load(c, &placement)).transpose()?;
    let (mut grounded, mut ungrounded) = (0, 0);
    let mut lifts: Vec<f32> = Vec::new();
    let mut no_floor: BTreeMap<&str, usize> = BTreeMap::new();
    let map = read_map(Path::new("tools/props_map.csv"))?;
    let arch = gta_archetypes(Path::new("out/props/gta_archetypes.csv"))?;
    let unknown: Vec<&String> = map.values().map(|(g, _)| g).filter(|g| !arch.contains_key(&joaat(g))).collect();
    anyhow::ensure!(unknown.is_empty(), "tools/props_map.csv names GTA props that do not exist: {unknown:?}");
    let props = read_all(game)?;
    let shapes = model_shapes(game, &props)?;
    std::fs::create_dir_all(out_dir)?;
    for e in std::fs::read_dir(out_dir)?.flatten() {
        if e.file_name().to_string_lossy().starts_with("bp_props_") {
            let _ = std::fs::remove_file(e.path());
        }
    }

    let mut by_unit: BTreeMap<u32, (String, [f32; 3], [f32; 3])> = BTreeMap::new();
    // For paradise.asi, which keeps cars from being damaged by these props.
    let mut positions = String::from("x,y,z,model\n");
    let (mut placed, mut skipped) = (0, 0);
    for (i, p) in props.iter().enumerate() {
        let (Some((gta, yaw_off)), Some(shape)) = (map.get(&p.name), shapes.get(&p.name)) else {
            skipped += 1;
            continue;
        };
        let g = arch[&joaat(gta)];
        // Burnout local axes in GTA world: x, y = -(local z), z = up.
        let ax = norm3(placement.apply_dir(p.m[0]));
        let az = norm3(placement.apply_dir(p.m[1]));
        let ay = norm3(placement.apply_dir(p.m[2]).map(|c| -c));
        let pos = placement.apply(p.m[3]);
        let mut at = [0, 1, 2].map(|k| pos[k] + ax[k] * shape.base[0] + ay[k] * shape.base[1] + az[k] * shape.lo[2]);
        if let Some(f) = &floors {
            let centre = [0, 1, 2].map(|k| pos[k] + ax[k] * shape.base[0] + ay[k] * shape.base[1]);
            match f.ground(at[0], at[1], at[2], centre[2].max(at[2])) {
                Some(z) => {
                    lifts.push(z - at[2]);
                    at[2] = z;
                    grounded += 1;
                }
                None => {
                    ungrounded += 1;
                    *no_floor.entry(p.name.as_str()).or_default() += 1;
                }
            }
        }
        // Most GTA props stand on their origin; a few (barrels, some gates)
        // are centred on it and are lifted by their depth below it.
        if g.0[2] < -0.3 {
            at = [0, 1, 2].map(|k| at[k] - az[k] * g.0[2]);
        }
        let (s, c) = (match_yaw(shape, g) + yaw_off.to_radians()).sin_cos();
        let gx = [0, 1, 2].map(|k| ax[k] * c + ay[k] * s);
        let gy = [0, 1, 2].map(|k| -ax[k] * s + ay[k] * c);
        // A symmetric Burnout prop replaced by a GTA one whose origin is at
        // one end (fence sections): centre the GTA prop on the Burnout one.
        let arm_b = [(shape.lo[0] + shape.hi[0]) / 2.0 - shape.base[0], (shape.lo[1] + shape.hi[1]) / 2.0 - shape.base[1]];
        let gm = [(g.0[0] + g.1[0]) / 2.0, (g.0[1] + g.1[1]) / 2.0];
        if arm_b[0].hypot(arm_b[1]) <= 0.5 && gm[0].hypot(gm[1]) > 0.5 {
            at = [0, 1, 2].map(|k| at[k] - gx[k] * gm[0] - gy[k] * gm[1]);
        }
        let q = quat([gx, gy, az]);
        let _ = writeln!(positions, "{:.1},{:.1},{:.1},{gta}", at[0], at[1], at[2]);
        let (xml, lo, hi) = by_unit.entry(p.unit).or_insert((String::new(), [f32::MAX; 3], [f32::MIN; 3]));
        // Map entities store the inverse rotation (checked against Los
        // Santos street lights, whose arms reach over the road).
        let _ = write!(
            xml,
            "  <Item type=\"CEntityDef\">\n   <archetypeName>{gta}</archetypeName>\n   <flags value=\"1572864\" />\n   <guid value=\"{}\" />\n   {}\n   <rotation x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />\n   <scaleXY value=\"1\" />\n   <scaleZ value=\"1\" />\n   <parentIndex value=\"-1\" />\n   <lodDist value=\"-1\" />\n   <childLodDist value=\"0\" />\n   <lodLevel>LODTYPES_DEPTH_ORPHANHD</lodLevel>\n   <numChildren value=\"0\" />\n   <priorityLevel>PRI_REQUIRED</priorityLevel>\n   <extensions />\n   <ambientOcclusionMultiplier value=\"255\" />\n   <artificialAmbientOcclusion value=\"255\" />\n   <tintValue value=\"0\" />\n  </Item>\n",
            joaat(&format!("bp_prop_{}_{i}", p.unit)),
            v3("position", at),
            -q[0],
            -q[1],
            -q[2],
            q[3]
        );
        let r = (g.1[0] - g.0[0]).abs().max((g.1[1] - g.0[1]).abs()).max(g.1[2] - g.0[2]);
        for k in 0..3 {
            lo[k] = lo[k].min(at[k] - r);
            hi[k] = hi[k].max(at[k] + r);
        }
        placed += 1;
    }
    std::fs::create_dir_all("out/props")?;
    std::fs::write("out/props/paradise_props.csv", positions)?;
    for (unit, (entities, lo, hi)) in &by_unit {
        let ymap = format!("bp_props_{unit}");
        let d = 150.0;
        let head = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>{ymap}</name>\n <parent />\n <flags value=\"0\" />\n <contentFlags value=\"1\" />\n {}\n {}\n {}\n {}\n <entities>\n",
            v3("streamingExtentsMin", lo.map(|v| v - d)),
            v3("streamingExtentsMax", hi.map(|v| v + d)),
            v3("entitiesExtentsMin", *lo),
            v3("entitiesExtentsMax", *hi)
        );
        std::fs::write(out_dir.join(format!("{ymap}.ymap.xml")), head + entities + &YMAP_TAIL.replace("{ymap}", &ymap))?;
    }
    if !lifts.is_empty() {
        lifts.sort_by(f32::total_cmp);
        let q = |f: f32| lifts[((lifts.len() - 1) as f32 * f) as usize];
        println!("props: moved up onto the floor by {:.2} m (median; 10-90 %: {:.2} .. {:.2})", q(0.5), q(0.1), q(0.9));
    }
    if !no_floor.is_empty() {
        let mut v: Vec<_> = no_floor.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        println!("props: no floor found under: {}", v.iter().take(8).map(|(n, c)| format!("{n} {c}")).collect::<Vec<_>>().join(", "));
    }
    println!("props: {placed} GTA props placed in {} ymaps ({grounded} set on the floor, {ungrounded} with no floor found); {skipped} Burnout props have no GTA stand-in -> {}", by_unit.len(), out_dir.display());
    Ok(())
}
