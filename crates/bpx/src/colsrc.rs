//! Where the world collision comes from.
//!
//! Paradise's collision is Burnout's own (WORLDCOL.BIN) plus a large set of
//! added pieces (out-of-bounds surfaces) made for this project. Only the
//! added pieces are ours to share, so they are kept in a `.pcx` file
//! ("paradise collision extra"): per unit, the order of the combined polygon
//! list, as runs copied from the player's own WORLDCOL.BIN and runs of added
//! polygons. Loading it with the stock file gives back exactly the combined
//! collision, polygon for polygon and in the same order, so the polygon
//! indices the viewer's collision edits use stay valid.
//!
//! Every step that reads collision takes either a Burnout collision bundle
//! (or a game folder: its WORLDCOL.BIN) or a `.pcx` file, for which the stock
//! WORLDCOL.BIN is found through BPX_STOCK_COL or DEFAULT_STOCK.
//!
//! A modded map (OTN, ...) changes some units' collision. The `.pcx` records
//! each unit's stock collision (polygon count and a hash), so a unit that
//! differs is recognised: there the map's own collision is used as it is,
//! with the added pieces after it. Every polygon comes with an id for the
//! collision edits: its index in the stock-plus-added list, which the added
//! pieces keep in changed units too (all hand edits so far are on added
//! pieces), and MODDED_ID + its index for a modded unit's own polygons.

use anyhow::{bail, Context, Result};
use bnd2::Bundle;
use bpworld::{self as bw, types, CollisionPoly, Res};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"PCX2";
/// The first format, without the stock units' fingerprints.
const MAGIC_V1: &[u8; 4] = b"PCX1";
/// Collision edit ids of a modded map's own polygons in a unit it changed.
pub const MODDED_ID: u32 = 0x8000_0000;
/// Burnout's own collision, relative to the project folder.
pub const DEFAULT_STOCK: &str = "../Burnout Paradise Remastered/WORLDCOL.BIN";

/// One unit's polygons, and each polygon's id for the collision edits.
pub type Unit = (u32, Vec<CollisionPoly>, Vec<u32>);
/// A unit as `Unit`, plus which polygons are the map's own (Burnout's, or a
/// modded map's): not added pieces or hole fill.
pub type MarkedUnit = (u32, Vec<CollisionPoly>, Vec<u32>, Vec<bool>);

/// The collision units in `path`, sorted by unit number, with the hole fill
/// when there is one (crate::fill).
pub fn load(path: &Path) -> Result<Vec<Unit>> {
    Ok(load_marked(path)?.into_iter().map(|(u, p, i, _)| (u, p, i)).collect())
}

/// BPX_ADDED=none: leave out the added pieces of a `.pcx` (the map's own
/// collision only, for collision generated from Burnout's visible models
/// instead: crate::fill).
pub fn added_off() -> bool {
    std::env::var("BPX_ADDED").as_deref() == Ok("none")
}

/// As `load`, marking the map's own polygons.
pub fn load_marked(path: &Path) -> Result<Vec<MarkedUnit>> {
    let mut units = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcx")) {
        let stock = std::env::var_os("BPX_STOCK_COL").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_STOCK));
        let stock = if stock.is_dir() { stock.join("WORLDCOL.BIN") } else { stock };
        load_pcx(path, &stock)?
    } else {
        let path = if path.is_dir() { path.join("WORLDCOL.BIN") } else { path.to_path_buf() };
        load_bundle(&path)?
            .into_iter()
            .map(|(u, p)| {
                let ids = (0..p.len() as u32).collect();
                let native = vec![true; p.len()];
                (u, p, ids, native)
            })
            .collect()
    };
    apply_fixes(&mut units)?;
    crate::fill::append(&mut units)?;
    Ok(units)
}

/// Fixes to Burnout's own collision (BPX_COL_FIXES overrides, "none" for
/// none). `drop_water`: per entry a unit and a point (Burnout x, y, z); the
/// whole connected water sheet (surface 15) of that unit through the point,
/// within 1 m of its height, is dropped. Crash sheets Burnout stretches over
/// some basins would otherwise read as lakes far above the real water.
pub const DEFAULT_FIXES: &str = "tools/collision/collision_fixes.json";

