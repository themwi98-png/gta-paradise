//! Generates GTA V water for Paradise City (`water.xml`).
//!
//! GTA only has water where `water.xml` defines axis-aligned quads. Once Los
//! Santos is replaced, none of its quads fit: its lakes, rivers and pools
//! would float over Paradise City, its coast cuts through the new land, and
//! its calming / wave quads are shaped for its own shoreline. So all of them
//! are dropped and replaced:
//!
//! - Lakes and ponds: Burnout's visible water meshes (`bpx water-meshes`,
//!   passed with `--lakes`), rasterised onto the grid at their own heights.
//!   Sloped water (waterfalls, rapids) and sea-level meshes are skipped.
//!   Lakes drawn another way (Silver Lake) are found as enclosed areas with
//!   no land, levelled from their water collision (the surface Burnout
//!   crashes you on), which sits WATER_COLLISION_DEPTH below the surface as
//!   it does for the sea. Lakes then spread over banks lower than their
//!   surface, so they reach the shore.
//! - Shore cells: a 16 m cell the shoreline cuts through counts as land, so
//!   on curved shores the water would stop a cell short in a grid pattern.
//!   Land cells next to water with some floor just below its surface get
//!   that water too; it stays hidden under the parts of the bank above it.
//! - Sea: everywhere else. Water under land stays hidden below the ground
//!   (Paradise has no tunnels below sea level), so rectangles start on cells
//!   with no land and grow over anything but lakes, within 1 km tiles. That
//!   keeps quads few and moderately sized: GTA drops the whole file (no water
//!   at all) with too many quads (~3800 failed in game, ~2500 worked), and
//!   very long thin quads lost their water.
//! - Calming quads over the water near Paradise, so waves stay off low
//!   streets; no wave quads.

use crate::gta::Placement;
use anyhow::{Context, Result};
use bpworld::{self as bw, CollisionTag};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// Sea level (GTA z). It must be GTA's own: outside x -4000..4500 /
/// y -4000..8000 water.xml quads are ignored and GTA draws its endless ocean
/// at z 0, and Paradise City is wider than that. The map is raised to match
/// (`--offset 0,0,9.9`, found by eye in game).
const SEA_Z: f32 = 0.0;
/// Grid cell size in metres (GTA's own quad edges sit on even coordinates).
const CELL: f32 = 16.0;
/// A floor this far above sea level makes a cell land.
const LAND_ABOVE: f32 = 0.3;
/// Lake meshes must sit at least this far above sea level (lower ones, e.g.
/// a harbour mesh 6 m up, are sea-level water covered by the sea) and be
/// flat to within LAKE_FLAT.
const LAKE_ABOVE: f32 = 9.0;
const LAKE_FLAT: f32 = 0.5;
/// Burnout's water collision lies this far below the water surface (sea:
/// collision at -11.4 with the map at +1.5, sea level found at -8.4).
const WATER_COLLISION_DEPTH: f32 = 3.0;
/// How far (cells) a lake may spread over banks lower than its surface.
const LAKE_GROW: usize = 8;
/// Hole fill (crate::fill) at most this far below Burnout's water surface is
/// a lake bed, not land: Silver Lake's bed has collision once filled. Deeper
/// fill lies under a crash surface Burnout stretches over a ravine, no lake.
const LAKE_BED_DEPTH: f32 = 20.0;
/// A land cell next to water is a shore cell, and gets that water, if its
/// lowest floor is below the surface by less than this (deeper ground is
/// beyond a dam or cliff, not the bank).
const SHORE_DEPTH: f32 = 8.0;
/// Enclosed areas smaller than this (cells) are collision noise, not lakes.
const LAKE_MIN_CELLS: usize = 3;
/// Quads never cross these tiles (cells), keeping them moderately sized.
const TILE_CELLS: usize = 64;
/// Calming covers water within one block of land, in blocks of CALM_BLOCK
/// cells.
const CALM_BLOCK: usize = 8;
const CALM_DAMPENING: f32 = 0.1;
/// GTA's water quads span this area; the generated sea covers it too, so the
/// whole world outside Paradise City is ocean once Los Santos is gone.
const WORLD_MIN: [f32; 2] = [-4000.0, -4000.0];
const WORLD_MAX: [f32; 2] = [4500.0, 8000.0];
/// Above this many quads GTA ignores water.xml (see module docs).
const MAX_QUADS: usize = 2400;

