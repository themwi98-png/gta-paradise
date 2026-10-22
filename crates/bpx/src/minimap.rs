//! GTA V's minimap (the radar and the pause map) for Paradise City, drawn
//! from Burnout's collision in GTA's own style.
//!
//! GTA draws the close-up radar from flat mesh tiles: x64e.rpf
//! levels/gta5/minimap.rpf/minimap_<col>_<row>.ydd, an 8 x 9 grid (rows from
//! the north), each tile a sea drawable (shades of blue-grey by distance from
//! the shore, z -12), a land drawable (z 0) and 3 x 3 drawables of buildings
//! (z 6) and roads (z 12), all plain vertex-coloured triangles. Zoomed out it
//! uses 2 x 3 images instead (scaleform_generic.rpf minimap_<row>_<col>.ytd
//! for the radar, minimap_sea_* for the pause map, and minimap_lod_128). Where
//! both sit is set in data/tune/minimap.ymt (Tiles, Bitmap); the images'
//! place is moved here to fit Paradise.
//!
//! Water isn't drawn (no sea drawables; see-through in the images): the
//! radar and pause map show through there.
//!
//! The map is first rasterised at RES m from the up-facing collision: land,
//! roads (asphalt), railways, water, and buildings (flat roofs of Burnout's
//! own pieces well above the ground around them; the added out-of-bounds
//! terrain is never a building). Empty cells the open sea can't reach become
//! land. Hand edits to the coast (tools/edits/minimap_edits.json, from the
//! viewer's Map coast panel: shapes that are land or sea) go on top. Each
//! tile is then traced into outlines (marching squares), simplified,
//! triangulated and cut into its drawables; the images are drawn from the
//! same raster. `coast_view.png` / `.json` in the output folder show the
//! viewer where the map has sea (to check the coast against the real one).

use crate::gta::Placement;
use anyhow::{Context, Result};
use bpworld as bw;
use contour::ContourBuilder;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// Raster cell size (m).
const RES: f64 = 2.0;
/// The radar's tile grid, 8 x 9 tiles: GTA's is 9400 x 12492 m from (-4500,
/// 8000); moved and widened here (in minimap.ymt) to take in Paradise's
/// roads (x -3949..5703) and Big Surf Island.
const TILES: [usize; 2] = [8, 9];
const GRID_START: [f64; 2] = [-4250.0, 8000.0];
const GRID_SIZE: [f64; 2] = [10000.0, 12492.0];
/// GTA's own grid: where its tiles' drawables (whose names ours take) lie.
const GTA_GRID_START: [f64; 2] = [-4500.0, 8000.0];
const GTA_TILE: [f64; 2] = [1175.0, 1388.0];
/// The raster: the tile grid, and east to the images' edge.
const RASTER_START: [f64; 2] = [-4500.0, 8000.0];
const RASTER_SIZE: [f64; 2] = [10400.0, 12492.0];
/// Each tile's buildings and roads are split into SUB x SUB drawables.
const SUB: usize = 3;
/// The zoomed-out images: 2 x 3 tiles of BITMAP_TILE m from BITMAP_START
/// (GTA's: 4500 m from (-4140, 8400)), each MINIMAP_PX (radar) and SEA_PX
/// (pause map) square.
const BITMAP_TILES: [usize; 2] = [2, 3];
const BITMAP_START: [f64; 2] = [-4100.0, 8000.0];
const BITMAP_TILE: f64 = 5000.0;
const MINIMAP_PX: usize = 512;
const SEA_PX: usize = 1024;
const LOD_PX: usize = 128;

const Z_LAND: f32 = 0.0;
const Z_BUILDING: f32 = 6.0;
const Z_RAIL: f32 = 11.0;
const Z_ROAD: f32 = 12.0;
/// GTA's radar colours.
const LAND: [u8; 3] = [29, 29, 29];
const BUILDING: [u8; 3] = [65, 65, 65];
const RAIL: [u8; 3] = [130, 130, 130];
const ROAD: [u8; 3] = [180, 180, 180];
/// The zoomed-out images' colours (GTA's): land, buildings, roads.
const IMG_LAND: [u8; 3] = [30, 30, 30];
const IMG_BUILDING: [u8; 3] = [41, 40, 41];
const IMG_ROAD: [u8; 3] = [181, 182, 181];

const SURFACE_RAIL: u8 = 16;
const SURFACE_FENCE: u8 = 7;
const ROADS: [u8; 2] = [0, 1];
/// Surfaces the ground is made of (for the height buildings stand out from).
const GROUND: [u8; 9] = [0, 1, 3, 4, 5, 11, 12, 18, 19];
/// Surfaces a building's roof can be (concrete, stone, metal, wood, plastic,
/// glass).
const ROOFS: [u8; 7] = [2, 6, 8, 9, 10, 13, 14];
/// A roof is this flat (normal's up component) and this high (m) above the
/// lowest ground within GROUND_REACH coarse cells of GROUND_CELL m.
const ROOF_FLAT: f64 = 0.9;
const ROOF_HEIGHT: f64 = 3.5;
const GROUND_CELL: f64 = 16.0;
const GROUND_REACH: usize = 2;
/// Up-facing: normal's up component at least this.
const UP: f64 = 0.5;
/// Pieces wider than this (m) are map-wide sea planes.
const MAP_WIDE: f64 = 1000.0;
/// Water counts as covered by land within this height of its surface (m):
/// banks and piers, not lake beds below it or bridges above.
const SHORE_BELOW: f64 = 0.5;
const SHORE_ABOVE: f64 = 3.0;
/// Bits of land smaller than this (m²) are dropped: buoys and props in the
/// sea would each get their own ring of shore shading.
const MIN_ISLAND: f64 = 600.0;
/// Outline simplification tolerance (m) and smallest kept ring (m²).
const SIMPLIFY: f64 = 0.6;
const MIN_AREA: f64 = 6.0;

const F_LAND: u8 = 1;
const F_ROAD: u8 = 2;
const F_RAIL: u8 = 4;
const F_BUILDING: u8 = 8;
const F_WATER: u8 = 16;

/// The map, rasterised: row 0 is the north edge.
struct Raster {
    w: usize,
    h: usize,
    /// West and north edges (GTA world).
    x0: f64,
    y1: f64,
    flags: Vec<u8>,
}

impl Raster {
    fn cell(&self, x: f64, y: f64) -> Option<usize> {
        let c = ((x - self.x0) / RES).floor();
        let r = ((self.y1 - y) / RES).floor();
        (c >= 0.0 && r >= 0.0 && (c as usize) < self.w && (r as usize) < self.h).then(|| r as usize * self.w + c as usize)
    }
}

