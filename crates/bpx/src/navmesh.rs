//! GTA V pedestrian navmeshes (ynv) generated from Paradise City's collision.
//!
//! GTA's navmesh is a grid of 150 m tiles from (-6000, -6000) (100 x 100,
//! file `navmesh[3*cx][3*cy].ynv`, area id `cy * 100 + cx`). Each tile is a
//! list of convex polygons (counter-clockwise seen from above); edge k runs
//! from vertex k to k+1 and names the polygon across it as `area:index`
//! (16383 = none). Polygons sharing an edge only partly are fine.
//!
//! Every tile is built with Recast (the `rerecast` port) from the placed
//! Burnout collision around it. Surfaces become Recast areas so polygons
//! never mix them, and each area gets the flags Los Santos uses for it
//! (measured from its downtown navmesh): Burnout pavement is footpath (where
//! GTA spawns pedestrians), asphalt is road, other walkable ground is plain.
//! Edges on a tile border are joined afterwards to the neighbouring tile's
//! polygon with the most overlap.

use crate::gta::Placement;
use anyhow::Result;
use bpworld::{self as bw, CollisionTag};
use glam::{UVec3, Vec3, Vec3A};
use rayon::prelude::*;
use rerecast::{Aabb3d, AreaType, BuildContoursFlags, ConvexVolume, HeightfieldBuilder, PolygonNavmesh, TriMesh};
use std::fmt::Write as _;
use std::path::Path;

const TILE: f32 = 150.0;
const GRID_ORIGIN: f32 = -6000.0;
const GRID_CELLS: i32 = 100;
const NO_POLY: u32 = 16383;

// Recast settings (metres unless noted), for a GTA pedestrian.
const CELL_SIZE: f32 = 0.3;
const CELL_HEIGHT: f32 = 0.2;
const AGENT_HEIGHT: f32 = 1.8;
const AGENT_RADIUS: f32 = 0.4;
const AGENT_CLIMB: f32 = 0.6;
const MAX_SLOPE_DEG: f32 = 45.0;
const MAX_EDGE_LEN: f32 = 12.0;
const MAX_SIMPLIFICATION_ERROR: f32 = 1.3;
/// Region sizes in cells.
const MIN_REGION_AREA: u16 = 8;
const MERGE_REGION_AREA: u16 = 400;
const MAX_VERTS_PER_POLY: u16 = 6;

/// Recast areas for our surface classes.
const AREA_FOOTPATH: u8 = 1;
const AREA_ROAD: u8 = 2;
const AREA_GROUND: u8 = 3;

/// GTA polygon flag bytes (the first four of `<Flags>`) per area, copied
/// from the most common Los Santos values: footpath bit (4) + pedestrian
/// density 3 for pavement, road bit for roads, plain ground otherwise.
fn gta_flags(area: u8) -> [u8; 4] {
    match area {
        AREA_FOOTPATH => [4, 0, 97, 0],
        AREA_ROAD => [0, 0, 2, 0],
        _ => [0, 0, 1, 0],
    }
}

fn surface_area(surface_id: u8) -> u8 {
    match bw::surface_name(surface_id) {
        "pavement" | "cobbles" => AREA_FOOTPATH,
        "asphalt_road" | "asphalt_island" => AREA_ROAD,
        _ => AREA_GROUND,
    }
}

/// A finished polygon in GTA coordinates.
struct Poly {
    verts: Vec<[f32; 3]>,
    /// Per edge: neighbour (area id, poly index), or a tile-border marker.
    edges: Vec<Edge>,
    area: u8,
}

#[derive(Clone, Copy)]
enum Edge {
    None,
    Inner(u32),
    /// On the tile border, towards the neighbouring tile (dx, dy).
    Border(i32, i32),
    Outer(u32, u32),
}

struct Tile {
    cx: i32,
    cy: i32,
    polys: Vec<Poly>,
}

impl Tile {
    fn area_id(&self) -> u32 {
        (self.cy * GRID_CELLS + self.cx) as u32
    }
}

/// Placed collision triangles with their Recast area (0: not walkable).
struct Source {
    /// Hand-drawn sidewalk changes (tools/edits/sidewalk_edits.json).
    sidewalks: crate::edits::SidewalkEdits,
    tris: Vec<([[f32; 3]; 3], u8)>,
    /// Triangle indices per tile (with margin).
    buckets: std::collections::HashMap<(i32, i32), Vec<u32>>,
}