struct Grid {
    origin: [f32; 2],
    w: usize,
    h: usize,
    land: Vec<bool>,
    /// Highest / lowest floor per cell (NaN: no floor).
    top: Vec<f32>,
    bottom: Vec<f32>,
    /// Highest water collision per cell (NaN: none).
    water_col: Vec<f32>,
    /// Lake surface height per cell (NaN: no lake).
    lake: Vec<f32>,
}

impl Grid {
    fn cell_of(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let cx = ((x - self.origin[0]) / CELL).floor();
        let cy = ((y - self.origin[1]) / CELL).floor();
        (cx >= 0.0 && cy >= 0.0 && (cx as usize) < self.w && (cy as usize) < self.h).then(|| (cx as usize, cy as usize))
    }

    /// Calls `f(cell, z)` for every cell whose centre the triangle covers
    /// (z interpolated there).
    fn for_centres(&self, t: &[[f32; 3]; 3], mut f: impl FnMut(usize, f32)) {
        let x0 = t.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        let x1 = t.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        let y0 = t.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        let y1 = t.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let (Some((cx0, cy0)), Some((cx1, cy1))) = (self.cell_of(x0, y0), self.cell_of(x1, y1)) else { return };
        let [a, b, c] = *t;
        let den = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if den.abs() < 1e-9 {
            return;
        }
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                let px = self.origin[0] + (cx as f32 + 0.5) * CELL;
                let py = self.origin[1] + (cy as f32 + 0.5) * CELL;
                let l1 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / den;
                let l2 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / den;
                let l3 = 1.0 - l1 - l2;
                if l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0 {
                    f(cy * self.w + cx, l1 * a[2] + l2 * b[2] + l3 * c[2]);
                }
            }
        }
    }
}

/// Replaces the items of `<tag>...</tag>` in water.xml; returns the new
/// text and how many items were there.
fn replace_section(xml: &str, tag: &str, items: &str) -> Result<(String, usize)> {
    let open_tag = format!("<{tag}>");
    let open = xml.find(&open_tag).with_context(|| format!("no {open_tag} in water.xml"))? + open_tag.len();
    let close = open + xml[open..].find(&format!("</{tag}>")).with_context(|| format!("no </{tag}> in water.xml"))?;
    let count = xml[open..close].matches("<Item>").count();
    Ok((format!("{}\n{items}  {}", &xml[..open], &xml[close..]), count))
}

/// Greedy rectangles over a `w`x`h` grid: each starts on a `start` cell and
/// grows over `ok` cells, never crossing `tile`-sized tiles.
fn rectangles(w: usize, h: usize, tile: usize, start: impl Fn(usize) -> bool, ok: impl Fn(usize) -> bool) -> Vec<[usize; 4]> {
    let mut used = vec![false; w * h];
    let mut rects = Vec::new();
    for cy in 0..h {
        for cx in 0..w {
            let i = cy * w + cx;
            if used[i] || !start(i) {
                continue;
            }
            let x_end = ((cx / tile + 1) * tile).min(w);
            let y_end = ((cy / tile + 1) * tile).min(h);
            let mut x1 = cx;
            while x1 + 1 < x_end && !used[cy * w + x1 + 1] && ok(cy * w + x1 + 1) {
                x1 += 1;
            }
            let mut y1 = cy;
            'grow: while y1 + 1 < y_end {
                for x in cx..=x1 {
                    let j = (y1 + 1) * w + x;
                    if used[j] || !ok(j) {
                        break 'grow;
                    }
                }
                y1 += 1;
            }
            for y in cy..=y1 {
                for x in cx..=x1 {
                    used[y * w + x] = true;
                }
            }
            rects.push([cx, cy, x1, y1]);
        }
    }
    rects
}

fn normal_z(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let n = [
        (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
        (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]),
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 {
        n[2] / len
    } else {
        0.0
    }
}

/// Lake triangles from a `bpx water-meshes` CSV (already placed).
fn read_lakes(path: &Path) -> Result<Vec<[[f32; 3]; 3]>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut tris = Vec::new();
    for line in text.lines() {
        let v: Vec<f32> = line.split(',').skip(1).filter_map(|s| s.parse().ok()).collect();
        if v.len() == 9 {
            tris.push([[v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]]);
        }
    }
    Ok(tris)
}