/// One collision triangle in GTA's world.
struct Tri {
    p: [[f64; 3]; 3],
    surface: u8,
    generated: bool,
    up: f64,
}

/// `water`: GTA's water.xml for Paradise (out/water_paradise.xml), so the
/// map has water where the game does (with the hand water edits); without
/// it, Burnout's water collision.
///
/// `names`: GTA's own tiles' drawable names (tile,hash,min x,y,z,max x,y,z
/// rows, from `cwconv yddinfo`): each of our drawables takes the name of
/// GTA's in the same place.
pub fn export(col: &Path, out: &Path, placement: Placement, ymt: Option<&Path>, water: Option<&Path>, names: Option<&Path>) -> Result<()> {
    let off = [placement.offset[0] as f64, placement.offset[1] as f64];
    let tris = load(col, &placement)?;
    eprintln!("minimap: {} up-facing collision triangles", tris.len());
    let names = names.map(load_names).transpose()?.unwrap_or_default();
    let quads = water.map(load_water).transpose()?;
    let mut raster = rasterise(&tris, quads.as_deref());
    drop(tris);
    eprintln!("minimap: {} x {} raster at {RES} m", raster.w, raster.h);
    apply_coast_edits(&mut raster)?;
    write_coast_view(&raster, out)?;

    let tiles = out.join("tiles");
    std::fs::create_dir_all(&tiles)?;
    let start = [GRID_START[0] + off[0], GRID_START[1] + off[1]];
    let tile = [GRID_SIZE[0] / TILES[0] as f64, GRID_SIZE[1] / TILES[1] as f64];
    let mut triangles = 0usize;
    for i in 0..TILES[0] {
        for j in 0..TILES[1] {
            let x0 = start[0] + i as f64 * tile[0];
            let y1 = start[1] - j as f64 * tile[1];
            let name = format!("minimap_{i}_{j}");
            let rect = [x0, y1 - tile[1], x0 + tile[0], y1];
            let drawables = tile_drawables(&raster, &name, rect, names.get(&format!("{i}_{j}")));
            triangles += drawables.iter().map(|d| d.indices.len() / 3).sum::<usize>();
            std::fs::write(tiles.join(format!("{name}.ydd.xml")), ydd_xml(&drawables))?;
        }
    }
    eprintln!("minimap: {} tiles, {triangles} triangles", TILES[0] * TILES[1]);

    let bitmaps = out.join("bitmaps");
    std::fs::create_dir_all(&bitmaps)?;
    write_bitmaps(&raster, &bitmaps, [BITMAP_START[0] + off[0], BITMAP_START[1] + off[1]])?;

    if let Some(ymt) = ymt {
        let xml = std::fs::read_to_string(ymt).with_context(|| format!("reading {}", ymt.display()))?;
        let bitmap_start = [BITMAP_START[0] + off[0], BITMAP_START[1] + off[1]];
        let xml = set_vector(&xml, "vMiniMapWorldSize", GRID_SIZE)?;
        let xml = set_vector(&xml, "vMiniMapWorldStart", start)?;
        // Tiles where GTA's only drawable is its sea one hold Paradise in it;
        // the pause map leaves sea drawables out unless this is on.
        let xml = set_bool(&xml, "bDrawVectorSeaPaused", true)?;
        let xml = set_vector(&xml, "vBitmapTileSize", [BITMAP_TILE, BITMAP_TILE])?;
        let xml = set_vector(&xml, "vBitmapStart", bitmap_start)?;
        std::fs::write(out.join("minimap.ymt.pso.xml"), xml)?;
    }
    Ok(())
}

/// The viewer's hand edits to the map's coast (paradise-minimap-edits-v1):
/// `land` and `sea` shapes, each a list of GTA [x, y, z] points.
pub const DEFAULT_MINIMAP_EDITS: &str = "tools/edits/minimap_edits.json";

/// Cells inside a `land` shape become land, inside a `sea` shape sea (and
/// lose any buildings or railway; roads stay, as bridges). Sea shapes go
/// last, so a sea shape inside a land one cuts a hole.
fn apply_coast_edits(r: &mut Raster) -> Result<()> {
    let path = std::env::var_os("BPX_MINIMAP_EDITS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_MINIMAP_EDITS));
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(()) };
    let doc: serde_json::Value = serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    let shapes = |key: &str| -> Vec<Vec<[f64; 2]>> {
        doc[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_array().map(|pts| pts.iter().filter_map(|p| Some([p[0].as_f64()?, p[1].as_f64()?])).collect::<Vec<_>>()))
            .filter(|s| s.len() >= 3)
            .collect()
    };
    let mut changed = [0usize; 2];
    for (k, key) in ["land", "sea"].into_iter().enumerate() {
        for shape in shapes(key) {
            let (x0, x1) = shape.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
            let (y0, y1) = shape.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
            let c0 = ((x0 - r.x0) / RES).floor().max(0.0) as usize;
            let c1 = (((x1 - r.x0) / RES).ceil().max(0.0) as usize).min(r.w);
            let r0 = ((r.y1 - y1) / RES).floor().max(0.0) as usize;
            let r1 = (((r.y1 - y0) / RES).ceil().max(0.0) as usize).min(r.h);
            for row in r0..r1 {
                let y = r.y1 - (row as f64 + 0.5) * RES;
                for col in c0..c1 {
                    let x = r.x0 + (col as f64 + 0.5) * RES;
                    if !inside(&shape, x, y) {
                        continue;
                    }
                    let f = &mut r.flags[row * r.w + col];
                    *f = if k == 0 { (*f | F_LAND) & !F_WATER } else { *f & !(F_LAND | F_BUILDING | F_RAIL) };
                    changed[k] += 1;
                }
            }
        }
    }
    eprintln!("minimap: coast edits from {}: {} cells made land, {} made sea", path.display(), changed[0], changed[1]);
    Ok(())
}

/// Is (x, y) inside the polygon (even-odd)?
fn inside(shape: &[[f64; 2]], x: f64, y: f64) -> bool {
    let mut c = false;
    let mut j = shape.len() - 1;
    for i in 0..shape.len() {
        let (a, b) = (shape[i], shape[j]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            c = !c;
        }
        j = i;
    }
    c
}