fn apply_fixes(units: &mut [MarkedUnit]) -> Result<()> {
    let path = std::env::var_os("BPX_COL_FIXES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_FIXES));
    if path.as_os_str() == "none" || !path.is_file() {
        return Ok(());
    }
    let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?).with_context(|| format!("reading {}", path.display()))?;
    let (mut dropped, mut sheets) = (0usize, 0usize);
    for e in doc["drop_water"].as_array().into_iter().flatten() {
        let unit = e["unit"].as_u64().context("collision fix without unit")? as u32;
        let at: Vec<f32> = e["at"].as_array().context("collision fix without at")?.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
        anyhow::ensure!(at.len() == 3, "collision fix: at takes x, y, z");
        let Some((_, polys, ids, native)) = units.iter_mut().find(|u| u.0 == unit) else {
            eprintln!("collision fixes: unit {unit} not found");
            continue;
        };
        let water: Vec<usize> = (0..polys.len()).filter(|&i| bw::CollisionTag::decode(polys[i].tag).surface_id == bw::SURFACE_WATER).collect();
        // The water polygon under the point, at its height.
        let through = |p: &CollisionPoly| {
            let v = &p.verts;
            (1..v.len() - 1).any(|k| {
                let (a, b, c) = (v[0], v[k], v[k + 1]);
                let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
                if d.abs() < 1e-9 {
                    return false;
                }
                let l1 = ((b[2] - c[2]) * (at[0] - c[0]) + (c[0] - b[0]) * (at[2] - c[2])) / d;
                let l2 = ((c[2] - a[2]) * (at[0] - c[0]) + (a[0] - c[0]) * (at[2] - c[2])) / d;
                let l3 = 1.0 - l1 - l2;
                l1 >= -1e-4 && l2 >= -1e-4 && l3 >= -1e-4 && (l1 * a[1] + l2 * b[1] + l3 * c[1] - at[1]).abs() <= 1.0
            })
        };
        let Some(&seed) = water.iter().find(|&&i| through(&polys[i])) else {
            eprintln!("collision fixes: no water sheet at {at:?} in unit {unit}");
            continue;
        };
        // Flood over water polygons sharing a vertex (to 5 cm).
        let key = |v: [f32; 3]| v.map(|c| (c / 0.05).round() as i32);
        let mut by_vertex: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
        for &i in &water {
            for &v in &polys[i].verts {
                by_vertex.entry(key(v)).or_default().push(i);
            }
        }
        let mut drop = vec![false; polys.len()];
        let mut stack = vec![seed];
        drop[seed] = true;
        while let Some(i) = stack.pop() {
            for &v in &polys[i].verts {
                for &j in &by_vertex[&key(v)] {
                    if !drop[j] {
                        drop[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        let n = drop.iter().filter(|&&d| d).count();
        let mut k = drop.iter();
        polys.retain(|_| !*k.next().unwrap());
        let mut k = drop.iter();
        ids.retain(|_| !*k.next().unwrap());
        let mut k = drop.iter();
        native.retain(|_| !*k.next().unwrap());
        dropped += n;
        sheets += 1;
    }
    eprintln!("collision fixes: {dropped} water polygons in {sheets} sheets dropped ({})", path.display());
    Ok(())
}

fn load_bundle(path: &Path) -> Result<Vec<(u32, Vec<CollisionPoly>)>> {
    let b = Bundle::open(path).with_context(|| format!("opening collision {}", path.display()))?;
    let mut out = Vec::new();
    for e in b.entries.iter().filter(|e| e.type_id == types::POLYGON_SOUP_LIST) {
        let Some(unit) = bw::collision_unit(&b, e.id) else { continue };
        out.push((unit, bw::parse_polygon_soup_list(&Res::load(&b, e)?).polys));
    }
    out.sort_by_key(|u| u.0);
    Ok(out)
}

/// Identity of a polygon when matching against the stock file: exact.
fn key(p: &CollisionPoly) -> (u32, Vec<u32>) {
    (p.tag, p.verts.iter().flat_map(|v| v.map(f32::to_bits)).collect())
}

/// Identity of a polygon's shape, whatever its tag and vertex order, to 2 cm:
/// a modded map that saved a unit again moves its vertices by tiny amounts.
fn shape_key(p: &CollisionPoly) -> Vec<[i32; 3]> {
    let mut v: Vec<[i32; 3]> = p.verts.iter().map(|v| v.map(|c| (c * 50.0).round() as i32)).collect();
    v.sort();
    v
}

/// A unit's stock collision, to recognise it again: FNV-1a over every
/// polygon's tag and vertices.
fn fingerprint(polys: &[CollisionPoly]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut add = |v: u32| {
        for b in v.to_le_bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    };
    for p in polys {
        add(p.tag);
        add(p.verts.len() as u32);
        for v in &p.verts {
            v.iter().for_each(|c| add(c.to_bits()));
        }
    }
    h
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
}

struct Reader<'a>(&'a [u8], usize);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let s = self.0.get(self.1..self.1 + n).context("collision extra file is cut short")?;
        self.1 += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into()?))
    }
}

