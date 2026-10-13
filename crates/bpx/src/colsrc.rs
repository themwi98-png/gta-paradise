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

use anyhow::{bail, Context, Result};
use bnd2::Bundle;
use bpworld::{self as bw, types, CollisionPoly, Res};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"PCX1";
/// Burnout's own collision, relative to the project folder.
pub const DEFAULT_STOCK: &str = "../Burnout Paradise Remastered/WORLDCOL.BIN";

/// One unit's polygons.
pub type Unit = (u32, Vec<CollisionPoly>);

/// The collision units in `path`, sorted by unit number.
pub fn load(path: &Path) -> Result<Vec<Unit>> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcx")) {
        let stock = std::env::var_os("BPX_STOCK_COL").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_STOCK));
        let stock = if stock.is_dir() { stock.join("WORLDCOL.BIN") } else { stock };
        return load_pcx(path, &stock);
    }
    let path = if path.is_dir() { path.join("WORLDCOL.BIN") } else { path.to_path_buf() };
    load_bundle(&path)
}

fn load_bundle(path: &Path) -> Result<Vec<Unit>> {
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

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
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
    let stock_units: HashMap<u32, Vec<CollisionPoly>> = load(stock)?.into_iter().collect();
    let combined_units = load(combined)?;
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
    for ((ua, pa), (ub, pb)) in back.iter().zip(&combined_units) {
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

fn load_pcx(path: &Path, stock: &Path) -> Result<Vec<Unit>> {
    let mut data = Vec::new();
    ZlibDecoder::new(std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?).read_to_end(&mut data)?;
    let stock_units: HashMap<u32, Vec<CollisionPoly>> = load_bundle(stock)
        .with_context(|| format!("{} needs Burnout's own collision (BPX_STOCK_COL)", path.display()))?
        .into_iter()
        .collect();
    let mut r = Reader(&data, 0);
    if r.take(4)? != MAGIC {
        bail!("{} is not a collision extra file", path.display());
    }
    let n = r.u32()?;
    let mut out = Vec::with_capacity(n as usize);
    let empty = Vec::new();
    for _ in 0..n {
        let unit = r.u32()?;
        let base = stock_units.get(&unit).unwrap_or(&empty);
        let runs = r.u32()?;
        let mut polys = Vec::new();
        for _ in 0..runs {
            match r.u8()? {
                RUN_STOCK => {
                    let (start, len) = (r.u32()? as usize, r.u32()? as usize);
                    let part = base.get(start..start + len).with_context(|| {
                        format!("unit {unit}: Burnout's collision doesn't match (is it Burnout Paradise Remastered's WORLDCOL.BIN?)")
                    })?;
                    polys.extend_from_slice(part);
                }
                RUN_ADDED => {
                    for _ in 0..r.u32()? {
                        let tag = r.u32()?;
                        let nv = r.u8()? as usize;
                        let mut verts = Vec::with_capacity(nv);
                        for _ in 0..nv {
                            verts.push([r.f32()?, r.f32()?, r.f32()?]);
                        }
                        polys.push(CollisionPoly { verts, tag });
                    }
                }
                k => bail!("{}: unknown run kind {k}", path.display()),
            }
        }
        out.push((unit, polys));
    }
    out.sort_by_key(|u| u.0);
    Ok(out)
}