/// Where the map has sea, for the viewer: coast_view.png (VIEW_CELL m a
/// pixel, north at the top; sea translucent magenta, so it stands out from
/// the world's own blue water; land clear) and
/// coast_view.json (its west and north edges, pixel size and size).
const VIEW_CELL: usize = 2;
fn write_coast_view(r: &Raster, out: &Path) -> Result<()> {
    let (w, h) = (r.w / VIEW_CELL, r.h / VIEW_CELL);
    let mut rgba = vec![0u8; w * h * 4];
    for row in 0..h {
        for col in 0..w {
            let land = (0..VIEW_CELL).any(|dr| (0..VIEW_CELL).any(|dc| r.flags[(row * VIEW_CELL + dr) * r.w + col * VIEW_CELL + dc] & F_LAND != 0));
            if !land {
                rgba[(row * w + col) * 4..][..4].copy_from_slice(&[255, 0, 200, 110]);
            }
        }
    }
    std::fs::create_dir_all(out)?;
    let file = std::io::BufWriter::new(std::fs::File::create(out.join("coast_view.png"))?);
    let mut enc = png::Encoder::new(file, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    let cell = RES * VIEW_CELL as f64;
    std::fs::write(out.join("coast_view.json"), format!("{{\"format\":\"paradise-coast-view-v1\",\"x0\":{},\"y1\":{},\"cell\":{cell},\"width\":{w},\"height\":{h}}}\n", r.x0, r.y1))?;
    eprintln!("minimap: coast view {w} x {h} at {cell} m");
    Ok(())
}

/// Sets `<name value=".." />` (a bool) in the minimap tuning XML.
fn set_bool(xml: &str, name: &str, v: bool) -> Result<String> {
    let open = format!("<{name} ");
    let at = xml.find(&open).with_context(|| format!("minimap.ymt has no {name}"))?;
    let end = at + xml[at..].find("/>").context("minimap.ymt: unclosed element")? + 2;
    Ok(format!("{}<{name} value=\"{v}\" />{}", &xml[..at], &xml[end..]))
}

/// Sets `<name x=".." y=".." />` in the minimap tuning XML.
fn set_vector(xml: &str, name: &str, v: [f64; 2]) -> Result<String> {
    let open = format!("<{name} ");
    let at = xml.find(&open).with_context(|| format!("minimap.ymt has no {name}"))?;
    let end = at + xml[at..].find("/>").context("minimap.ymt: unclosed element")? + 2;
    Ok(format!("{}<{name} x=\"{}\" y=\"{}\" />{}", &xml[..at], v[0], v[1], &xml[end..]))
}

/// GTA's names for one tile's drawables: the land one, the sea one and each
/// detail square's (index: column + SUB * row from the south). The game
/// draws a tile's drawables by these names only (others are never drawn).
#[derive(Default)]
struct TileNames {
    land: Option<u32>,
    sea: Option<u32>,
    subs: [Option<u32>; SUB * SUB],
}

fn load_names(path: &Path) -> Result<HashMap<String, TileNames>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut out: HashMap<String, TileNames> = HashMap::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 8 {
            continue;
        }
        let (tile, hash) = (f[0], u32::from_str_radix(f[1], 16)?);
        let v: Vec<f64> = f[2..8].iter().map(|x| x.parse()).collect::<Result<_, _>>()?;
        let mut ij = tile.split('_').map(|x| x.parse::<usize>());
        let (Some(Ok(i)), Some(Ok(j))) = (ij.next(), ij.next()) else { continue };
        let entry = out.entry(tile.to_string()).or_default();
        if v[2].abs() < 0.01 && v[5].abs() < 0.01 {
            entry.land = Some(hash);
        } else if v[5] <= -1.0 {
            entry.sea = Some(hash);
        } else {
            // A detail square: by its middle.
            let (x0, y0) = (GTA_GRID_START[0] + i as f64 * GTA_TILE[0], GTA_GRID_START[1] - (j + 1) as f64 * GTA_TILE[1]);
            let a = (((v[0] + v[3]) / 2.0 - x0) / (GTA_TILE[0] / SUB as f64)).floor().clamp(0.0, SUB as f64 - 1.0) as usize;
            let b = (((v[1] + v[4]) / 2.0 - y0) / (GTA_TILE[1] / SUB as f64)).floor().clamp(0.0, SUB as f64 - 1.0) as usize;
            entry.subs[b * SUB + a] = Some(hash);
        }
    }
    eprintln!("minimap: GTA's drawable names for {} tiles", out.len());
    Ok(out)
}

/// GTA water quads: min x, min y, max x, max y, height.
fn load_water(path: &Path) -> Result<Vec<[f64; 5]>> {
    let xml = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value = |item: &str, name: &str| -> Option<f64> {
        let key = format!("<{name} value=\"");
        let at = item.find(&key)? + key.len();
        item[at..].split('"').next()?.parse().ok()
    };
    let mut quads = Vec::new();
    let body = xml.split("</WaterQuads>").next().unwrap_or("");
    for item in body.split("<Item>").skip(1) {
        if let (Some(x0), Some(x1), Some(y0), Some(y1), Some(z)) = (value(item, "minX"), value(item, "maxX"), value(item, "minY"), value(item, "maxY"), value(item, "z")) {
            quads.push([x0, y0, x1, y1, z]);
        }
    }
    eprintln!("minimap: {} water quads from {}", quads.len(), path.display());
    Ok(quads)
}

/// The up-facing collision triangles (with the collision edits' deletions
/// applied), knowing which pieces were added (not in the stock collision).
fn load(col: &Path, placement: &Placement) -> Result<Vec<Tri>> {
    let units = crate::colsrc::load(col)?;
    let edits = crate::edits::CollisionEdits::load()?;
    let stock_keys = if col.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcx")) {
        let stock = std::env::var_os("BPX_STOCK_COL").map(std::path::PathBuf::from).unwrap_or_else(|| crate::colsrc::DEFAULT_STOCK.into());
        let stock = if stock.is_dir() { stock.join("WORLDCOL.BIN") } else { stock };
        Some(crate::collision::load_poly_keys(&stock)?)
    } else {
        None
    };
    let empty = HashSet::new();
    let mut tris = Vec::new();
    for (unit, polys, ids) in units {
        let stock = stock_keys.as_ref().map(|k| k.get(&unit).unwrap_or(&empty));
        for poly in edits.polys(unit, polys, &ids) {
            let tag = bw::CollisionTag::decode(poly.tag);
            let s = tag.surface_id;
            if s == bw::SURFACE_INVISIBLE_WALL || s == SURFACE_FENCE || poly.verts.len() < 3 {
                continue;
            }
            let generated = stock.is_some_and(|k| !k.contains(&crate::collision::poly_key(&poly)));
            let v: Vec<[f64; 3]> = poly.verts.iter().map(|&p| placement.apply(p).map(|c| c as f64)).collect();
            for k in 1..v.len() - 1 {
                let p = [v[0], v[k], v[k + 1]];
                let (minx, maxx) = (p.iter().map(|q| q[0]).fold(f64::MAX, f64::min), p.iter().map(|q| q[0]).fold(f64::MIN, f64::max));
                let (miny, maxy) = (p.iter().map(|q| q[1]).fold(f64::MAX, f64::min), p.iter().map(|q| q[1]).fold(f64::MIN, f64::max));
                if maxx - minx > MAP_WIDE || maxy - miny > MAP_WIDE {
                    continue;
                }
                let a = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
                let b = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
                let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len < 1e-9 {
                    continue;
                }
                let up = (n[2] / len).abs();
                if up >= UP {
                    tris.push(Tri { p, surface: s, generated, up });
                }
            }
        }
    }
    Ok(tris)
}