/// Run kinds in a unit's list.
const RUN_STOCK: u8 = 0;
const RUN_ADDED: u8 = 1;

/// `bpx col-split <stock WORLDCOL.BIN> <combined collision> <out.pcx>`:
/// keeps only what the combined file adds to the stock one, then checks that
/// loading it back gives the combined collision exactly.
pub fn split(stock: &Path, combined: &Path, out: &Path) -> Result<()> {
    std::env::set_var(crate::fill::ENV, "none");
    std::env::set_var("BPX_COL_FIXES", "none");
    let stock_units: HashMap<u32, Vec<CollisionPoly>> = load(stock)?.into_iter().map(|(u, p, _)| (u, p)).collect();
    let combined_units: Vec<(u32, Vec<CollisionPoly>)> = load(combined)?.into_iter().map(|(u, p, _)| (u, p)).collect();
    let mut w = Writer(Vec::new());
    w.0.extend_from_slice(MAGIC);
    w.u32(combined_units.len() as u32);
    let (mut copied, mut added) = (0usize, 0usize);
    for (unit, polys) in &combined_units {
        let empty = Vec::new();
        let base = stock_units.get(unit).unwrap_or(&empty);
        // Where each stock polygon sits (the first, if repeated).
        let mut at: HashMap<(u32, Vec<u32>), u32> = HashMap::with_capacity(base.len());
        for (i, p) in base.iter().enumerate() {
            at.entry(key(p)).or_insert(i as u32);
        }
        // Runs: (stock start, length) or added polygons.
        enum Run<'a> {
            Stock(u32, u32),
            Added(Vec<&'a CollisionPoly>),
        }
        let mut runs: Vec<Run> = Vec::new();
        for p in polys {
            match at.get(&key(p)) {
                Some(&i) => {
                    copied += 1;
                    match runs.last_mut() {
                        Some(Run::Stock(start, len)) if *start + *len == i => *len += 1,
                        _ => runs.push(Run::Stock(i, 1)),
                    }
                }
                None => {
                    added += 1;
                    match runs.last_mut() {
                        Some(Run::Added(list)) => list.push(p),
                        _ => runs.push(Run::Added(vec![p])),
                    }
                }
            }
        }
        w.u32(*unit);
        w.u32(base.len() as u32);
        w.u64(fingerprint(base));
        w.u32(runs.len() as u32);
        for r in &runs {
            match r {
                Run::Stock(start, len) => {
                    w.u8(RUN_STOCK);
                    w.u32(*start);
                    w.u32(*len);
                }
                Run::Added(list) => {
                    w.u8(RUN_ADDED);
                    w.u32(list.len() as u32);
                    for p in list {
                        w.u32(p.tag);
                        w.u8(p.verts.len() as u8);
                        for v in &p.verts {
                            v.iter().for_each(|&c| w.f32(c));
                        }
                    }
                }
            }
        }
    }
    let mut z = ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(&w.0)?;
    std::fs::write(out, z.finish()?)?;

    // Check: stock + extra must give back the combined collision exactly.
    let back = load_pcx(out, stock)?;
    if back.len() != combined_units.len() {
        bail!("check failed: {} units back, {} expected", back.len(), combined_units.len());
    }
    for ((ua, pa, _, _), (ub, pb)) in back.iter().zip(&combined_units) {
        if ua != ub || pa.len() != pb.len() || pa.iter().zip(pb).any(|(a, b)| key(a) != key(b)) {
            bail!("check failed in unit {ub}");
        }
    }
    println!(
        "collision extra: {} units, {copied} polygons from the stock file, {added} added -> {} ({} KB); checked: identical to {}",
        combined_units.len(),
        out.display(),
        std::fs::metadata(out)?.len() / 1024,
        combined.display()
    );
    Ok(())
}