fn load_source(col: &Path, placement: Placement, margin: f32) -> Result<Source> {
    let mut tris = Vec::new();
    let edits = crate::edits::CollisionEdits::load()?;
    for (unit, polys) in crate::colsrc::load(col)? {
        for p in edits.polys(unit, polys) {
            let tag = CollisionTag::decode(p.tag);
            let v: Vec<[f32; 3]> = p.verts.iter().map(|&v| placement.apply(v)).collect();
            // Water collision is the sea / lake bed: solid but not walkable.
            let area = if tag.surface_id == bw::SURFACE_WATER || tag.surface_id == bw::SURFACE_INVISIBLE_WALL { 0 } else { surface_area(tag.surface_id) };
            tris.push(([v[0], v[1], v[2]], area));
            if v.len() == 4 {
                tris.push(([v[0], v[2], v[3]], area));
            }
        }
    }
    let mut buckets: std::collections::HashMap<(i32, i32), Vec<u32>> = std::collections::HashMap::new();
    for (i, (t, _)) in tris.iter().enumerate() {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in t {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        let cell = |v: f32| ((v - GRID_ORIGIN) / TILE).floor() as i32;
        for cy in cell(lo[1] - margin)..=cell(hi[1] + margin) {
            for cx in cell(lo[0] - margin)..=cell(hi[0] + margin) {
                if (0..GRID_CELLS).contains(&cx) && (0..GRID_CELLS).contains(&cy) {
                    buckets.entry((cx, cy)).or_default().push(i as u32);
                }
            }
        }
    }
    Ok(Source { sidewalks: crate::edits::SidewalkEdits::load()?, tris, buckets })
}

/// Hand-drawn sidewalk changes for one tile: inside "remove" shapes the
/// footpath becomes plain ground; inside "add" shapes ground and road become
/// footpath. Shapes reach 3 m above and below their drawn points.
fn apply_sidewalk_edits(chf: &mut rerecast::CompactHeightfield, edits: &crate::edits::SidewalkEdits, x0: f32, y0: f32) {
    let overlaps = |shape: &[[f32; 3]]| {
        let (lo_x, hi_x) = shape.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
        let (lo_y, hi_y) = shape.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
        hi_x >= x0 - 10.0 && lo_x <= x0 + TILE + 10.0 && hi_y >= y0 - 10.0 && lo_y <= y0 + TILE + 10.0
    };
    let volume = |shape: &[[f32; 3]], area: u8| {
        let (lo_z, hi_z) = shape.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[2]), a.1.max(p[2])));
        ConvexVolume {
            vertices: shape.iter().map(|p| glam::Vec2::new(p[0], -p[1])).collect(),
            min_y: lo_z - 3.0,
            max_y: hi_z + 3.0,
            area: AreaType(area),
        }
    };
    for shape in edits.remove.iter().filter(|s| overlaps(s)) {
        chf.replace_poly_area(&volume(shape, AREA_GROUND), &[AreaType(AREA_FOOTPATH)]);
    }
    for shape in edits.add.iter().filter(|s| overlaps(s)) {
        chf.replace_poly_area(&volume(shape, AREA_FOOTPATH), &[AreaType(AREA_GROUND), AreaType(AREA_ROAD)]);
    }
}

/// Recast coordinates are y-up: (x, y, z)_recast = (x, z, -y)_gta.
fn to_recast(p: [f32; 3]) -> Vec3A {
    Vec3A::new(p[0], p[2], -p[1])
}