/// Calls `f` for every raster cell whose centre is inside the triangle (or,
/// for one smaller than a cell, the cell under its middle).
fn cover(r: &Raster, p: &[[f64; 3]; 3], mut f: impl FnMut(usize)) {
    let minx = p.iter().map(|q| q[0]).fold(f64::MAX, f64::min);
    let maxx = p.iter().map(|q| q[0]).fold(f64::MIN, f64::max);
    let miny = p.iter().map(|q| q[1]).fold(f64::MAX, f64::min);
    let maxy = p.iter().map(|q| q[1]).fold(f64::MIN, f64::max);
    let c0 = (((minx - r.x0) / RES - 0.5).ceil().max(0.0)) as usize;
    let c1 = (((maxx - r.x0) / RES - 0.5).floor()).min(r.w as f64 - 1.0);
    let r0 = (((r.y1 - maxy) / RES - 0.5).ceil().max(0.0)) as usize;
    let r1 = (((r.y1 - miny) / RES - 0.5).floor()).min(r.h as f64 - 1.0);
    let edge = |a: [f64; 3], b: [f64; 3], x: f64, y: f64| (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
    let mut hit = false;
    if c1 >= 0.0 && r1 >= 0.0 {
        for row in r0..=r1 as usize {
            let y = r.y1 - (row as f64 + 0.5) * RES;
            for col in c0..=c1 as usize {
                let x = r.x0 + (col as f64 + 0.5) * RES;
                let (e0, e1, e2) = (edge(p[0], p[1], x, y), edge(p[1], p[2], x, y), edge(p[2], p[0], x, y));
                if (e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0) || (e0 <= 0.0 && e1 <= 0.0 && e2 <= 0.0) {
                    f(row * r.w + col);
                    hit = true;
                }
            }
        }
    }
    if !hit {
        if let Some(i) = r.cell((p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][1] + p[1][1] + p[2][1]) / 3.0) {
            f(i);
        }
    }
}

fn rasterise(tris: &[Tri], quads: Option<&[[f64; 5]]>) -> Raster {
    let start = RASTER_START;
    let w = (RASTER_SIZE[0] / RES).ceil() as usize;
    let h = (RASTER_SIZE[1] / RES).ceil() as usize;
    // `grid` locates cells; the layers are filled in alongside it.
    let grid = Raster { w, h, x0: start[0], y1: start[1], flags: Vec::new() };
    let mut flags = vec![0u8; w * h];
    let mid_z = |t: &Tri| (t.p[0][2] + t.p[1][2] + t.p[2][2]) / 3.0;

    // The lowest ground nearby, for telling roofs from hills.
    let (gw, gh) = ((RASTER_SIZE[0] / GROUND_CELL).ceil() as usize, (RASTER_SIZE[1] / GROUND_CELL).ceil() as usize);
    let mut ground = vec![f64::INFINITY; gw * gh];
    let gcell = |x: f64, y: f64| -> Option<usize> {
        let c = ((x - start[0]) / GROUND_CELL).floor();
        let rr = ((start[1] - y) / GROUND_CELL).floor();
        (c >= 0.0 && rr >= 0.0 && (c as usize) < gw && (rr as usize) < gh).then(|| rr as usize * gw + c as usize)
    };
    for t in tris.iter().filter(|t| t.generated || GROUND.contains(&t.surface)) {
        for q in &t.p {
            if let Some(i) = gcell(q[0], q[1]) {
                ground[i] = ground[i].min(q[2]);
            }
        }
    }
    let ground = min_filter(&ground, gw, gh, GROUND_REACH);

    // Water first, so land can be judged against its surface: GTA's water
    // where given (its sea reaches under the land, hidden by the ground),
    // else Burnout's water collision. `wet` marks Burnout's water surfaces,
    // which tell a bridge over water from a street above the sea's level.
    let mut water_top = vec![f32::NEG_INFINITY; w * h];
    let mut wet = vec![false; w * h];
    for t in tris.iter().filter(|t| t.surface == bw::SURFACE_WATER) {
        let z = mid_z(t) as f32;
        cover(&grid, &t.p, |i| {
            wet[i] = true;
            if quads.is_none() {
                water_top[i] = water_top[i].max(z);
            }
        });
    }
    let mut quad_bounds = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    if let Some(quads) = quads {
        for q in quads {
            quad_bounds = [quad_bounds[0].min(q[0]), quad_bounds[1].min(q[1]), quad_bounds[2].max(q[2]), quad_bounds[3].max(q[3])];
            let c0 = ((q[0] - start[0]) / RES - 0.5).ceil().max(0.0) as usize;
            let c1 = (((q[2] - start[0]) / RES - 0.5).floor() + 1.0).clamp(0.0, w as f64) as usize;
            let r0 = ((start[1] - q[3]) / RES - 0.5).ceil().max(0.0) as usize;
            let r1 = (((start[1] - q[1]) / RES - 0.5).floor() + 1.0).clamp(0.0, h as f64) as usize;
            for row in r0..r1 {
                for col in c0..c1 {
                    let i = row * w + col;
                    water_top[i] = water_top[i].max(q[4] as f32);
                }
            }
        }
    }
    for (f, wt) in flags.iter_mut().zip(&water_top) {
        if wt.is_finite() {
            *f |= F_WATER;
        }
    }
    let mut land = vec![false; w * h];
    for t in tris.iter().filter(|t| t.surface != bw::SURFACE_WATER) {
        let z = mid_z(t);
        let mx = (t.p[0][0] + t.p[1][0] + t.p[2][0]) / 3.0;
        let my = (t.p[0][1] + t.p[1][1] + t.p[2][1]) / 3.0;
        let building = !t.generated
            && ROOFS.contains(&t.surface)
            && t.up >= ROOF_FLAT
            && gcell(mx, my).is_some_and(|g| ground[g].is_finite() && z > ground[g] + ROOF_HEIGHT);
        let mut mark = 0;
        if ROADS.contains(&t.surface) {
            mark |= F_ROAD;
        }
        if t.surface == SURFACE_RAIL {
            mark |= F_RAIL;
        }
        if building {
            mark |= F_BUILDING;
        }
        cover(&grid, &t.p, |i| {
            let wt = water_top[i] as f64;
            // Over water, what's at its edge is land (banks, piers), and
            // what's higher is too unless there's water surface below it
            // (a bridge); lake beds below the surface aren't.
            if !wt.is_finite() || (z >= wt - SHORE_BELOW && (z <= wt + SHORE_ABOVE || !wet[i])) {
                land[i] = true;
            }
            flags[i] |= mark;
        });
    }
    for (i, &l) in land.iter().enumerate() {
        if l {
            flags[i] = (flags[i] | F_LAND) & !F_WATER;
        }
    }
    // Within GTA's water's reach, where it has none the game shows ground.
    if quads.is_some() {
        for (i, f) in flags.iter_mut().enumerate() {
            let (x, y) = (start[0] + ((i % w) as f64 + 0.5) * RES, start[1] - ((i / w) as f64 + 0.5) * RES);
            let inside = x >= quad_bounds[0] && x <= quad_bounds[2] && y >= quad_bounds[1] && y <= quad_bounds[3];
            if inside && *f & (F_LAND | F_WATER) == 0 {
                *f |= F_LAND;
            }
        }
    }
    drop(water_top);
    drop(wet);
    drop(land);
    let mut r = Raster { flags, ..grid };
    drop_specks(&mut r);

    // Empty cells the open sea (the raster's edge) can't reach are land
    // (gaps in the collision), not water.
    let mut sea = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    for c in 0..w {
        stack.push(c);
        stack.push((h - 1) * w + c);
    }
    for rr in 0..h {
        stack.push(rr * w);
        stack.push(rr * w + w - 1);
    }
    while let Some(i) = stack.pop() {
        if sea[i] || r.flags[i] & F_LAND != 0 {
            continue;
        }
        sea[i] = true;
        let (c, rr) = (i % w, i / w);
        if c > 0 {
            stack.push(i - 1);
        }
        if c + 1 < w {
            stack.push(i + 1);
        }
        if rr > 0 {
            stack.push(i - w);
        }
        if rr + 1 < h {
            stack.push(i + w);
        }
    }
    for (f, &open_sea) in r.flags.iter_mut().zip(&sea) {
        if *f & F_LAND == 0 && *f & F_WATER == 0 && !open_sea {
            *f |= F_LAND;
        }
        if *f & F_LAND == 0 {
            *f &= !F_BUILDING;
        }
    }
    drop(sea);
    r
}