/// `bpx col-bounds <WORLDCOL.BIN>`: each unit's bounding box (Burnout axes).
pub fn bounds(path: &Path) -> Result<()> {
    println!("unit,minx,miny,minz,maxx,maxy,maxz");
    for (u, polys) in load_bundle(path)? {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in polys.iter().flat_map(|p| &p.verts) {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        if !polys.is_empty() {
            println!("{u},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0}", lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]);
        }
    }
    Ok(())
}

/// `bpx col-compare <stock WORLDCOL.BIN> <modded WORLDCOL.BIN>`: which units
/// a mod changed, and how much of the stock collision it kept.
pub fn compare(stock: &Path, modded: &Path) -> Result<()> {
    let a: HashMap<u32, Vec<CollisionPoly>> = load_bundle(stock)?.into_iter().collect();
    let b: HashMap<u32, Vec<CollisionPoly>> = load_bundle(modded)?.into_iter().collect();
    let mut units: Vec<u32> = a.keys().chain(b.keys()).copied().collect();
    units.sort();
    units.dedup();
    let empty = Vec::new();
    let (mut changed, mut kept_total, mut lost_total, mut new_total, mut prefix_units) = (0, 0, 0, 0, 0);
    for u in units {
        let (pa, pb) = (a.get(&u).unwrap_or(&empty), b.get(&u).unwrap_or(&empty));
        if pa.len() == pb.len() && pa.iter().zip(pb).all(|(x, y)| key(x) == key(y)) {
            continue;
        }
        changed += 1;
        let mut have: HashMap<(u32, Vec<u32>), usize> = HashMap::new();
        for p in pb {
            *have.entry(key(p)).or_default() += 1;
        }
        let mut kept = 0;
        for p in pa {
            if let Some(c) = have.get_mut(&key(p)).filter(|c| **c > 0) {
                *c -= 1;
                kept += 1;
            }
        }
        let prefix = pa.iter().zip(pb).take_while(|(x, y)| key(x) == key(y)).count();
        if prefix == pa.len() {
            prefix_units += 1;
        }
        kept_total += kept;
        lost_total += pa.len() - kept;
        new_total += pb.len() - kept;
        println!("unit {u}: stock {} polygons, modded {}; {kept} kept ({prefix} in the same place from the start), {} gone, {} new", pa.len(), pb.len(), pa.len() - kept, pb.len() - kept);
    }
    println!("{changed} units changed: {kept_total} stock polygons kept, {lost_total} gone, {new_total} new; {prefix_units} units keep all of stock as their start");
    Ok(())
}

/// `bpx col-compare <stock> <modded> <extra.pcx>`: also, how many of the
/// added pieces lie where the mod built something new (within 3 m above or
/// below a new polygon's footprint, on a 4 m grid): places to check in game.
pub fn compare_added(stock: &Path, modded: &Path, pcx: &Path) -> Result<()> {
    let a: HashMap<u32, Vec<CollisionPoly>> = load_bundle(stock)?.into_iter().collect();
    std::env::set_var("BPX_STOCK_COL", modded);
    std::env::set_var(crate::fill::ENV, "none");
    std::env::set_var("BPX_COL_FIXES", "none");
    let units = load(pcx)?;
    const CELL: f32 = 4.0;
    let (mut near_total, mut added_total) = (0usize, 0usize);
    let empty = Vec::new();
    for (unit, polys, ids) in units {
        if !ids.iter().any(|&i| i >= MODDED_ID) {
            continue;
        }
        let stock_keys: std::collections::HashSet<Vec<[i32; 3]>> = a.get(&unit).unwrap_or(&empty).iter().map(shape_key).collect();
        // Cells under the mod's new polygons, with their height range.
        let mut cells: HashMap<(i32, i32), (f32, f32)> = HashMap::new();
        for (p, &id) in polys.iter().zip(&ids) {
            if id < MODDED_ID || stock_keys.contains(&shape_key(p)) {
                continue;
            }
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &p.verts {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            // Burnout is y up: the ground plane is x, z.
            for cx in (lo[0] / CELL).floor() as i32..=(hi[0] / CELL).floor() as i32 {
                for cz in (lo[2] / CELL).floor() as i32..=(hi[2] / CELL).floor() as i32 {
                    let c = cells.entry((cx, cz)).or_insert((f32::MAX, f32::MIN));
                    c.0 = c.0.min(lo[1]);
                    c.1 = c.1.max(hi[1]);
                }
            }
        }
        let (mut near, mut added) = (0, 0);
        for (p, &id) in polys.iter().zip(&ids) {
            if id >= MODDED_ID {
                continue;
            }
            added += 1;
            let n = p.verts.len() as f32;
            let c = p.verts.iter().fold([0.0; 3], |s, v| [s[0] + v[0] / n, s[1] + v[1] / n, s[2] + v[2] / n]);
            if let Some(&(lo, hi)) = cells.get(&((c[0] / CELL).floor() as i32, (c[2] / CELL).floor() as i32)) {
                if c[1] >= lo - 3.0 && c[1] <= hi + 3.0 {
                    near += 1;
                }
            }
        }
        if near > 0 {
            println!("unit {unit}: {near} of {added} added pieces where the mod built something new");
        }
        near_total += near;
        added_total += added;
    }
    println!("{near_total} of {added_total} added pieces in changed units lie where the mod built something new");
    Ok(())
}

/// Added pieces dropped for a modded map: in a unit the mod changed, those
/// that clash with its own new collision (`bpx col-modcompat`). Kept in
/// tools/collision/modded_maps.json (or BPX_MODDED_COL), per unit a list of
/// { "map", "count", "fingerprint" (the modded unit's), "drop": [edit ids] },
/// applied only when the modded unit is exactly that one.
pub const DEFAULT_MODDED: &str = "tools/collision/modded_maps.json";

fn modded_path() -> PathBuf {
    std::env::var_os("BPX_MODDED_COL").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_MODDED))
}