/// Builds a tile, retrying with gentler contour simplification when Recast
/// rejects a contour (it fails the whole tile where C++ Recast skips it).
fn build_tile(src: &Source, cx: i32, cy: i32) -> Result<Option<Tile>> {
    let mut last = None;
    for &max_error in &[MAX_SIMPLIFICATION_ERROR, 0.9, 0.6, 0.3, 0.0] {
        match build_tile_with(src, cx, cy, max_error) {
            Ok(t) => return Ok(t),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap())
}

fn build_tile_with(src: &Source, cx: i32, cy: i32, max_error: f32) -> Result<Option<Tile>> {
    let Some(ids) = src.buckets.get(&(cx, cy)) else { return Ok(None) };
    if !ids.iter().any(|&i| src.tris[i as usize].1 != 0) {
        return Ok(None);
    }
    let walkable_radius = (AGENT_RADIUS / CELL_SIZE).ceil() as u16;
    let walkable_height = (AGENT_HEIGHT / CELL_HEIGHT).ceil() as u16;
    let walkable_climb = (AGENT_CLIMB / CELL_HEIGHT).floor() as u16;
    let border = walkable_radius + 3;
    let x0 = GRID_ORIGIN + cx as f32 * TILE;
    let y0 = GRID_ORIGIN + cy as f32 * TILE;

    let mut mesh = TriMesh::default();
    let (mut zlo, mut zhi) = (f32::MAX, f32::MIN);
    let slope_cos = MAX_SLOPE_DEG.to_radians().cos();
    for &i in ids {
        let (t, area) = &src.tris[i as usize];
        let base = mesh.vertices.len() as u32;
        for p in t {
            mesh.vertices.push(to_recast(*p));
            zlo = zlo.min(p[2]);
            zhi = zhi.max(p[2]);
        }
        // Recast winds walkable triangles counter-clockwise seen from +y;
        // GTA floors face up (counter-clockwise from above, z up), which
        // the y-up mapping mirrors, so flip.
        mesh.indices.push(UVec3::new(base, base + 2, base + 1));
        let n = {
            let (a, b, c) = (Vec3::from(to_recast(t[0])), Vec3::from(to_recast(t[2])), Vec3::from(to_recast(t[1])));
            (b - a).cross(c - a).normalize_or_zero()
        };
        let walkable = *area != 0 && n.y.abs() >= slope_cos;
        mesh.area_types.push(if walkable { AreaType(*area) } else { AreaType::NOT_WALKABLE });
        if walkable && n.y < 0.0 {
            // Downward-facing floor: flip so Recast sees it as ground.
            let last = mesh.indices.last_mut().unwrap();
            *last = UVec3::new(last.x, last.z, last.y);
        }
    }
    let pad = border as f32 * CELL_SIZE;
    let aabb = Aabb3d {
        min: Vec3::new(x0 - pad, zlo - 1.0, -(y0 + TILE) - pad),
        max: Vec3::new(x0 + TILE + pad, zhi + AGENT_HEIGHT + 1.0, -y0 + pad),
    };
    let mut hf = HeightfieldBuilder { aabb, cell_size: CELL_SIZE, cell_height: CELL_HEIGHT }.build()?;
    hf.rasterize_triangles(&mesh, walkable_climb)?;
    hf.filter_low_hanging_walkable_obstacles(walkable_climb);
    hf.filter_ledge_spans(walkable_height, walkable_climb);
    hf.filter_walkable_low_height_spans(walkable_height);
    let mut chf = hf.into_compact(walkable_height, walkable_climb)?;
    chf.erode_walkable_area(walkable_radius);
    apply_sidewalk_edits(&mut chf, &src.sidewalks, x0, y0);
    chf.build_distance_field();
    chf.build_regions(border, MIN_REGION_AREA, MERGE_REGION_AREA)?;
    let contours = chf.build_contours(
        max_error,
        (MAX_EDGE_LEN / CELL_SIZE) as u16,
        BuildContoursFlags::TESSELLATE_SOLID_WALL_EDGES | BuildContoursFlags::TESSELLATE_AREA_EDGES,
    );
    let pm = contours.into_polygon_mesh(MAX_VERTS_PER_POLY)?;
    let polys = convert_polys(&pm);
    if polys.is_empty() {
        return Ok(None);
    }
    Ok(Some(Tile { cx, cy, polys }))
}

/// Recast polygons -> GTA polygons (z-up, counter-clockwise from above).
fn convert_polys(pm: &PolygonNavmesh) -> Vec<Poly> {
    let nvp = pm.max_vertices_per_polygon as usize;
    let world = |i: u16| -> [f32; 3] {
        let v = pm.vertices[i as usize];
        let x = pm.aabb.min.x + v.x as f32 * pm.cell_size;
        let y = pm.aabb.min.y + v.y as f32 * pm.cell_height;
        let z = pm.aabb.min.z + v.z as f32 * pm.cell_size;
        [x, -z, y]
    };
    let mut out = Vec::with_capacity(pm.polygon_count());
    for p in 0..pm.polygon_count() {
        let idx = &pm.polygons[p * nvp..(p + 1) * nvp];
        let nb = &pm.polygon_neighbors[p * nvp..(p + 1) * nvp];
        let n = idx.iter().take_while(|&&i| i != PolygonNavmesh::NO_INDEX).count();
        if n < 3 {
            continue;
        }
        let mut verts: Vec<[f32; 3]> = idx[..n].iter().map(|&i| world(i)).collect();
        let mut edges: Vec<Edge> = nb[..n]
            .iter()
            .map(|&e| {
                if e == PolygonNavmesh::NO_CONNECTION {
                    Edge::None
                } else if e & 0x8000 != 0 {
                    // Recast portal directions: 0 -x, 1 +z, 2 +x, 3 -z
                    // (recast +z is GTA -y).
                    match e & 0xF {
                        0 => Edge::Border(-1, 0),
                        1 => Edge::Border(0, -1),
                        2 => Edge::Border(1, 0),
                        _ => Edge::Border(0, 1),
                    }
                } else {
                    Edge::Inner(e as u32)
                }
            })
            .collect();
        // Make counter-clockwise seen from above (z up).
        let signed: f32 = (0..n).map(|i| {
            let (a, b) = (verts[i], verts[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        }).sum();
        if signed < 0.0 {
            // Reversing the vertices moves edge (k -> k+1) to (n-2-k).
            verts.reverse();
            edges = (0..n).map(|k| edges[(2 * n - 2 - k) % n]).collect();
        }
        out.push(Poly { verts, edges, area: pm.areas[p].0 });
    }
    out
}

/// Joins tile-border edges to the neighbouring tile's polygon with the most
/// overlap along the shared border.
fn link_tiles(tiles: &mut [Tile]) {
    use std::collections::HashMap;
    let index: HashMap<(i32, i32), usize> = tiles.iter().enumerate().map(|(i, t)| ((t.cx, t.cy), i)).collect();
    // Border edges per tile and direction: (poly, edge, span along border, mean z).
    let spans = |t: &Tile, dir: (i32, i32)| -> Vec<(usize, usize, f32, f32, f32)> {
        let mut v = Vec::new();
        for (pi, p) in t.polys.iter().enumerate() {
            for (k, e) in p.edges.iter().enumerate() {
                if let Edge::Border(dx, dy) = *e {
                    if (dx, dy) == dir {
                        let (a, b) = (p.verts[k], p.verts[(k + 1) % p.verts.len()]);
                        let (s0, s1) = if dx != 0 { (a[1].min(b[1]), a[1].max(b[1])) } else { (a[0].min(b[0]), a[0].max(b[0])) };
                        v.push((pi, k, s0, s1, (a[2] + b[2]) * 0.5));
                    }
                }
            }
        }
        v
    };
    let mut links: Vec<(usize, usize, usize, u32, u32)> = Vec::new();
    for (ti, t) in tiles.iter().enumerate() {
        for dir in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let Some(&ni) = index.get(&(t.cx + dir.0, t.cy + dir.1)) else { continue };
            let theirs = spans(&tiles[ni], (-dir.0, -dir.1));
            for (pi, k, s0, s1, z) in spans(t, dir) {
                let best = theirs
                    .iter()
                    .filter(|o| (o.4 - z).abs() < AGENT_CLIMB * 2.0)
                    .map(|o| (o.0, s1.min(o.3) - s0.max(o.2)))
                    .filter(|&(_, overlap)| overlap > 0.05)
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((other, _)) = best {
                    links.push((ti, pi, k, tiles[ni].area_id(), other as u32));
                }
            }
        }
    }
    for (ti, pi, k, area, poly) in links {
        tiles[ti].polys[pi].edges[k] = Edge::Outer(area, poly);
    }
}

fn write_tile(t: &Tile, out_dir: &Path) -> Result<()> {
    let area = t.area_id();
    let x0 = GRID_ORIGIN + t.cx as f32 * TILE;
    let y0 = GRID_ORIGIN + t.cy as f32 * TILE;
    let zlo = t.polys.iter().flat_map(|p| p.verts.iter().map(|v| v[2])).fold(f32::MAX, f32::min);
    let zhi = t.polys.iter().flat_map(|p| p.verts.iter().map(|v| v[2])).fold(f32::MIN, f32::max);
    let mut xml = String::new();
    let _ = write!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<NavMesh>\n <ContentFlags>Polygons</ContentFlags>\n <AreaID value=\"{area}\" />\n <BBMin x=\"{x0}\" y=\"{y0}\" z=\"{zlo}\" />\n <BBMax x=\"{}\" y=\"{}\" z=\"{zhi}\" />\n <BBSize x=\"{TILE}\" y=\"{TILE}\" z=\"{}\" />\n <Polygons>\n",
        x0 + TILE,
        y0 + TILE,
        zhi - zlo
    );
    for p in &t.polys {
        let f = gta_flags(p.area);
        // Last two flag bytes: polygon centre within its own bounding box.
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        let mut c = [0.0f32; 2];
        for v in &p.verts {
            lo = [lo[0].min(v[0]), lo[1].min(v[1])];
            hi = [hi[0].max(v[0]), hi[1].max(v[1])];
            c[0] += v[0] / p.verts.len() as f32;
            c[1] += v[1] / p.verts.len() as f32;
        }
        let q = |v: f32, a: f32, b: f32| if b - a > 1e-3 { (((v - a) / (b - a)) * 255.0).round().clamp(0.0, 255.0) as u8 } else { 127 };
        let _ = write!(xml, "  <Item>\n   <Flags>{} {} {} {} {} {}</Flags>\n   <Vertices>\n", f[0], f[1], f[2], f[3], q(c[0], lo[0], hi[0]), q(c[1], lo[1], hi[1]));
        for v in &p.verts {
            let _ = writeln!(xml, "    {}, {}, {}", v[0], v[1], v[2]);
        }
        xml.push_str("   </Vertices>\n   <Edges>\n");
        for e in &p.edges {
            let (a, i) = match *e {
                Edge::Inner(i) => (area, i),
                Edge::Outer(a, i) => (a, i),
                Edge::None | Edge::Border(..) => (NO_POLY, NO_POLY),
            };
            let _ = writeln!(xml, "    {a}:{i}, {a}:{i}");
        }
        xml.push_str("   </Edges>\n  </Item>\n");
    }
    xml.push_str(" </Polygons>\n <Portals />\n <Points />\n</NavMesh>\n");
    std::fs::write(out_dir.join(format!("navmesh[{}][{}].ynv.xml", t.cx * 3, t.cy * 3)), xml)?;
    Ok(())
}

/// `bpx gta-navmesh`: navmesh tiles for every 150 m GTA tile with walkable
/// Paradise collision, as CodeWalker XML.
pub fn export(col: &Path, out_dir: &Path, placement: Placement) -> Result<()> {
    let col = if col.is_dir() { col.join("WORLDCOL.BIN") } else { col.to_path_buf() };
    let margin = (AGENT_RADIUS / CELL_SIZE).ceil() * CELL_SIZE + 3.0 * CELL_SIZE + 1.0;
    let src = load_source(&col, placement, margin)?;
    let mut keys: Vec<(i32, i32)> = src.buckets.keys().copied().collect();
    keys.sort();
    println!("navmesh: {} triangles over {} candidate tiles", src.tris.len(), keys.len());
    let results: Vec<Result<Option<Tile>>> = keys.par_iter().map(|&(cx, cy)| build_tile(&src, cx, cy)).collect();
    let mut tiles = Vec::new();
    let mut failed = 0;
    for (r, k) in results.into_iter().zip(&keys) {
        match r {
            Ok(Some(t)) => tiles.push(t),
            Ok(None) => {}
            Err(e) => {
                failed += 1;
                eprintln!("tile {k:?}: {e}");
            }
        }
    }
    link_tiles(&mut tiles);
    std::fs::create_dir_all(out_dir)?;
    tiles.par_iter().try_for_each(|t| write_tile(t, out_dir))?;
    let polys: usize = tiles.iter().map(|t| t.polys.len()).sum();
    let max = tiles.iter().map(|t| t.polys.len()).max().unwrap_or(0);
    let foot: usize = tiles.iter().flat_map(|t| &t.polys).filter(|p| p.area == AREA_FOOTPATH).count();
    println!("navmesh: {} tiles, {polys} polygons (max {max} per tile, {foot} footpath), {failed} failed -> {}", tiles.len(), out_dir.display());
    Ok(())
}