/// Clears land (and what's on it) in pieces smaller than MIN_ISLAND.
fn drop_specks(r: &mut Raster) {
    let (w, h) = (r.w, r.h);
    let min_cells = (MIN_ISLAND / (RES * RES)).ceil() as usize;
    let mut seen = vec![false; w * h];
    let mut piece = Vec::new();
    let mut dropped = 0usize;
    for s in 0..w * h {
        if seen[s] || r.flags[s] & F_LAND == 0 {
            continue;
        }
        piece.clear();
        let mut stack = vec![s];
        seen[s] = true;
        while let Some(i) = stack.pop() {
            piece.push(i);
            let (c, row) = (i % w, i / w);
            let mut visit = |j: usize| {
                if !seen[j] && r.flags[j] & F_LAND != 0 {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if c > 0 {
                visit(i - 1);
            }
            if c + 1 < w {
                visit(i + 1);
            }
            if row > 0 {
                visit(i - w);
            }
            if row + 1 < h {
                visit(i + w);
            }
        }
        if piece.len() < min_cells {
            for &i in &piece {
                r.flags[i] &= !(F_LAND | F_BUILDING | F_RAIL);
            }
            dropped += 1;
        }
    }
    eprintln!("minimap: dropped {dropped} specks of land under {MIN_ISLAND} m²");
}

/// Each cell's minimum over the (2 reach + 1)² cells around it.
fn min_filter(v: &[f64], w: usize, h: usize, reach: usize) -> Vec<f64> {
    let mut rows = vec![f64::INFINITY; w * h];
    for r in 0..h {
        for c in 0..w {
            let (a, b) = (c.saturating_sub(reach), (c + reach).min(w - 1));
            rows[r * w + c] = v[r * w + a..=r * w + b].iter().copied().fold(f64::INFINITY, f64::min);
        }
    }
    let mut out = vec![f64::INFINITY; w * h];
    for r in 0..h {
        let (a, b) = (r.saturating_sub(reach), (r + reach).min(h - 1));
        for c in 0..w {
            out[r * w + c] = (a..=b).map(|rr| rows[rr * w + c]).fold(f64::INFINITY, f64::min);
        }
    }
    out
}



/// One drawable's triangles (vertex positions, colours, indices).
#[derive(Default)]
struct Mesh {
    name: String,
    verts: Vec<([f32; 3], [u8; 3])>,
    index: HashMap<([i64; 3], [u8; 3]), u32>,
    indices: Vec<u32>,
}

impl Mesh {
    fn named(name: String) -> Self {
        Mesh { name, ..Default::default() }
    }

    fn vertex(&mut self, p: [f64; 2], z: f32, colour: [u8; 3]) -> u32 {
        let key = ([(p[0] * 1000.0).round() as i64, (p[1] * 1000.0).round() as i64, (z * 1000.0) as i64], colour);
        let next = self.verts.len() as u32;
        *self.index.entry(key).or_insert_with(|| {
            self.verts.push(([p[0] as f32, p[1] as f32, z], colour));
            next
        })
    }

    /// Takes over another mesh's triangles.
    fn absorb(&mut self, other: Mesh) {
        for tri in other.indices.chunks(3) {
            for &i in tri {
                let (p, c) = other.verts[i as usize];
                let key = ([(p[0] as f64 * 1000.0).round() as i64, (p[1] as f64 * 1000.0).round() as i64, (p[2] * 1000.0) as i64], c);
                let next = self.verts.len() as u32;
                let j = *self.index.entry(key).or_insert_with(|| {
                    self.verts.push((p, c));
                    next
                });
                self.indices.push(j);
            }
        }
    }

    fn triangle(&mut self, t: [[f64; 2]; 3], z: f32, colour: [u8; 3]) {
        for p in t {
            let i = self.vertex(p, z, colour);
            self.indices.push(i);
        }
    }
}

/// A tile's drawables: its land, then SUB x SUB of buildings, railways and
/// roads. They take the names of GTA's in the same place; what has no GTA
/// counterpart (Los Santos had nothing there) goes into the land drawable,
/// or whichever GTA name the tile has. `rect` is min x, min y, max x, max y.
fn tile_drawables(r: &Raster, name: &str, rect: [f64; 4], gta: Option<&TileNames>) -> Vec<Mesh> {
    // The raster around the tile, with a margin so outlines close outside it.
    const MARGIN: usize = 3;
    let c0 = (((rect[0] - r.x0) / RES).floor() as isize - MARGIN as isize).max(0) as usize;
    let c1 = ((((rect[2] - r.x0) / RES).ceil() as usize) + MARGIN).min(r.w);
    let r0 = (((r.y1 - rect[3]) / RES).floor() as isize - MARGIN as isize).max(0) as usize;
    let r1 = ((((r.y1 - rect[1]) / RES).ceil() as usize) + MARGIN).min(r.h);
    let (w, h) = (c1 - c0, r1 - r0);
    // Traced in grid units (the tracer tells outlines from holes by which
    // way they turn, so the y axis mustn't be flipped), then placed.
    let builder = ContourBuilder::new(w, h, true);
    let (ox, oy) = (r.x0 + (c0 as f64 + 0.5) * RES, r.y1 - (r0 as f64 + 0.5) * RES);
    let place = move |p: [f64; 2]| [ox + p[0] * RES, oy - p[1] * RES];
    let window = |f: &dyn Fn(usize) -> f64| -> Vec<f64> {
        let mut v = Vec::with_capacity(w * h);
        for row in r0..r1 {
            for col in c0..c1 {
                v.push(f(row * r.w + col));
            }
        }
        v
    };
    let flag = |bit: u8| window(&|i| if r.flags[i] & bit != 0 { 1.0 } else { 0.0 });

    let mut land = Mesh::named(format!("{name}_land"));
    for t in outline(&builder, &smooth(&flag(F_LAND), w, h), &place) {
        add_clipped(&mut [&mut land], &[rect], t, Z_LAND, LAND);
    }

    let subs: Vec<[f64; 4]> = (0..SUB * SUB)
        .map(|k| {
            let (sx, sy) = ((rect[2] - rect[0]) / SUB as f64, (rect[3] - rect[1]) / SUB as f64);
            let (a, b) = ((k % SUB) as f64, (k / SUB) as f64);
            [rect[0] + a * sx, rect[1] + b * sy, rect[0] + (a + 1.0) * sx, rect[1] + (b + 1.0) * sy]
        })
        .collect();
    let mut detail: Vec<Mesh> = (0..SUB * SUB).map(|k| Mesh::named(format!("{name}_{k}"))).collect();
    let rail = dilate(&flag(F_RAIL), w, h);
    let layers = [(smooth(&flag(F_BUILDING), w, h), Z_BUILDING, BUILDING), (smooth(&rail, w, h), Z_RAIL, RAIL), (smooth(&flag(F_ROAD), w, h), Z_ROAD, ROAD)];
    for (field, z, colour) in &layers {
        for t in outline(&builder, field, &place) {
            let mut meshes: Vec<&mut Mesh> = detail.iter_mut().collect();
            add_clipped(&mut meshes, &subs, t, *z, *colour);
        }
    }
    let Some(gta) = gta else {
        let mut out = vec![land];
        out.extend(detail);
        out.retain(|m| !m.indices.is_empty());
        return out;
    };
    let hash = |h: u32| format!("hash_{h:08X}");
    // The catch-all: GTA's land drawable, else its sea one, else any.
    let catch_all = gta.land.or(gta.sea).or_else(|| gta.subs.iter().flatten().next().copied());
    let Some(catch_all) = catch_all else { return Vec::new() };
    land.name = hash(catch_all);
    let mut out = Vec::new();
    for (k, mut d) in detail.into_iter().enumerate() {
        match gta.subs[k] {
            Some(h) if h != catch_all => {
                d.name = hash(h);
                out.push(d);
            }
            _ => land.absorb(d),
        }
    }
    out.insert(0, land);
    out.retain(|m| !m.indices.is_empty());
    out
}

/// 3 x 3 box blur, so outlines come out smooth and specks vanish.
fn smooth(v: &[f64], w: usize, h: usize) -> Vec<f64> {
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                        s += v[yy as usize * w + xx as usize];
                    }
                }
            }
            out[y * w + x] = s / 9.0;
        }
    }
    out
}