/// Added piece ids to drop in `unit` for this modded collision, and the map's name.
fn modded_drops(doc: &serde_json::Value, unit: u32, polys: &[CollisionPoly]) -> Option<(String, std::collections::HashSet<u32>)> {
    let fp = format!("{:016x}", fingerprint(polys));
    doc["units"][unit.to_string()].as_array()?.iter().find_map(|e| {
        (e["count"].as_u64() == Some(polys.len() as u64) && e["fingerprint"].as_str() == Some(fp.as_str())).then(|| {
            let map = e["map"].as_str().unwrap_or("modded map").to_string();
            (map, e["drop"].as_array().into_iter().flatten().filter_map(|v| v.as_u64().map(|v| v as u32)).collect())
        })
    })
}

/// `bpx col-modcompat <stock WORLDCOL.BIN> <modded WORLDCOL.BIN> <extra.pcx> <map name>`:
/// finds the added pieces that clash with a modded map's new collision and
/// records them in modded_maps.json (replacing that map's earlier entries).
///
/// A clash: most of the piece (its corners, edge middles and centre) lies
/// over the mod's new collision (a polygon stock Burnout doesn't have, to
/// 2 cm), from 2 m below to 4 m above it, where stock Burnout had no
/// collision of its own at that height. The added pieces fill in where
/// Burnout has none, so where the mod now has some, theirs wins.
pub fn modcompat(stock: &Path, modded: &Path, pcx: &Path, name: &str) -> Result<()> {
    const CELL: f32 = 2.0;
    let stock_units: HashMap<u32, Vec<CollisionPoly>> = load_bundle(stock)?.into_iter().collect();
    let modded_units: HashMap<u32, Vec<CollisionPoly>> = load_bundle(modded)?.into_iter().collect();
    let path = modded_path();
    std::env::set_var("BPX_STOCK_COL", modded);
    std::env::set_var("BPX_MODDED_COL", "none");
    std::env::set_var(crate::fill::ENV, "none");
    std::env::set_var("BPX_COL_FIXES", "none");
    let units = load(pcx)?;
    let mut doc: serde_json::Value = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| {
        serde_json::json!({"format": "paradise-modded-collision-v1",
            "about": "Added collision pieces dropped for modded Burnout maps, where they clash with the map's own new collision (bpx col-modcompat). Applied per unit only when the modded unit matches exactly."})
    });
    // Drop this map's earlier entries.
    if let Some(units) = doc["units"].as_object_mut() {
        for list in units.values_mut() {
            if let Some(a) = list.as_array_mut() {
                a.retain(|e| e["map"].as_str() != Some(name));
            }
        }
        units.retain(|_, l| l.as_array().is_some_and(|a| !a.is_empty()));
    }
    // Cells (x, z) a set of polygons covers, with the height ranges there.
    fn cover(polys: &[&CollisionPoly]) -> HashMap<(i32, i32), Vec<(f32, f32)>> {
        let mut out: HashMap<(i32, i32), Vec<(f32, f32)>> = HashMap::new();
        for p in polys {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &p.verts {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            let inside = |x: f32, z: f32| {
                let n = p.verts.len();
                let mut c = false;
                for i in 0..n {
                    let (a, b) = (p.verts[i], p.verts[(i + n - 1) % n]);
                    if (a[2] > z) != (b[2] > z) && x < (b[0] - a[0]) * (z - a[2]) / (b[2] - a[2]) + a[0] {
                        c = !c;
                    }
                }
                c
            };
            for cx in (lo[0] / CELL).floor() as i32..=(hi[0] / CELL).floor() as i32 {
                for cz in (lo[2] / CELL).floor() as i32..=(hi[2] / CELL).floor() as i32 {
                    let (x, z) = ((cx as f32 + 0.5) * CELL, (cz as f32 + 0.5) * CELL);
                    let corner = p.verts.iter().any(|v| (v[0] / CELL).floor() as i32 == cx && (v[2] / CELL).floor() as i32 == cz);
                    if corner || inside(x, z) {
                        out.entry((cx, cz)).or_default().push((lo[1], hi[1]));
                    }
                }
            }
        }
        out
    }
    let empty = Vec::new();
    let (mut total, mut changed) = (0usize, 0usize);
    for (unit, polys, ids) in &units {
        if !ids.iter().any(|&i| i >= MODDED_ID) {
            continue;
        }
        changed += 1;
        let stock_polys = stock_units.get(unit).unwrap_or(&empty);
        let stock_shapes: std::collections::HashSet<Vec<[i32; 3]>> = stock_polys.iter().map(shape_key).collect();
        let new: Vec<&CollisionPoly> = polys.iter().zip(ids).filter(|(p, &id)| id >= MODDED_ID && !stock_shapes.contains(&shape_key(p))).map(|(p, _)| p).collect();
        if new.is_empty() {
            continue;
        }
        let new_cells = cover(&new);
        let stock_cells = cover(&stock_polys.iter().collect::<Vec<_>>());
        let near = |cells: &HashMap<(i32, i32), Vec<(f32, f32)>>, x: f32, y: f32, z: f32| {
            cells.get(&((x / CELL).floor() as i32, (z / CELL).floor() as i32)).is_some_and(|l| l.iter().any(|&(lo, hi)| y >= lo - 2.0 && y <= hi + 4.0))
        };
        let mut drop: Vec<u32> = Vec::new();
        for (p, &id) in polys.iter().zip(ids) {
            if id >= MODDED_ID || p.verts.is_empty() {
                continue;
            }
            let n = p.verts.len();
            let c = p.verts.iter().fold([0.0; 3], |s, v| [s[0] + v[0] / n as f32, s[1] + v[1] / n as f32, s[2] + v[2] / n as f32]);
            let mut samples = vec![c];
            for i in 0..n {
                let (a, b) = (p.verts[i], p.verts[(i + 1) % n]);
                samples.push(a);
                samples.push([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]);
            }
            let clash = samples.iter().filter(|s| near(&new_cells, s[0], s[1], s[2]) && !near(&stock_cells, s[0], s[1], s[2])).count();
            if clash * 2 >= samples.len() {
                drop.push(id);
            }
        }
        if drop.is_empty() {
            continue;
        }
        let modded_polys = modded_units.get(unit).unwrap_or(&empty);
        println!("unit {unit}: {} of {} added pieces clash with {name}'s new collision", drop.len(), ids.iter().filter(|&&i| i < MODDED_ID).count());
        total += drop.len();
        let entry = serde_json::json!({"map": name, "count": modded_polys.len(), "fingerprint": format!("{:016x}", fingerprint(modded_polys)), "drop": drop});
        let units = doc["units"].as_object_mut().map(|_| ()).is_some();
        if !units {
            doc["units"] = serde_json::json!({});
        }
        let list = doc["units"].as_object_mut().unwrap().entry(unit.to_string()).or_insert_with(|| serde_json::json!([]));
        list.as_array_mut().unwrap().push(entry);
    }
    std::fs::write(&path, serde_json::to_string(&doc)?)?;
    println!("{name}: {changed} units changed; {total} added pieces dropped -> {}", path.display());
    Ok(())
}