fn quad_xml(out: &mut String, [min_x, max_x, min_y, max_y]: [f32; 4], z: f32) {
    let _ = write!(
        out,
        "    <Item>\n      <minX value=\"{min_x}\" />\n      <maxX value=\"{max_x}\" />\n      <minY value=\"{min_y}\" />\n      <maxY value=\"{max_y}\" />\n      <Type value=\"0\" />\n      <IsInvisible value=\"false\" />\n      <HasLimitedDepth value=\"false\" />\n      <z value=\"{z}\" />\n      <a1 value=\"26\" />\n      <a2 value=\"26\" />\n      <a3 value=\"26\" />\n      <a4 value=\"26\" />\n      <NoStencil value=\"false\" />\n    </Item>\n"
    );
}

pub fn generate(col: &Path, gta_water: &Path, out: &Path, placement: Placement, lakes: Option<&Path>) -> Result<()> {
    let units = crate::colsrc::load(col)?;
    let base = std::fs::read_to_string(gta_water).with_context(|| format!("reading {}", gta_water.display()))?;

    // Placed floor triangles (water surfaces, which are Burnout's seabed, and
    // invisible walls excluded).
    // Each floor with whether it is hole fill.
    let mut tris: Vec<([[f32; 3]; 3], bool)> = Vec::new();
    let mut water_tris: Vec<[[f32; 3]; 3]> = Vec::new();
    let (mut lo, mut hi) = (WORLD_MIN, WORLD_MAX);
    let edits = crate::edits::CollisionEdits::load()?;
    for (unit, polys, ids) in units {
        for (p, &id) in polys.iter().zip(&ids) {
            if edits.is_deleted(unit, id) {
                continue;
            }
            let fill = id >= crate::fill::FILL_ID && id < crate::colsrc::MODDED_ID;
            let tag = CollisionTag::decode(p.tag);
            if tag.surface_id == bw::SURFACE_INVISIBLE_WALL {
                continue;
            }
            let v: Vec<[f32; 3]> = p.verts.iter().map(|&v| placement.apply(v)).collect();
            if tag.surface_id == bw::SURFACE_WATER {
                water_tris.push([v[0], v[1], v[2]]);
                if v.len() == 4 {
                    water_tris.push([v[0], v[2], v[3]]);
                }
                continue;
            }
            for q in &v {
                lo = [lo[0].min(q[0]), lo[1].min(q[1])];
                hi = [hi[0].max(q[0]), hi[1].max(q[1])];
            }
            let mut faces = vec![[v[0], v[1], v[2]]];
            if v.len() == 4 {
                faces.push([v[0], v[2], v[3]]);
            }
            for f in faces {
                if normal_z(f[0], f[1], f[2]).abs() >= 0.25 {
                    tris.push((f, fill));
                }
            }
        }
    }
    let origin = [(lo[0] / CELL).floor() * CELL, (lo[1] / CELL).floor() * CELL];
    let w = ((hi[0] - origin[0]) / CELL).ceil() as usize + 1;
    let h = ((hi[1] - origin[1]) / CELL).ceil() as usize + 1;
    let n = w * h;
    let nan = || vec![f32::NAN; n];
    let mut g = Grid { origin, w, h, land: vec![false; n], top: nan(), bottom: nan(), water_col: nan(), lake: nan() };

    // Land: cells whose centres a floor covers above sea level, plus the
    // cells of floor vertices so thin features register.
    let land_z = SEA_Z + LAND_ABOVE;
    let raise = |v: &mut Vec<f32>, i: usize, z: f32| {
        if !(v[i] >= z) {
            v[i] = z;
        }
    };
    for t in &water_tris {
        let mut hits: Vec<(usize, f32)> = t.iter().filter_map(|p| g.cell_of(p[0], p[1]).map(|(cx, cy)| (cy * w + cx, p[2]))).collect();
        g.for_centres(t, |i, z| hits.push((i, z)));
        for (i, z) in hits {
            raise(&mut g.water_col, i, z);
        }
    }
    let mut bed_hits = 0;
    for (t, fill) in &tris {
        let mut hits: Vec<(usize, f32)> = t.iter().filter_map(|p| g.cell_of(p[0], p[1]).map(|(cx, cy)| (cy * w + cx, p[2]))).collect();
        g.for_centres(t, |i, z| hits.push((i, z)));
        for (i, z) in hits {
            raise(&mut g.top, i, z);
            if !(g.bottom[i] <= z) {
                g.bottom[i] = z;
            }
            let surface = g.water_col[i] + WATER_COLLISION_DEPTH;
            if *fill && z < surface && z >= surface - LAKE_BED_DEPTH {
                bed_hits += 1;
                continue;
            }
            if z >= land_z {
                g.land[i] = true;
            }
        }
    }
    println!("water: {bed_hits} hole fill points under Burnout's water surface are lake bed, not land");

    // Lakes: flat water meshes above sea level, highest surface per cell.
    let mut lake_tris = 0;
    if let Some(path) = lakes {
        for t in read_lakes(path)? {
            let zs = t.map(|p| p[2]);
            let (z0, z1) = (zs.iter().copied().fold(f32::MAX, f32::min), zs.iter().copied().fold(f32::MIN, f32::max));
            if z1 - z0 > LAKE_FLAT || z0 < SEA_Z + LAKE_ABOVE {
                continue;
            }
            lake_tris += 1;
            let z = (zs[0] + zs[1] + zs[2]) / 3.0;
            let mut hits = Vec::new();
            g.for_centres(&t, |i, _| hits.push(i));
            for i in hits {
                if !(g.lake[i] >= z) {
                    g.lake[i] = z;
                }
            }
        }
    }

    // Lakes with no water mesh: areas without land that do not reach the
    // grid edge (the open sea does), levelled from their water collision.
    let mut comp = vec![usize::MAX; n];
    let mut collision_lakes = Vec::new();
    for s0 in 0..n {
        if comp[s0] != usize::MAX || g.land[s0] || !g.lake[s0].is_nan() {
            continue;
        }
        let id = s0;
        let (mut cells, mut queue, mut edge) = (Vec::new(), vec![s0], false);
        comp[s0] = id;
        while let Some(i) = queue.pop() {
            cells.push(i);
            let (x, y) = (i % w, i / w);
            edge |= x == 0 || y == 0 || x == w - 1 || y == h - 1;
            let mut visit = |j: usize| {
                if comp[j] == usize::MAX && !g.land[j] && g.lake[j].is_nan() {
                    comp[j] = id;
                    queue.push(j);
                }
            };
            if x > 0 { visit(i - 1); }
            if x + 1 < w { visit(i + 1); }
            if y > 0 { visit(i - w); }
            if y + 1 < h { visit(i + w); }
        }
        if edge || cells.len() < LAKE_MIN_CELLS {
            continue;
        }
        let mut zs: Vec<f32> = cells.iter().map(|&i| g.water_col[i]).filter(|z| !z.is_nan()).collect();
        if zs.is_empty() {
            continue;
        }
        zs.sort_by(f32::total_cmp);
        let level = zs[zs.len() / 2] + WATER_COLLISION_DEPTH;
        if level < SEA_Z + LAKE_ABOVE {
            continue;
        }
        for &i in &cells {
            g.lake[i] = level;
        }
        collision_lakes.push((cells.len(), level));
    }
    for &(cells, level) in &collision_lakes {
        println!("water: lake from water collision, {cells} cells, surface z={level:.1}");
    }

    // Lakes spread over neighbouring banks lower than their surface.
    let mut frontier: Vec<usize> = (0..n).filter(|&i| !g.lake[i].is_nan()).collect();
    for _ in 0..LAKE_GROW {
        let mut next = Vec::new();
        for &i in &frontier {
            let level = g.lake[i];
            let (x, y) = (i % w, i / w);
            let neighbours = [(x > 0).then(|| i - 1), (x + 1 < w).then(|| i + 1), (y > 0).then(|| i - w), (y + 1 < h).then(|| i + w)];
            for j in neighbours.into_iter().flatten() {
                if g.lake[j].is_nan() && g.land[j] && g.top[j] < level {
                    g.lake[j] = level;
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    // ...and over shore cells along their edge (not spreading further).
    // The lowest ground may be Burnout's water collision (the sea / river /
    // lake bed under the surface), so it counts too.
    let shore = |g: &Grid, j: usize, level: f32| {
        let low = g.bottom[j].min(g.water_col[j]);
        g.land[j] && low < level && low > level - SHORE_DEPTH
    };
    let neighbours8 = |i: usize| {
        let (x, y) = ((i % w) as isize, (i / w) as isize);
        (-1..=1isize)
            .flat_map(move |dy| (-1..=1isize).map(move |dx| (x + dx, y + dy)))
            .filter(move |&(nx, ny)| (nx, ny) != (x, y) && nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h)
            .map(move |(nx, ny)| ny as usize * w + nx as usize)
    };
    let mut lake_shore = Vec::new();
    for i in (0..n).filter(|&i| !g.lake[i].is_nan()) {
        for j in neighbours8(i) {
            if g.lake[j].is_nan() && shore(&g, j, g.lake[i]) {
                lake_shore.push((j, g.lake[i]));
            }
        }
    }
    let lake_shore_cells = lake_shore.len();
    for (j, level) in lake_shore {
        if !(g.lake[j] >= level) {
            g.lake[j] = level;
        }
    }
    // Sea shore cells: land beside open sea, partly below sea level.
    let open_sea = |i: usize| !g.land[i] && g.lake[i].is_nan();
    let sea_shore: Vec<bool> =
        (0..n).map(|j| g.lake[j].is_nan() && shore(&g, j, SEA_Z) && neighbours8(j).any(|i| open_sea(i))).collect();

    // Hand edits (tools/edits/water_edits.json): cells with no water at all.
    let water_edits = crate::edits::WaterEdits::load()?;
    let removed: Vec<bool> = (0..n)
        .map(|i| water_edits.deleted(origin[0] + ((i % w) as f32 + 0.5) * CELL, origin[1] + ((i / w) as f32 + 0.5) * CELL))
        .collect();

    // Debug dump of the grid: u32 w, h; f32 origin x, y; then a u8 class
    // per cell (0 sea, 1 land, 2 lake), row-major from the south-west.
    if let Some(dir) = out.parent() {
        let mut raw = Vec::with_capacity(16 + n);
        raw.extend_from_slice(&(w as u32).to_le_bytes());
        raw.extend_from_slice(&(h as u32).to_le_bytes());
        raw.extend_from_slice(&origin[0].to_le_bytes());
        raw.extend_from_slice(&origin[1].to_le_bytes());
        raw.extend((0..n).map(|i| if !g.lake[i].is_nan() { 2u8 } else { g.land[i] as u8 }));
        let _ = std::fs::create_dir_all(dir.join("debug"));
        let _ = std::fs::write(dir.join("debug").join("water_grid.bin"), raw);
    }

    let span = |c0: usize, c1: usize, o: f32, k: f32| (o + c0 as f32 * k, o + (c1 + 1) as f32 * k);
    let rect = |[cx, cy, x1, y1]: [usize; 4]| {
        let (min_x, max_x) = span(cx, x1, origin[0], CELL);
        let (min_y, max_y) = span(cy, y1, origin[1], CELL);
        [min_x, max_x, min_y, max_y]
    };
    let mut water = String::new();
    // For the viewer's water editor: [minX, minY, maxX, maxY, z, kind (0 sea, 1 lake)].
    let mut view: Vec<[f32; 6]> = Vec::new();

    // Sea: start on cells with no land, grow over anything but lakes.
    let sea = rectangles(w, h, TILE_CELLS, |i| (open_sea(i) || sea_shore[i]) && !removed[i], |i| g.lake[i].is_nan() && !removed[i]);
    for &r in &sea {
        let q = rect(r);
        quad_xml(&mut water, q, SEA_Z);
        view.push([q[0], q[2], q[1], q[3], SEA_Z, 0.0]);
    }
    // Lakes: one set of rectangles per surface height (10 cm steps).
    let mut levels: BTreeMap<i32, Vec<bool>> = BTreeMap::new();
    for i in (0..n).filter(|&i| !g.lake[i].is_nan() && !removed[i]) {
        levels.entry((g.lake[i] * 10.0).round() as i32).or_insert_with(|| vec![false; n])[i] = true;
    }
    let mut lake_quads = 0;
    for (level, cells) in &levels {
        for r in rectangles(w, h, TILE_CELLS, |i| cells[i], |i| cells[i]) {
            let q = rect(r);
            quad_xml(&mut water, q, *level as f32 / 10.0);
            view.push([q[0], q[2], q[1], q[3], *level as f32 / 10.0, 1.0]);
            lake_quads += 1;
        }
    }
    let total = sea.len() + lake_quads;
    anyhow::ensure!(total <= MAX_QUADS, "{total} water quads; GTA ignores water.xml above about {MAX_QUADS}");

    // Calming: blocks with land in them or next to them.
    let (bw_, bh) = (w.div_ceil(CALM_BLOCK), h.div_ceil(CALM_BLOCK));
    let mut has_land = vec![false; bw_ * bh];
    for cy in 0..h {
        for cx in 0..w {
            if g.land[cy * w + cx] {
                has_land[(cy / CALM_BLOCK) * bw_ + cx / CALM_BLOCK] = true;
            }
        }
    }
    let near = |i: usize| {
        let (bx, by) = ((i % bw_) as isize, (i / bw_) as isize);
        (-1..=1).any(|dy| {
            (-1..=1).any(|dx| {
                let (x, y) = (bx + dx, by + dy);
                x >= 0 && y >= 0 && (x as usize) < bw_ && (y as usize) < bh && has_land[y as usize * bw_ + x as usize]
            })
        })
    };
    let calm_rects = rectangles(bw_, bh, TILE_CELLS / CALM_BLOCK, near, near);
    let block = CELL * CALM_BLOCK as f32;
    let mut calm = String::new();
    for &[bx, by, x1, y1] in &calm_rects {
        let (min_x, max_x) = span(bx, x1, origin[0], block);
        let (min_y, max_y) = span(by, y1, origin[1], block);
        let _ = write!(
            calm,
            "    <Item>\n      <minX value=\"{min_x}\" />\n      <maxX value=\"{max_x}\" />\n      <minY value=\"{min_y}\" />\n      <maxY value=\"{max_y}\" />\n      <fDampening value=\"{CALM_DAMPENING}\" />\n    </Item>\n"
        );
    }

    let (xml, original) = replace_section(&base, "WaterQuads", &water)?;
    let (xml, original_calm) = replace_section(&xml, "CalmingQuads", &calm)?;
    let (xml, original_waves) = replace_section(&xml, "WaveQuads", "")?;
    std::fs::write(out, xml)?;
    write_view(out, &view, origin, &placement, removed.iter().filter(|&&r| r).count())?;
    let land = g.land.iter().filter(|&&l| l).count();
    println!(
        "water: grid {w}x{h} cells of {CELL} m, {land} land ({} sea shore, {lake_shore_cells} lake shore); {} sea quads at z={SEA_Z}, {lake_quads} lake quads at {} levels (from {lake_tris} lake triangles) replace {original}; {} calming quads replace {original_calm}; {original_waves} wave quads dropped -> {}",
        sea_shore.iter().filter(|&&b| b).count(),
        sea.len(),
        levels.len(),
        calm_rects.len(),
        out.display()
    );
    Ok(())
}

/// out/water/water_view.json next to the water.xml: the generated quads in
/// GTA coordinates plus what the viewer needs to place them over the
/// Burnout-space map (the placement) and to snap edits to grid cells.
fn write_view(out: &Path, quads: &[[f32; 6]], origin: [f32; 2], placement: &Placement, removed_cells: usize) -> Result<()> {
    let Some(dir) = out.parent() else { return Ok(()) };
    let dir = dir.join("water");
    std::fs::create_dir_all(&dir)?;
    let doc = serde_json::json!({
        "format": "paradise-water-view-v1",
        "cell": CELL,
        "origin": origin,
        "seaZ": SEA_Z,
        "placement": { "offset": placement.offset, "scale": placement.scale, "heading": placement.heading },
        "removedCells": removed_cells,
        "quads": quads,
    });
    std::fs::write(dir.join("water_view.json"), serde_json::to_vec(&doc)?)?;
    Ok(())
}