/// Grows a mask by one cell (thin railways would vanish in `smooth`).
fn dilate(v: &[f64], w: usize, h: usize) -> Vec<f64> {
    let mut out = v.to_vec();
    for y in 0..h {
        for x in 0..w {
            if v[y * w + x] > 0.5 {
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                        out[yy as usize * w + xx as usize] = 1.0;
                    }
                }
            }
        }
    }
    out
}

/// The triangles of a mask's outline (value 0.5).
fn outline(builder: &ContourBuilder, field: &[f64], place: &dyn Fn([f64; 2]) -> [f64; 2]) -> Vec<[[f64; 2]; 3]> {
    match builder.contours(field, &[0.5]) {
        Ok(contours) => contours.iter().flat_map(|c| triangulate(c.geometry(), place)).collect(),
        Err(_) => Vec::new(),
    }
}

/// Places (grid units -> world), simplifies and triangulates polygons with
/// holes.
fn triangulate(mp: &geo_types::MultiPolygon<f64>, place: &dyn Fn([f64; 2]) -> [f64; 2]) -> Vec<[[f64; 2]; 3]> {
    let mut out = Vec::new();
    for poly in &mp.0 {
        let ext = simplify_ring(&poly.exterior().0.iter().map(|c| place([c.x, c.y])).collect::<Vec<_>>());
        if ext.len() < 3 || ring_area(&ext) < MIN_AREA {
            continue;
        }
        let mut coords: Vec<f64> = ext.iter().flat_map(|p| [p[0], p[1]]).collect();
        let mut holes = Vec::new();
        for hole in poly.interiors() {
            let ring = simplify_ring(&hole.0.iter().map(|c| place([c.x, c.y])).collect::<Vec<_>>());
            if ring.len() < 3 || ring_area(&ring) < MIN_AREA {
                continue;
            }
            holes.push(coords.len() / 2);
            coords.extend(ring.iter().flat_map(|p| [p[0], p[1]]));
        }
        if let Ok(idx) = earcutr::earcut(&coords, &holes, 2) {
            let p = |i: usize| [coords[2 * i], coords[2 * i + 1]];
            out.extend(idx.chunks(3).map(|t| [p(t[0]), p(t[1]), p(t[2])]));
        }
    }
    out
}