fn load_pcx(path: &Path, stock: &Path) -> Result<Vec<MarkedUnit>> {
    let mut data = Vec::new();
    ZlibDecoder::new(std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?).read_to_end(&mut data)?;
    let mut stock_units: HashMap<u32, Vec<CollisionPoly>> = load_bundle(stock)
        .with_context(|| format!("{} needs Burnout's own collision (BPX_STOCK_COL)", path.display()))?
        .into_iter()
        .collect();
    let drops_doc: Option<serde_json::Value> = {
        let p = modded_path();
        (p.as_os_str() != "none").then(|| std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok())).flatten()
    };
    let mut dropped: HashMap<String, usize> = HashMap::new();
    let skip_added = added_off();
    if skip_added {
        eprintln!("collision: added pieces left out (BPX_ADDED=none)");
    }
    let mut r = Reader(&data, 0);
    let v2 = match r.take(4)? {
        m if m == MAGIC => true,
        m if m == MAGIC_V1 => false,
        _ => bail!("{} is not a collision extra file", path.display()),
    };
    let n = r.u32()?;
    let mut out = Vec::with_capacity(n as usize);
    let mut modded: Vec<u32> = Vec::new();
    for _ in 0..n {
        let unit = r.u32()?;
        let base = stock_units.remove(&unit).unwrap_or_default();
        // Is this the stock unit the extra file was made against?
        let same = if v2 {
            let (count, hash) = (r.u32()?, r.u64()?);
            base.len() == count as usize && fingerprint(&base) == hash
        } else {
            true
        };
        let runs = r.u32()?;
        let (mut polys, mut ids, mut native) = (Vec::new(), Vec::new(), Vec::new());
        if !same {
            // A modded map's own collision, then the added pieces.
            modded.push(unit);
            ids.extend((0..base.len() as u32).map(|i| MODDED_ID + i));
            polys.extend_from_slice(&base);
            native.resize(polys.len(), true);
        }
        let mut at = 0u32;
        for _ in 0..runs {
            match r.u8()? {
                RUN_STOCK => {
                    let (start, len) = (r.u32()? as usize, r.u32()? as usize);
                    if same {
                        let part = base.get(start..start + len).with_context(|| {
                            format!("unit {unit}: Burnout's collision doesn't match (is it Burnout Paradise Remastered's WORLDCOL.BIN?)")
                        })?;
                        polys.extend_from_slice(part);
                        ids.extend(at..at + len as u32);
                        native.resize(polys.len(), true);
                    }
                    at += len as u32;
                }
                RUN_ADDED => {
                    for _ in 0..r.u32()? {
                        let tag = r.u32()?;
                        let nv = r.u8()? as usize;
                        let mut verts = Vec::with_capacity(nv);
                        for _ in 0..nv {
                            verts.push([r.f32()?, r.f32()?, r.f32()?]);
                        }
                        if !skip_added {
                            polys.push(CollisionPoly { verts, tag });
                            ids.push(at);
                            native.push(false);
                        }
                        at += 1;
                    }
                }
                k => bail!("{}: unknown run kind {k}", path.display()),
            }
        }
        if !same {
            // Added pieces that clash with the modded map's own collision.
            if let Some((map, drop)) = drops_doc.as_ref().and_then(|d| modded_drops(d, unit, &base)) {
                let before = polys.len();
                let keep: Vec<bool> = ids.iter().map(|id| !drop.contains(id)).collect();
                let mut k = keep.iter();
                polys.retain(|_| *k.next().unwrap());
                let mut k = keep.iter();
                ids.retain(|_| *k.next().unwrap());
                let mut k = keep.iter();
                native.retain(|_| *k.next().unwrap());
                *dropped.entry(map).or_default() += before - polys.len();
            }
        }
        out.push((unit, polys, ids, native));
    }
    for (map, n) in &dropped {
        eprintln!("collision: {n} added pieces dropped where they clash with {map}'s own collision");
    }
    // Units only the modded map has collision in.
    for (unit, polys) in stock_units.into_iter().filter(|(_, p)| !p.is_empty()) {
        modded.push(unit);
        let ids = (0..polys.len() as u32).map(|i| MODDED_ID + i).collect();
        let native = vec![true; polys.len()];
        out.push((unit, polys, ids, native));
    }
    if !modded.is_empty() {
        modded.sort();
        eprintln!(
            "collision: {} units differ from stock Burnout (a modded map); there its own collision is used, with the added pieces: {modded:?}",
            modded.len()
        );
    }
    out.sort_by_key(|u| u.0);
    Ok(out)
}