/// A closed ring (first point repeated or not) simplified with
/// Ramer-Douglas-Peucker, without the repeated end point.
fn simplify_ring(ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = ring.to_vec();
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    if pts.len() < 4 {
        return pts;
    }
    // Split at the point farthest from the first, simplify both halves.
    let far = (1..pts.len()).max_by(|&a, &b| dist2(pts[0], pts[a]).total_cmp(&dist2(pts[0], pts[b]))).unwrap();
    let mut a = rdp(&pts[..=far]);
    let mut second: Vec<[f64; 2]> = pts[far..].to_vec();
    second.push(pts[0]);
    let b = rdp(&second);
    a.pop();
    a.extend(&b[..b.len() - 1]);
    a
}

fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn rdp(pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let len = dist2(a, b).sqrt();
    let (mut worst, mut at) = (0.0, 0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = if len < 1e-9 { dist2(a, *p).sqrt() } else { ((b[0] - a[0]) * (a[1] - p[1]) - (a[0] - p[0]) * (b[1] - a[1])).abs() / len };
        if d > worst {
            worst = d;
            at = i;
        }
    }
    if worst <= SIMPLIFY {
        return vec![a, b];
    }
    let mut left = rdp(&pts[..=at]);
    left.pop();
    left.extend(rdp(&pts[at..]));
    left
}

fn ring_area(r: &[[f64; 2]]) -> f64 {
    let mut s = 0.0;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[(i + 1) % r.len()]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    s.abs() / 2.0
}

/// Adds the parts of a triangle inside each rectangle to that rectangle's
/// mesh (Sutherland-Hodgman, then a fan).
fn add_clipped(meshes: &mut [&mut Mesh], rects: &[[f64; 4]], t: [[f64; 2]; 3], z: f32, colour: [u8; 3]) {
    let (minx, maxx) = (t.iter().map(|p| p[0]).fold(f64::MAX, f64::min), t.iter().map(|p| p[0]).fold(f64::MIN, f64::max));
    let (miny, maxy) = (t.iter().map(|p| p[1]).fold(f64::MAX, f64::min), t.iter().map(|p| p[1]).fold(f64::MIN, f64::max));
    for (mesh, r) in meshes.iter_mut().zip(rects) {
        if maxx <= r[0] || minx >= r[2] || maxy <= r[1] || miny >= r[3] {
            continue;
        }
        if minx >= r[0] && maxx <= r[2] && miny >= r[1] && maxy <= r[3] {
            mesh.triangle(t, z, colour);
            continue;
        }
        let mut poly: Vec<[f64; 2]> = t.to_vec();
        for (axis, bound, keep_above) in [(0, r[0], true), (0, r[2], false), (1, r[1], true), (1, r[3], false)] {
            let inside = |p: &[f64; 2]| if keep_above { p[axis] >= bound } else { p[axis] <= bound };
            let mut next = Vec::with_capacity(poly.len() + 2);
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if inside(&a) {
                    next.push(a);
                }
                if inside(&a) != inside(&b) {
                    let s = (bound - a[axis]) / (b[axis] - a[axis]);
                    next.push([a[0] + (b[0] - a[0]) * s, a[1] + (b[1] - a[1]) * s]);
                }
            }
            poly = next;
            if poly.len() < 3 {
                break;
            }
        }
        for k in 1..poly.len().saturating_sub(1) {
            mesh.triangle([poly[0], poly[k], poly[k + 1]], z, colour);
        }
    }
}

/// CodeWalker XML for a drawable dictionary of minimap meshes (laid out like
/// GTA's own tiles).
fn ydd_xml(meshes: &[Mesh]) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DrawableDictionary>\n");
    for m in meshes {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for (p, _) in &m.verts {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let radius = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt() / 2.0;
        let _ = write!(
            s,
            " <Item>\n  <Name>{}</Name>\n  <BoundingSphereCenter x=\"{}\" y=\"{}\" z=\"{}\" />\n  <BoundingSphereRadius value=\"{radius}\" />\n  <BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" />\n  <BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" />\n",
            m.name, centre[0], centre[1], centre[2], lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]
        );
        for lod in ["High", "Med", "Low", "Vlow"] {
            let _ = writeln!(s, "  <LodDist{lod} value=\"9998\" />");
        }
        s.push_str("  <FlagsHigh value=\"1\" />\n  <FlagsMed value=\"0\" />\n  <FlagsLow value=\"0\" />\n  <FlagsVlow value=\"0\" />\n");
        s.push_str("  <ShaderGroup>\n   <Shaders>\n    <Item>\n     <Name>hash_3D88F3F5</Name>\n     <FileName>hash_EACC5E6F</FileName>\n     <RenderBucket value=\"0\" />\n     <Parameters>\n      <Item name=\"useTessellation\" type=\"Vector\" x=\"0\" y=\"0\" z=\"0\" w=\"0\" />\n     </Parameters>\n    </Item>\n   </Shaders>\n  </ShaderGroup>\n");
        s.push_str("  <DrawableModelsHigh>\n   <Item>\n    <RenderMask value=\"255\" />\n    <Flags value=\"0\" />\n    <HasSkin value=\"0\" />\n    <BoneIndex value=\"0\" />\n    <Unknown1 value=\"0\" />\n    <Geometries>\n");
        // 16-bit indices: at most 65535 vertices per geometry.
        let mut start = 0;
        while start < m.indices.len() {
            let mut remap: HashMap<u32, u16> = HashMap::new();
            let mut verts: Vec<u32> = Vec::new();
            let mut idx: Vec<u16> = Vec::new();
            let mut end = start;
            while end < m.indices.len() {
                let tri = &m.indices[end..end + 3];
                let new = tri.iter().filter(|v| !remap.contains_key(v)).count();
                if verts.len() + new > 65535 {
                    break;
                }
                for &v in tri {
                    let n = *remap.entry(v).or_insert_with(|| {
                        verts.push(v);
                        (verts.len() - 1) as u16
                    });
                    idx.push(n);
                }
                end += 3;
            }
            let mut glo = [f32::MAX; 3];
            let mut ghi = [f32::MIN; 3];
            for &v in &verts {
                let p = m.verts[v as usize].0;
                for k in 0..3 {
                    glo[k] = glo[k].min(p[k]);
                    ghi[k] = ghi[k].max(p[k]);
                }
            }
            let _ = write!(
                s,
                "     <Item>\n      <ShaderIndex value=\"0\" />\n      <BoundingBoxMin x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />\n      <BoundingBoxMax x=\"{}\" y=\"{}\" z=\"{}\" w=\"{}\" />\n      <VertexBuffer>\n       <Flags value=\"0\" />\n       <Layout type=\"GTAV1\">\n        <Position />\n        <Colour0 />\n       </Layout>\n       <Data>\n",
                glo[0], glo[1], glo[2], glo[0], ghi[0], ghi[1], ghi[2], ghi[0]
            );
            for &v in &verts {
                let (p, c) = m.verts[v as usize];
                let _ = writeln!(s, "        {} {} {}   {} {} {} 255", p[0], p[1], p[2], c[0], c[1], c[2]);
            }
            s.push_str("       </Data>\n      </VertexBuffer>\n      <IndexBuffer>\n       <Data>\n");
            for chunk in idx.chunks(24) {
                let line: Vec<String> = chunk.iter().map(|i| i.to_string()).collect();
                let _ = writeln!(s, "        {}", line.join(" "));
            }
            s.push_str("       </Data>\n      </IndexBuffer>\n     </Item>\n");
            start = end;
        }
        s.push_str("    </Geometries>\n   </Item>\n  </DrawableModelsHigh>\n  <Lights />\n </Item>\n");
    }
    s.push_str("</DrawableDictionary>\n");
    s
}

/// The zoomed-out images: minimap_<row>_<col> (radar), minimap_sea_<row>_<col>
/// (pause map, larger) and minimap_lod_128 (the whole image area,
/// smallest); water is see-through in all of them.
fn write_bitmaps(r: &Raster, dir: &Path, start: [f64; 2]) -> Result<()> {
    for row in 0..BITMAP_TILES[1] {
        for col in 0..BITMAP_TILES[0] {
            let x0 = start[0] + col as f64 * BITMAP_TILE;
            let y1 = start[1] - row as f64 * BITMAP_TILE;
            let area = [x0, y1 - BITMAP_TILE, x0 + BITMAP_TILE, y1];
            write_image(dir, &format!("minimap_{row}_{col}"), &draw(r, area, MINIMAP_PX))?;
            write_image(dir, &format!("minimap_sea_{row}_{col}"), &draw(r, area, SEA_PX))?;
        }
    }
    // The lod image: the whole image area, centred in a square.
    let (wd, ht) = (BITMAP_TILES[0] as f64 * BITMAP_TILE, BITMAP_TILES[1] as f64 * BITMAP_TILE);
    let side = wd.max(ht);
    let (cx, cy) = (start[0] + wd / 2.0, start[1] - ht / 2.0);
    write_image(dir, "minimap_lod_128", &draw(r, [cx - side / 2.0, cy - side / 2.0, cx + side / 2.0, cy + side / 2.0], LOD_PX))?;
    Ok(())
}

/// An image of `area` (min x, min y, max x, max y), `px` square, each pixel
/// the average of the raster cells in it.
fn draw(r: &Raster, area: [f64; 4], px: usize) -> Vec<u8> {
    let step = (area[2] - area[0]) / px as f64;
    let samples = ((step / RES).round() as usize).clamp(1, 8);
    let mut rgba = vec![0u8; px * px * 4];
    for y in 0..px {
        for x in 0..px {
            let mut sum = [0.0f64; 4];
            for sy in 0..samples {
                for sx in 0..samples {
                    let wx = area[0] + (x as f64 + (sx as f64 + 0.5) / samples as f64) * step;
                    let wy = area[3] - (y as f64 + (sy as f64 + 0.5) / samples as f64) * step;
                    let (rgb, a) = pixel(r, wx, wy);
                    for k in 0..3 {
                        sum[k] += rgb[k] as f64 * a;
                    }
                    sum[3] += a;
                }
            }
            let n = (samples * samples) as f64;
            let i = (y * px + x) * 4;
            for k in 0..3 {
                rgba[i + k] = if sum[3] > 0.0 { (sum[k] / sum[3]).round() as u8 } else { 255 };
            }
            rgba[i + 3] = (sum[3] / n * 255.0).round() as u8;
        }
    }
    rgba
}

/// One point's colour and opacity in the images (water and beyond the map:
/// see-through).
fn pixel(r: &Raster, x: f64, y: f64) -> ([u8; 3], f64) {
    let Some(i) = r.cell(x, y) else { return ([0, 0, 0], 0.0) };
    let f = r.flags[i];
    if f & F_ROAD != 0 {
        return (IMG_ROAD, 1.0);
    }
    if f & F_LAND != 0 {
        return (if f & F_BUILDING != 0 { IMG_BUILDING } else { IMG_LAND }, 1.0);
    }
    ([0, 0, 0], 0.0)
}

/// Writes `<name>.dds` (uncompressed) and its one-texture `<name>.ytd.xml`.
fn write_image(dir: &Path, name: &str, rgba: &[u8]) -> Result<()> {
    let px = ((rgba.len() / 4) as f64).sqrt() as u32;
    crate::gtavis::write_rgba_dds(&dir.join(format!("{name}.dds")), px, px, rgba)?;
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<TextureDictionary>\n <Item>\n  <Name>{name}</Name>\n  <Unk32 value=\"48\" />\n  <Usage>DEFAULT</Usage>\n  <UsageFlags>UNK24</UsageFlags>\n  <ExtraFlags value=\"0\" />\n  <Width value=\"{px}\" />\n  <Height value=\"{px}\" />\n  <MipLevels value=\"1\" />\n  <Format>D3DFMT_A8R8G8B8</Format>\n  <FileName>{name}.dds</FileName>\n </Item>\n</TextureDictionary>\n"
    );
    std::fs::write(dir.join(format!("{name}.ytd.xml")), xml)?;
    Ok(())
}
