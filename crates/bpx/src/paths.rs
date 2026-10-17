//! GTA V traffic paths (ynd) from Burnout Paradise's traffic lanes.

use crate::gta::Placement;
use anyhow::{Context, Result};
use bnd2::Bundle;
use bpworld::traffic::{self, Traffic};
use bpworld::Res;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

pub fn load_traffic(game: &Path) -> Result<Traffic> {
    let b = Bundle::open(game.join("B5TRAFFIC.BNDL"))?;
    let e = b.entries.iter().find(|e| e.type_id == traffic::TRAFFIC_DATA).context("no TrafficData in B5TRAFFIC.BNDL")?;
    let mut t = traffic::parse_traffic(&Res::load(&b, e)?.main);
    // A fifth of Burnout's lanes have no flow of their own (cars only pass
    // through from other lanes), long ones too; in GTA a road where no
    // traffic starts looks dead, so they count as the quietest. A 0 set in
    // the editor still means none.
    for sec in t.hulls.iter_mut().flat_map(|h| h.sections.iter_mut()) {
        sec.vehicles_per_minute = sec.vehicles_per_minute.max(MIN_BURNOUT_FLOW);
    }
    Ok(t)
}

/// Least traffic flow (vehicles per minute) a Burnout lane gets.
const MIN_BURNOUT_FLOW: u16 = 1;

/// Burnout traffic with the hand edits (tools/edits/traffic_edits.json)
/// applied.
pub fn load_traffic_edited(game: &Path) -> Result<Traffic> {
    let mut t = load_traffic(game)?;
    if let Some(edits) = crate::edits::TrafficEdits::load()? {
        edits.apply(&mut t);
    }
    Ok(t)
}

/// Lane centre of a rung.
fn centre(r: &[[f32; 3]; 2]) -> [f32; 3] {
    [(r[0][0] + r[1][0]) * 0.5, (r[0][1] + r[1][1]) * 0.5, (r[0][2] + r[1][2]) * 0.5]
}

/// `bpx traffic`: statistics and a CSV of placed lane centre lines
/// (hull,section,rung,x,y,z) for checking the data.
pub fn dump(game: &Path, out: &Path, placement: Placement) -> Result<()> {
    let t = load_traffic(game)?;
    let sections: usize = t.hulls.iter().map(|h| h.sections.len()).sum();
    let rungs: usize = t.hulls.iter().flat_map(|h| &h.sections).map(|s| s.rungs.len()).sum();
    let fwd: usize = t.hulls.iter().flat_map(|h| &h.sections).map(|s| s.forward.len()).sum();
    let nb: usize = t.hulls.iter().flat_map(|h| &h.sections).map(|s| s.left.len() + s.right.len()).sum();
    let mut speeds: Vec<f32> = t.hulls.iter().flat_map(|h| &h.sections).map(|s| s.speed).collect();
    speeds.sort_by(f32::total_cmp);
    let mut widths: Vec<f32> = t
        .hulls
        .iter()
        .flat_map(|h| &h.sections)
        .flat_map(|s| s.rungs.iter())
        .map(|r| ((r[0][0] - r[1][0]).powi(2) + (r[0][2] - r[1][2]).powi(2)).sqrt())
        .collect();
    widths.sort_by(f32::total_cmp);
    let pct = |v: &[f32], f: f32| if v.is_empty() { 0.0 } else { v[((v.len() - 1) as f32 * f) as usize] };
    println!(
        "traffic v{}: {} hulls, {sections} sections (lanes), {rungs} rungs, {fwd} forward links, {nb} neighbour links",
        t.version,
        t.hulls.len()
    );
    println!("speed p5/p50/p95 {:.1}/{:.1}/{:.1}; lane width p5/p50/p95 {:.2}/{:.2}/{:.2}", pct(&speeds, 0.05), pct(&speeds, 0.5), pct(&speeds, 0.95), pct(&widths, 0.05), pct(&widths, 0.5), pct(&widths, 0.95));
    let mut vpm: Vec<f32> = t.hulls.iter().flat_map(|h| &h.sections).map(|s| s.vehicles_per_minute as f32).collect();
    vpm.sort_by(f32::total_cmp);
    println!("vehicles per minute p0/p5/p25/p50/p75/p95/p100 {}/{}/{}/{}/{}/{}/{}", pct(&vpm, 0.0), pct(&vpm, 0.05), pct(&vpm, 0.25), pct(&vpm, 0.5), pct(&vpm, 0.75), pct(&vpm, 0.95), pct(&vpm, 1.0));
    let mut csv = String::from("hull,section,rung,x,y,z,vehicles_per_minute\n");
    for (h, hull) in t.hulls.iter().enumerate() {
        for (s, sec) in hull.sections.iter().enumerate() {
            for (i, r) in sec.rungs.iter().enumerate() {
                let p = placement.apply(centre(r));
                let _ = writeln!(csv, "{h},{s},{i},{:.2},{:.2},{:.2},{}", p[0], p[1], p[2], sec.vehicles_per_minute);
            }
        }
    }
    std::fs::write(out, csv)?;
    // Lanes that end (or start) without a link, and the gap to the nearest
    // lane start (end) of another lane.
    let lanes: Vec<(usize, usize, [f32; 3], [f32; 3])> = t
        .hulls
        .iter()
        .enumerate()
        .flat_map(|(h, hull)| hull.sections.iter().enumerate().filter(|(_, s)| !s.rungs.is_empty()).map(move |(s, sec)| (h, s, centre(&sec.rungs[0]), centre(sec.rungs.last().unwrap()))))
        .collect();
    let gap = |p: [f32; 3], starts: bool, me: (usize, usize)| {
        lanes
            .iter()
            .filter(|l| (l.0, l.1) != me)
            .map(|l| {
                let q = if starts { l.2 } else { l.3 };
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
            })
            .fold(f32::MAX, f32::min)
    };
    let mut dead_end_gaps = Vec::new();
    let mut no_back = 0;
    for &(h, s, start, end) in &lanes {
        let sec = &t.hulls[h].sections[s];
        if sec.forward.is_empty() {
            dead_end_gaps.push(gap(end, true, (h, s)));
        }
        if sec.backward.is_empty() {
            no_back += 1;
            let _ = start;
        }
    }
    // Gap between each lane's end and the start of each lane it links to.
    let mut link_gaps: Vec<f32> = Vec::new();
    for hull in &t.hulls {
        for sec in &hull.sections {
            let Some(end) = sec.rungs.last().map(centre) else { continue };
            for &(fh, fs) in &sec.forward {
                if let Some(next) = t.hulls.get(fh as usize).and_then(|h| h.sections.get(fs as usize)).and_then(|s| s.rungs.first()) {
                    let q = centre(next);
                    link_gaps.push(((end[0] - q[0]).powi(2) + (end[1] - q[1]).powi(2) + (end[2] - q[2]).powi(2)).sqrt());
                } else {
                    link_gaps.push(f32::INFINITY);
                }
            }
        }
    }
    link_gaps.sort_by(f32::total_cmp);
    println!(
        "forward link gaps: n {} p50 {:.2} p90 {:.2} p99 {:.2} max {:.1}; > 5 m: {}",
        link_gaps.len(),
        pct(&link_gaps, 0.5),
        pct(&link_gaps, 0.9),
        pct(&link_gaps, 0.99),
        link_gaps.last().copied().unwrap_or(0.0),
        link_gaps.iter().filter(|&&g| g > 5.0).count()
    );
    // Do lanes share rung end points (aligned cross-sections)? Key points to
    // 5 cm; count how many sections touch each point and in which direction.
    {
        let key = |p: [f32; 3]| ((p[0] * 20.0).round() as i64, (p[1] * 20.0).round() as i64, (p[2] * 20.0).round() as i64);
        let mut users: HashMap<(i64, i64, i64), Vec<(usize, usize, usize, u8)>> = HashMap::new();
        let mut rung_total = 0usize;
        for (h, hull) in t.hulls.iter().enumerate() {
            for (s, sec) in hull.sections.iter().enumerate() {
                for (i, r) in sec.rungs.iter().enumerate() {
                    rung_total += 1;
                    users.entry(key(r[0])).or_default().push((h, s, i, 0));
                    users.entry(key(r[1])).or_default().push((h, s, i, 1));
                }
            }
        }
        let mut shared_rungs = 0usize;
        let mut opposite = 0usize;
        let mut same_dir = 0usize;
        for list in users.values() {
            let secs: std::collections::HashSet<(usize, usize)> = list.iter().map(|u| (u.0, u.1)).collect();
            if secs.len() < 2 {
                continue;
            }
            shared_rungs += list.len();
            // Same side shared (left with left) means opposite directions;
            // left with right means neighbouring lanes in the same direction.
            for a in list {
                for b in list {
                    if (a.0, a.1) < (b.0, b.1) {
                        if a.3 == b.3 {
                            opposite += 1;
                        } else {
                            same_dir += 1;
                        }
                    }
                }
            }
        }
        println!("rung end points: {} total, {shared_rungs} shared with another lane; pairs: {opposite} same-side (opposite lanes), {same_dir} left-right (same direction)", rung_total * 2);
    }
    dead_end_gaps.sort_by(f32::total_cmp);
    println!(
        "lanes without forward link: {} (gap to nearest lane start p25/p50/p75/p90 {:.2}/{:.2}/{:.2}/{:.2} m), without backward link: {no_back}",
        dead_end_gaps.len(),
        pct(&dead_end_gaps, 0.25),
        pct(&dead_end_gaps, 0.5),
        pct(&dead_end_gaps, 0.75),
        pct(&dead_end_gaps, 0.9)
    );
    let parked: Vec<_> = t.hulls.iter().flat_map(|h| &h.parked).collect();
    println!("{} parked cars", parked.len());
    for v in parked.iter().take(3) {
        println!("  {:?} flow {} chance {} flags {:#x}", v.transform.map(|f| (f * 100.0).round() / 100.0), v.flow_type, v.exists_chance, v.flags);
    }
    Ok(())
}

/// Parked-car ymaps cover tiles of this size (m) and stream in from this far
/// outside them.
const CARGEN_TILE: f32 = 1024.0;
const CARGEN_STREAM: f32 = 300.0;
/// CCarGen flags as used by Rockstar's ambient street parking...
const CARGEN_FLAGS: u32 = 1568;
/// ...plus, for a vehicle set by hand, spawn it whatever the traffic
/// density (force spawn, ignore density).
const CARGEN_SET_MODEL_FLAGS: u32 = CARGEN_FLAGS | 1 | 2;

/// `bpx gta-cargens`: Burnout's parked cars as GTA car generators (random
/// models picked by GTA, or the vehicle set in the traffic editor), one ymap
/// per CARGEN_TILE tile, as CodeWalker XML.
pub fn export_cargens(game: &Path, out_dir: &Path, placement: Placement, prefix: &str) -> Result<()> {
    let t = load_traffic_edited(game)?;
    let mut tiles: BTreeMap<(i32, i32), Vec<([f32; 3], [f32; 3], Option<String>)>> = BTreeMap::new();
    for v in t.hulls.iter().flat_map(|h| &h.parked) {
        let m = &v.transform;
        let p = placement.apply([m[12], m[13], m[14]]);
        // Row 2 is the car's forward axis.
        let f = placement.apply_dir([m[8], m[9], m[10]]);
        let len = (f[0] * f[0] + f[1] * f[1]).sqrt().max(1e-6);
        let tile = ((p[0] / CARGEN_TILE).floor() as i32, (p[1] / CARGEN_TILE).floor() as i32);
        tiles.entry(tile).or_default().push((p, [f[0] / len, f[1] / len, 0.0], v.model.clone()));
    }
    std::fs::create_dir_all(out_dir)?;
    // Drop tiles from an earlier run (cars may have been deleted since).
    for entry in std::fs::read_dir(out_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&format!("{prefix}cargen_")) && name.ends_with(".ymap.xml") {
            std::fs::remove_file(entry.path())?;
        }
    }
    for (&(tx, ty), cars) in &tiles {
        let name = format!("{prefix}cargen_{}_{}", tx + 100, ty + 100);
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        let mut items = String::new();
        for (p, f, model) in cars {
            let (p, f) = (*p, *f);
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
            // orientX/Y: the parking slot's length along the car (5 m).
            let (car_model, flags) = match model {
                Some(m) => (format!("<carModel>{m}</carModel>"), CARGEN_SET_MODEL_FLAGS),
                None => ("<carModel />".to_string(), CARGEN_FLAGS),
            };
            let _ = write!(
                items,
                "  <Item>\n   {}\n   <orientX value=\"{}\" />\n   <orientY value=\"{}\" />\n   <perpendicularLength value=\"3\" />\n   {car_model}\n   <flags value=\"{flags}\" />\n   <bodyColorRemap1 value=\"-1\" />\n   <bodyColorRemap2 value=\"-1\" />\n   <bodyColorRemap3 value=\"-1\" />\n   <bodyColorRemap4 value=\"-1\" />\n   <popGroup />\n   <livery value=\"-1\" />\n  </Item>\n",
                crate::gtavis::v3("position", [p[0], p[1], p[2] + 0.5]),
                f[0] * 5.0,
                f[1] * 5.0
            );
        }
        let s = CARGEN_STREAM;
        let head = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>\n <name>{name}</name>\n <parent />\n <flags value=\"0\" />\n <contentFlags value=\"0\" />\n {}\n {}\n {}\n {}\n <entities>\n",
            crate::gtavis::v3("streamingExtentsMin", [lo[0] - s, lo[1] - s, lo[2] - s]),
            crate::gtavis::v3("streamingExtentsMax", [hi[0] + s, hi[1] + s, hi[2] + s]),
            crate::gtavis::v3("entitiesExtentsMin", lo),
            crate::gtavis::v3("entitiesExtentsMax", hi)
        );
        let tail = crate::gtavis::YMAP_TAIL
            .replace("{ymap}", &name)
            .replace(" <carGenerators itemType=\"CCarGen\" />\n", &format!(" <carGenerators itemType=\"CCarGen\">\n{items} </carGenerators>\n"));
        std::fs::write(out_dir.join(format!("{name}.ymap.xml")), head + &tail)?;
    }
    let total: usize = tiles.values().map(|c| c.len()).sum();
    let set: usize = tiles.values().flatten().filter(|c| c.2.is_some()).count();
    println!("cargens: {total} parked vehicles ({set} set by hand) in {} ymaps -> {}", tiles.len(), out_dir.display());
    Ok(())
}

/// Keep a rung as a path node when the lane has turned this much (radians)
/// or run this far (m) since the last kept one.
const NODE_TURN: f32 = 0.17;
const NODE_SPACING: f32 = 25.0;
/// GTA's path node grid: 32 x 32 areas of 512 m from (-8192, -8192).
const AREA_SIZE: f32 = 512.0;
const AREA_ORIGIN: f32 = -8192.0;
const AREAS: usize = 32;

fn area_of(p: [f32; 3]) -> usize {
    let c = |v: f32| (((v - AREA_ORIGIN) / AREA_SIZE).floor() as isize).clamp(0, AREAS as isize - 1) as usize;
    c(p[1]) * AREAS + c(p[0])
}

/// Union-find over node indices.
struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, i: usize) -> usize {
        let mut r = i;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut i = i;
        while self.0[i] != r {
            let next = self.0[i];
            self.0[i] = r;
            i = next;
        }
        r
    }

    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[b] = a;
        }
    }
}

/// GTA node density (0-15: how much traffic starts there, relative to other
/// nodes) per vehicle per minute of Burnout traffic flow (0-9). Burnout's
/// middle value (5) gives 6, what every node had before.
const DENSITY_PER_VPM: f32 = 1.2;

fn gta_density(vehicles_per_minute: u16) -> u8 {
    (vehicles_per_minute as f32 * DENSITY_PER_VPM).round().clamp(0.0, 15.0) as u8
}

/// GTA node speed class (0 slow .. 3 fastest) from a Burnout lane speed (m/s).
fn speed_class(speed: f32) -> u8 {
    match speed {
        s if s < 11.0 => 0,
        s if s < 18.0 => 1,
        s if s < 27.0 => 2,
        _ => 3,
    }
}

/// `bpx gta-paths --lanes`: one ynd per GTA path area (all 1024, empty where there
/// is no road, so the Los Santos nodes are replaced) as CodeWalker XML.
///
/// Each Burnout lane becomes a one-way, one-lane GTA road along its lane
/// centre line, with nodes where it bends or every NODE_SPACING m. A lane's
/// end node is shared with the start node of every lane it leads into.
pub fn export(game: &Path, out_dir: &Path, placement: Placement) -> Result<()> {
    let t = load_traffic_edited(game)?;
    // Candidate nodes: the kept rungs of every lane, as indices into `pos`.
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut speed: Vec<f32> = Vec::new();
    let mut busy: Vec<u16> = Vec::new();
    let mut lane_nodes: Vec<Vec<Vec<usize>>> = Vec::with_capacity(t.hulls.len());
    let heading = |a: [f32; 3], b: [f32; 3]| (b[1] - a[1]).atan2(b[0] - a[0]);
    for hull in &t.hulls {
        let mut per_section = Vec::with_capacity(hull.sections.len());
        for sec in &hull.sections {
            let pts: Vec<[f32; 3]> = sec.rungs.iter().map(|r| placement.apply(centre(r))).collect();
            let mut kept: Vec<usize> = Vec::new();
            for (i, p) in pts.iter().enumerate() {
                let keep = match kept.last() {
                    None => true,
                    Some(_) if i + 1 == pts.len() => true,
                    Some(&k) => {
                        let last = pts[k];
                        let dist = ((p[0] - last[0]).powi(2) + (p[1] - last[1]).powi(2)).sqrt();
                        let prev = if k > 0 { heading(pts[k - 1], last) } else { heading(last, pts[k + 1]) };
                        let mut turn = (heading(last, *p) - prev).abs();
                        if turn > std::f32::consts::PI {
                            turn = 2.0 * std::f32::consts::PI - turn;
                        }
                        dist >= NODE_SPACING || (turn >= NODE_TURN && dist >= 4.0)
                    }
                };
                if keep {
                    kept.push(i);
                }
            }
            let ids: Vec<usize> = kept
                .iter()
                .map(|&i| {
                    pos.push(pts[i]);
                    speed.push(sec.speed);
                    busy.push(sec.vehicles_per_minute);
                    pos.len() - 1
                })
                .collect();
            per_section.push(ids);
        }
        lane_nodes.push(per_section);
    }

    // Join each lane's end to the start of the lanes it leads into.
    let mut dsu = Dsu((0..pos.len()).collect());
    let lane = |h: u16, s: u8| lane_nodes.get(h as usize).and_then(|l| l.get(s as usize));
    for (h, hull) in t.hulls.iter().enumerate() {
        for (s, sec) in hull.sections.iter().enumerate() {
            let ids = &lane_nodes[h][s];
            let (Some(&start), Some(&end)) = (ids.first(), ids.last()) else { continue };
            for &(fh, fs) in &sec.forward {
                if let Some(&next_start) = lane(fh, fs).and_then(|n| n.first()) {
                    dsu.union(end, next_start);
                }
            }
            for &(bh, bs) in &sec.backward {
                if let Some(&prev_end) = lane(bh, bs).and_then(|n| n.last()) {
                    dsu.union(prev_end, start);
                }
            }
        }
    }

    // Merged nodes: averaged position, fastest lane speed, busiest lane.
    let mut rep_index: HashMap<usize, usize> = HashMap::new();
    let mut sums: Vec<([f32; 3], usize, f32, u16)> = Vec::new();
    let mut node_of = vec![0usize; pos.len()];
    for i in 0..pos.len() {
        let r = dsu.find(i);
        let k = *rep_index.entry(r).or_insert_with(|| {
            sums.push(([0.0; 3], 0, 0.0, 0));
            sums.len() - 1
        });
        let n = &mut sums[k];
        for a in 0..3 {
            n.0[a] += pos[i][a];
        }
        n.1 += 1;
        n.2 = n.2.max(speed[i]);
        n.3 = n.3.max(busy[i]);
        node_of[i] = k;
    }
    let node_pos: Vec<[f32; 3]> = sums.iter().map(|n| [n.0[0] / n.1 as f32, n.0[1] / n.1 as f32, n.0[2] / n.1 as f32]).collect();

    // Lane edges between merged nodes; per node pair, whether traffic runs
    // each way.
    let mut pairs: BTreeMap<(usize, usize), (u8, u8)> = BTreeMap::new();
    for ids in lane_nodes.iter().flatten() {
        for w in ids.windows(2) {
            let (a, b) = (node_of[w[0]], node_of[w[1]]);
            if a == b {
                continue;
            }
            let e = pairs.entry((a.min(b), a.max(b))).or_insert((0, 0));
            if a < b {
                e.0 = 1;
            } else {
                e.1 = 1;
            }
        }
    }
    // Per node: (to, lanes towards it, lanes from it).
    let mut links: Vec<Vec<(usize, u8, u8)>> = vec![Vec::new(); node_pos.len()];
    for (&(a, b), &(ab, ba)) in &pairs {
        links[a].push((b, ab, ba));
        links[b].push((a, ba, ab));
    }

    let speeds: Vec<f32> = sums.iter().map(|n| n.2).collect();
    let density: Vec<u8> = sums.iter().map(|n| gta_density(n.3)).collect();
    let (total, areas, max_links) = write_ynds(&node_pos, &speeds, &density, &links, out_dir)?;
    let link_count = links.iter().map(|l| l.len()).sum::<usize>() / 2;
    println!(
        "paths: {} lane nodes merged into {total} GTA nodes, {link_count} links (max {max_links} per node), {areas} areas used -> {}",
        pos.len(),
        out_dir.display()
    );
    Ok(())
}

/// Writes one ynd XML per GTA path area (all 1024, empty where there is no
/// road, so the Los Santos nodes are replaced). `links[i]` lists (to, lanes
/// from i towards `to`, lanes back); `density` is GTA's 0-15 traffic density
/// per node. Nodes without links are dropped.
/// Returns (nodes written, areas used, most links on a node).
fn write_ynds(node_pos: &[[f32; 3]], speed: &[f32], density: &[u8], links: &[Vec<(usize, u8, u8)>], out_dir: &Path) -> Result<(usize, usize, usize)> {
    // Areas and ids; nodes without links are dropped.
    let mut area_nodes: Vec<Vec<usize>> = vec![Vec::new(); AREAS * AREAS];
    for (i, p) in node_pos.iter().enumerate() {
        if !links[i].is_empty() {
            area_nodes[area_of(*p)].push(i);
        }
    }
    let mut id_of = vec![(0usize, 0usize); node_pos.len()];
    for (area, list) in area_nodes.iter().enumerate() {
        for (k, &i) in list.iter().enumerate() {
            id_of[i] = (area, k);
        }
    }
    let max_links = links.iter().map(|l| l.len()).max().unwrap_or(0);
    anyhow::ensure!(max_links <= 31, "a path node has {max_links} links (GTA allows 31)");

    let dead = deadendness(links);
    std::fs::create_dir_all(out_dir)?;
    let mut total = 0;
    for (area, list) in area_nodes.iter().enumerate() {
        let mut xml = String::new();
        let _ = writeln!(
            xml,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<NodeDictionary>\n <VehicleNodeCount value=\"{}\" />\n <PedNodeCount value=\"0\" />",
            list.len()
        );
        if list.is_empty() {
            xml.push_str(" <Nodes />\n");
        } else {
            xml.push_str(" <Nodes>\n");
            for (k, &i) in list.iter().enumerate() {
                let p = node_pos[i];
                let class = speed_class(speed[i]);
                // Flags0 2: GPS enabled; Flags2 4: junction (3+ roads meet;
                // set on most of Los Santos' junction nodes and no others),
                // 64: highway; Flags4 bits 0-3: density, bits 4-6: how deep
                // in a dead end (as in Los Santos, so routes avoid them);
                // Flags5 bits 1-2: speed class.
                let highway = if class >= 3 { 64 } else { 0 };
                let junction = if links[i].len() >= 3 { 4 } else { 0 };
                let flags4 = density[i].min(15) | (dead[i] << 4);
                let _ = write!(
                    xml,
                    "  <Item>\n   <AreaID value=\"{area}\" />\n   <NodeID value=\"{k}\" />\n   <StreetName />\n   <Position x=\"{}\" y=\"{}\" z=\"{}\" />\n   <Flags0 value=\"2\" />\n   <Flags1 value=\"0\" />\n   <Flags2 value=\"{}\" />\n   <Flags3 value=\"0\" />\n   <Flags4 value=\"{flags4}\" />\n   <Flags5 value=\"{}\" />\n   <Links>\n",
                    p[0],
                    p[1],
                    p[2],
                    highway | junction,
                    class << 1
                );
                for &(to, lanes_to, lanes_from) in &links[i] {
                    let (ta, tn) = id_of[to];
                    let q = node_pos[to];
                    let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt().round().clamp(1.0, 255.0) as u8;
                    // Flags2: lanes forward (towards `to`) in bits 5-7, backward in 2-4.
                    let flags2 = (lanes_to << 5) | (lanes_from << 2);
                    let _ = write!(
                        xml,
                        "    <Item>\n     <ToAreaID value=\"{ta}\" />\n     <ToNodeID value=\"{tn}\" />\n     <Flags0 value=\"0\" />\n     <Flags1 value=\"0\" />\n     <Flags2 value=\"{flags2}\" />\n     <LinkLength value=\"{len}\" />\n    </Item>\n"
                    );
                }
                xml.push_str("   </Links>\n  </Item>\n");
            }
            xml.push_str(" </Nodes>\n");
            total += list.len();
        }
        xml.push_str(" <Junctions />\n <JunctionRefs />\n</NodeDictionary>\n");
        std::fs::write(out_dir.join(format!("nodes{area}.ynd.xml")), xml)?;
    }
    Ok((total, area_nodes.iter().filter(|l| !l.is_empty()).count(), max_links))
}

/// Dead-end depth per node (0..7): nodes on a branch that leads nowhere,
/// counted from where the branch leaves the network (7 = deep inside), found
/// by peeling end nodes off the undirected road graph.
fn deadendness(links: &[Vec<(usize, u8, u8)>]) -> Vec<u8> {
    let n = links.len();
    let mut adj: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for (a, l) in links.iter().enumerate() {
        for &(b, _, _) in l {
            if a != b {
                adj[a].insert(b);
                adj[b].insert(a);
            }
        }
    }
    // Round k removes the nodes left with a single neighbour.
    let mut degree: Vec<usize> = adj.iter().map(|s| s.len()).collect();
    let mut round = vec![0u32; n];
    let mut frontier: Vec<usize> = (0..n).filter(|&i| degree[i] == 1).collect();
    let mut k = 0;
    while !frontier.is_empty() {
        k += 1;
        let mut next = Vec::new();
        for &i in &frontier {
            if round[i] != 0 {
                continue;
            }
            round[i] = k;
            for &j in &adj[i] {
                if round[j] == 0 {
                    degree[j] -= 1;
                    if degree[j] == 1 {
                        next.push(j);
                    }
                }
            }
        }
        frontier = next;
    }
    // Walk each branch outwards from where it joins the network (or, for a
    // network that is all branches, from its last node).
    let start = |i: usize| round[i] != 0 && (adj[i].iter().any(|&j| round[j] == 0) || adj[i].iter().all(|&j| round[j] != 0 && round[j] <= round[i]));
    let mut stack: Vec<(usize, u8)> = (0..n).filter(|&i| start(i)).map(|i| (i, 1)).collect();
    let mut depth = vec![0u8; n];
    while let Some((i, d)) = stack.pop() {
        if depth[i] >= d {
            continue;
        }
        depth[i] = d;
        for &j in &adj[i] {
            if round[j] != 0 && round[j] < round[i] {
                stack.push((j, (d + 1).min(7)));
            }
        }
    }
    depth
}

/// Rungs that share an end point are side-by-side lanes of one road if
/// their directions agree to within this (|cos|).
const COLLINEAR: f32 = 0.9;
/// Straight runs are thinned to a node at most every ROAD_SPACING m; bends
/// sharper than ROAD_TURN (radians) keep their node.
const ROAD_SPACING: f32 = 25.0;
const ROAD_TURN: f32 = 0.12;
/// GTA stores at most 7 lanes per direction.
const MAX_LANES: usize = 7;

/// Largest strongly connected component as a fraction of the linked nodes
/// (directed by lane counts): how much of the network every car (and the
/// police) can reach from everywhere else.
fn largest_scc_fraction(links: &[Vec<(usize, u8, u8)>]) -> f32 {
    let comp = scc_labels(links);
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for (i, &c) in comp.iter().enumerate() {
        if !links[i].is_empty() {
            *sizes.entry(c).or_default() += 1;
        }
    }
    let used = links.iter().filter(|l| !l.is_empty()).count().max(1);
    sizes.values().copied().max().unwrap_or(0) as f32 / used as f32
}

/// Strongly connected component label per node (directed by lane counts).
fn scc_labels(links: &[Vec<(usize, u8, u8)>]) -> Vec<usize> {
    let n = links.len();
    let fwd: Vec<Vec<usize>> = links.iter().map(|l| l.iter().filter(|e| e.1 > 0).map(|e| e.0).collect()).collect();
    let mut rev: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (a, outs) in fwd.iter().enumerate() {
        for &b in outs {
            rev[b].push(a);
        }
    }
    // Kosaraju: finish order on the graph, then components on the reverse.
    let mut seen = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for s in 0..n {
        if seen[s] || links[s].is_empty() {
            continue;
        }
        seen[s] = true;
        let mut stack = vec![(s, 0usize)];
        while let Some(top) = stack.last_mut() {
            let (v, i) = *top;
            if i < fwd[v].len() {
                top.1 += 1;
                let w = fwd[v][i];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(v);
                stack.pop();
            }
        }
    }
    let mut comp: Vec<usize> = (0..n).collect();
    let mut done = vec![false; n];
    for &s in order.iter().rev() {
        if done[s] {
            continue;
        }
        done[s] = true;
        let mut stack = vec![s];
        while let Some(v) = stack.pop() {
            comp[v] = s;
            for &w in &rev[v] {
                if !done[w] {
                    done[w] = true;
                    stack.push(w);
                }
            }
        }
    }
    comp
}

/// `bpx gta-paths`: GTA roads from Burnout's traffic lanes.
///
/// Burnout's lanes are cut from shared road cross-sections: neighbouring
/// lanes' rungs share end points exactly (same direction: one lane's left
/// edge is the next one's right; opposite directions: both lanes' left
/// edges lie on the centre line). Rungs joined that way (and identical
/// rungs where one lane continues into the next) form one cross-section,
/// which becomes one GTA node on the line between the two directions. Each
/// lane's step from one cross-section to the next adds a lane to the link
/// between them, in its direction, so roads become two-way multi-lane GTA
/// links. Junction turning lanes share no rungs and stay one-lane
/// connectors. Straight runs are then thinned.
pub fn export_roads(game: &Path, out_dir: &Path, placement: Placement) -> Result<()> {
    let t = load_traffic_edited(game)?;
    // Placed rungs (left, right), per lane in driving order.
    let mut rungs: Vec<[[f32; 3]; 2]> = Vec::new();
    let mut rung_lane: Vec<usize> = Vec::new();
    let mut lanes: Vec<(f32, Vec<usize>)> = Vec::new();
    let mut lane_busy: Vec<u16> = Vec::new();
    for hull in &t.hulls {
        for sec in &hull.sections {
            let lane = lanes.len();
            let ids: Vec<usize> = sec
                .rungs
                .iter()
                .map(|r| {
                    rungs.push([placement.apply(r[0]), placement.apply(r[1])]);
                    rung_lane.push(lane);
                    rungs.len() - 1
                })
                .collect();
            lanes.push((sec.speed, ids));
            lane_busy.push(sec.vehicles_per_minute);
        }
    }
    let n = rungs.len();

    // Cross-sections: union rungs that share an end point and line up.
    let key = |p: [f32; 3]| ((p[0] * 20.0).round() as i64, (p[1] * 20.0).round() as i64, (p[2] * 20.0).round() as i64);
    let mut at: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, r) in rungs.iter().enumerate() {
        at.entry(key(r[0])).or_default().push(i);
        at.entry(key(r[1])).or_default().push(i);
    }
    let across = |r: &[[f32; 3]; 2]| {
        let d = [r[1][0] - r[0][0], r[1][1] - r[0][1]];
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
        [d[0] / l, d[1] / l]
    };
    let mut dsu = Dsu((0..n).collect());
    for list in at.values() {
        for (k, &a) in list.iter().enumerate() {
            for &b in &list[k + 1..] {
                if a == b || rung_lane[a] == rung_lane[b] {
                    continue;
                }
                let (da, db) = (across(&rungs[a]), across(&rungs[b]));
                if (da[0] * db[0] + da[1] * db[1]).abs() >= COLLINEAR {
                    dsu.union(a, b);
                }
            }
        }
    }

    // Lane heading at each rung.
    let mut heading = vec![[0.0f32; 2]; n];
    for (_, ids) in &lanes {
        let c: Vec<[f32; 3]> = ids.iter().map(|&i| centre(&rungs[i])).collect();
        for (k, &i) in ids.iter().enumerate() {
            let (a, b) = if k + 1 < c.len() {
                (c[k], c[k + 1])
            } else if k > 0 {
                (c[k - 1], c[k])
            } else {
                continue;
            };
            let d = [b[0] - a[0], b[1] - a[1]];
            let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
            heading[i] = [d[0] / l, d[1] / l];
        }
    }

    // One node per cross-section, on the line between the directions.
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        members.entry(dsu.find(i)).or_default().push(i);
    }
    let mut node_of_root: HashMap<usize, usize> = HashMap::new();
    let mut node_pos: Vec<[f32; 3]> = Vec::new();
    let mut node_speed: Vec<f32> = Vec::new();
    let mut node_density: Vec<u8> = Vec::new();
    let (mut two_way, mut split_ok, mut repeat_lane) = (0usize, 0usize, 0usize);
    for (&root, ids) in &members {
        let h0 = heading[ids[0]];
        let right = [h0[1], -h0[0]];
        let c0 = centre(&rungs[ids[0]]);
        let mut fwd: Vec<(f32, [f32; 3])> = Vec::new();
        let mut bwd: Vec<(f32, [f32; 3])> = Vec::new();
        for &i in ids {
            let c = centre(&rungs[i]);
            let off = (c[0] - c0[0]) * right[0] + (c[1] - c0[1]) * right[1];
            if heading[i][0] * h0[0] + heading[i][1] * h0[1] >= 0.0 {
                fwd.push((off, c));
            } else {
                bwd.push((off, c));
            }
        }
        let lanes_here: std::collections::HashSet<usize> = ids.iter().map(|&i| rung_lane[i]).collect();
        if lanes_here.len() < ids.len() {
            repeat_lane += 1;
        }
        let all: Vec<(f32, [f32; 3])> = fwd.iter().chain(&bwd).copied().collect();
        let k = all.len() as f32;
        let mut pos = all.iter().fold([0.0; 3], |a, (_, c)| [a[0] + c[0] / k, a[1] + c[1] / k, a[2] + c[2] / k]);
        if !fwd.is_empty() && !bwd.is_empty() {
            two_way += 1;
            // Right-hand traffic: lanes going our way lie to the right.
            let inner_f = fwd.iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
            let inner_b = bwd.iter().max_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
            if inner_b.0 < inner_f.0 {
                split_ok += 1;
                pos = [(inner_f.1[0] + inner_b.1[0]) * 0.5, (inner_f.1[1] + inner_b.1[1]) * 0.5, pos[2]];
            }
        }
        node_of_root.insert(root, node_pos.len());
        node_pos.push(pos);
        node_speed.push(ids.iter().map(|&i| lanes[rung_lane[i]].0).fold(0.0, f32::max));
        // As busy as its busiest lane.
        node_density.push(gta_density(ids.iter().map(|&i| lane_busy[rung_lane[i]]).max().unwrap_or(0)));
    }

    // Links: distinct lanes stepping between cross-sections, per direction.
    type LaneSet = std::collections::HashSet<usize>;
    let mut steps: HashMap<(usize, usize), (LaneSet, LaneSet)> = HashMap::new();
    for (lane, (_, ids)) in lanes.iter().enumerate() {
        for w in ids.windows(2) {
            let a = node_of_root[&dsu.find(w[0])];
            let b = node_of_root[&dsu.find(w[1])];
            if a == b {
                continue;
            }
            let e = steps.entry((a.min(b), a.max(b))).or_default();
            if a < b {
                e.0.insert(lane);
            } else {
                e.1.insert(lane);
            }
        }
    }
    let mut adj: Vec<BTreeMap<usize, (u8, u8)>> = vec![BTreeMap::new(); node_pos.len()];
    for (&(a, b), (ab, ba)) in &steps {
        let (ab, ba) = (ab.len().min(MAX_LANES) as u8, ba.len().min(MAX_LANES) as u8);
        adj[a].insert(b, (ab, ba));
        adj[b].insert(a, (ba, ab));
    }
    let before = adj.iter().filter(|m| !m.is_empty()).count();

    // Thin straight runs: drop a node with exactly two neighbours when the
    // lanes carry straight through, the road barely bends and the merged
    // link stays short.
    let dist = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    loop {
        let mut removed = 0;
        for v in 0..node_pos.len() {
            if adj[v].len() != 2 {
                continue;
            }
            let mut it = adj[v].iter();
            let (&p, &(vp_to, vp_from)) = it.next().unwrap();
            let (&q, &(vq_to, vq_from)) = it.next().unwrap();
            // Lanes p->v must equal v->q, and q->v equal v->p.
            if vp_from != vq_to || vq_from != vp_to || adj[p].contains_key(&q) {
                continue;
            }
            let (a, b, c) = (node_pos[p], node_pos[v], node_pos[q]);
            if dist(a, c) > ROAD_SPACING {
                continue;
            }
            let h1 = (b[1] - a[1]).atan2(b[0] - a[0]);
            let h2 = (c[1] - b[1]).atan2(c[0] - b[0]);
            let mut turn = (h2 - h1).abs();
            if turn > std::f32::consts::PI {
                turn = 2.0 * std::f32::consts::PI - turn;
            }
            if turn > ROAD_TURN {
                continue;
            }
            adj[p].remove(&v);
            adj[q].remove(&v);
            adj[v].clear();
            // p -> q carries what p -> v did.
            adj[p].insert(q, (vp_from, vp_to));
            adj[q].insert(p, (vp_to, vp_from));
            removed += 1;
        }
        if removed == 0 {
            break;
        }
    }

    let diamonds = merge_diamonds(&mut adj, &node_pos);
    println!("roads: merged {diamonds} pairs of one-way chains into two-way links");

    // Burnout's traffic never leaves some parts (Big Surf Island's bridge
    // ends short of the mainland roads): join each separate road group to
    // the main network by a two-way link between their closest nodes.
    // Repeated, since a joined group (a bridge) brings the next one in reach.
    let mut joins = Vec::new();
    loop {
        let more = join_road_groups(&mut adj, &node_pos);
        if more.is_empty() {
            break;
        }
        joins.extend(more);
    }
    for (gap, size) in &joins {
        println!("roads: joined a separate group of {size} nodes to the main network across {gap:.1} m");
    }

    let links: Vec<Vec<(usize, u8, u8)>> = adj.iter().map(|m| m.iter().map(|(&to, &(a, b))| (to, a, b)).collect()).collect();
    let mut hist = [0usize; 8];
    for l in links.iter().flatten() {
        hist[(l.1 + l.2).min(7) as usize] += 1;
    }
    let scc = largest_scc_fraction(&links);
    let (total, areas, max_links) = write_ynds(&node_pos, &node_speed, &node_density, &links, out_dir)?;
    let mut by_density = [0usize; 16];
    for (i, l) in links.iter().enumerate() {
        if !l.is_empty() {
            by_density[node_density[i] as usize] += 1;
        }
    }
    println!("roads: nodes by traffic density 0-15 {by_density:?}");
    println!(
        "roads: {n} rungs -> {} cross-sections ({two_way} two-way, {split_ok} with directions cleanly split, {repeat_lane} folding one lane twice); {before} linked nodes thinned to {total}",
        members.len()
    );
    println!(
        "roads: links by total lanes {:?}; max {max_links} links per node; largest strongly connected part {:.1}% of nodes; {areas} areas -> {}",
        &hist[1..],
        scc * 100.0,
        out_dir.display()
    );
    Ok(())
}

/// Separate road groups this close to the main network get joined to it.
const MAX_JOIN_GAP: f32 = 150.0;
/// Groups smaller than this (nodes) are left alone (stray connectors).
const MIN_JOIN_GROUP: usize = 20;

/// Joins road groups that cannot be both reached from and left to the main
/// network (strongly connected components other than the largest) to it: a
/// two-way link between the closest pair of nodes (an existing one-way link
/// there gets lanes both ways), one lane each way. Returns (gap, group size) per join.
fn join_road_groups(adj: &mut [BTreeMap<usize, (u8, u8)>], pos: &[[f32; 3]]) -> Vec<(f32, usize)> {
    let links: Vec<Vec<(usize, u8, u8)>> = adj.iter().map(|m| m.iter().map(|(&to, &(a, b))| (to, a, b)).collect()).collect();
    let comp = scc_labels(&links);
    let mut by_comp: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &c) in comp.iter().enumerate() {
        if !adj[i].is_empty() {
            by_comp.entry(c).or_default().push(i);
        }
    }
    let mut groups: Vec<Vec<usize>> = by_comp.into_values().collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
    let main = 0;
    if groups.is_empty() {
        return Vec::new();
    }
    let main_nodes = groups[main].clone();
    let mut joins = Vec::new();
    for (g, members) in groups.iter().enumerate() {
        if g == main || members.len() < MIN_JOIN_GROUP {
            continue;
        }
        let mut best: Option<(f32, usize, usize)> = None;
        for &a in members {
            for &b in &main_nodes {
                let d = ((pos[a][0] - pos[b][0]).powi(2) + (pos[a][1] - pos[b][1]).powi(2) + (pos[a][2] - pos[b][2]).powi(2)).sqrt();
                if best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, a, b));
                }
            }
        }
        let Some((gap, a, b)) = best else { continue };
        if std::env::var_os("BPX_DEBUG_JOIN").is_some() {
            eprintln!("group {} nodes: closest main node {gap:.1} m (at {:?})", members.len(), pos[a]);
        }
        if gap > MAX_JOIN_GAP {
            continue;
        }
        // One lane each way: the gap is not a real road, and wider links
        // put GTA's outer lanes on the kerbs of the road they meet.
        let lanes = 1;
        let (x, y) = adj[a].get(&b).copied().unwrap_or((0, 0));
        let (x, y) = (x.max(lanes), y.max(lanes));
        adj[a].insert(b, (x, y));
        adj[b].insert(a, (y, x));
        joins.push((gap, members.len()));
        break;
    }
    joins
}

/// One-way chains this far (at most) from the straight line between their
/// ends still count as straight.
const DIAMOND_DEVIATION: f32 = 6.0;
/// Longest pair of chains merged into one two-way link (m).
const DIAMOND_MAX_LEN: f32 = 150.0;

/// Where a road's two directions did not share cross-sections, they come
/// out as two one-way chains between the same pair of nodes (a "diamond").
/// Each such straight pair becomes one two-way link, so cars keep to one
/// road line. Returns how many pairs were merged.
fn merge_diamonds(adj: &mut [BTreeMap<usize, (u8, u8)>], pos: &[[f32; 3]]) -> usize {
    // One-way chain from `a` starting along `first`: (end node, inner nodes,
    // lanes), following nodes that have exactly two neighbours and carry
    // lanes in one direction only.
    let follow = |adj: &[BTreeMap<usize, (u8, u8)>], a: usize, first: usize| -> Option<(usize, Vec<usize>, u8)> {
        let (lanes, back) = adj[a].get(&first).copied()?;
        if lanes == 0 || back != 0 {
            return None;
        }
        let (mut prev, mut cur) = (a, first);
        let mut inner = Vec::new();
        while adj[cur].len() == 2 && inner.len() < 64 {
            let (&next, &(l, b)) = adj[cur].iter().find(|(&k, _)| k != prev)?;
            if l != lanes || b != 0 {
                break;
            }
            inner.push(cur);
            prev = cur;
            cur = next;
        }
        Some((cur, inner, lanes))
    };
    let straight = |a: usize, b: usize, inner: &[usize]| {
        let (p, q) = (pos[a], pos[b]);
        let d = [q[0] - p[0], q[1] - p[1]];
        let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
        len > 1.0
            && len <= DIAMOND_MAX_LEN
            && inner.iter().all(|&v| {
                let r = [pos[v][0] - p[0], pos[v][1] - p[1]];
                (r[0] * d[1] - r[1] * d[0]).abs() / len <= DIAMOND_DEVIATION
            })
    };
    let mut merged = 0;
    for a in 0..adj.len() {
        let starts: Vec<usize> = adj[a].keys().copied().collect();
        for first in starts {
            let Some((b, inner_ab, lanes_ab)) = follow(adj, a, first) else { continue };
            if b == a || inner_ab.is_empty() || !straight(a, b, &inner_ab) {
                continue;
            }
            // A one-way chain back from b to a, through other nodes.
            let back_starts: Vec<usize> = adj[b].keys().copied().collect();
            let back = back_starts.into_iter().filter_map(|f| follow(adj, b, f)).find(|(end, inner, _)| {
                *end == a && !inner.is_empty() && inner.iter().all(|v| !inner_ab.contains(v)) && straight(b, a, inner)
            });
            let Some((_, inner_ba, lanes_ba)) = back else { continue };
            if adj[a].contains_key(&b) {
                continue;
            }
            for &v in inner_ab.iter().chain(&inner_ba) {
                let nbrs: Vec<usize> = adj[v].keys().copied().collect();
                for w in nbrs {
                    adj[w].remove(&v);
                }
                adj[v].clear();
            }
            adj[a].insert(b, (lanes_ab, lanes_ba));
            adj[b].insert(a, (lanes_ba, lanes_ab));
            merged += 1;
        }
    }
    merged
}

/// `bpx traffic-json`: Burnout traffic for the viewer traffic editor
/// (paradise-viewer's traffic.js), unedited and in Burnout coordinates (y up):
/// `{"lanes": [{"id": "hull:section", "pts": [x, y, z, ...] lane centre,
/// "width": m, "speed": m/s, "traffic": vehicles per minute, "next": [ids]}], "parked": [{"id": "hull:index",
/// "pos": [x, y, z], "dir": [dx, dz]}]}`.
pub fn export_json(game: &Path, out: &Path) -> Result<()> {
    let t = load_traffic(game)?;
    let r1 = |v: f32| (v * 10.0).round() / 10.0;
    let mut lanes = Vec::new();
    let mut parked = Vec::new();
    for (h, hull) in t.hulls.iter().enumerate() {
        for (s, sec) in hull.sections.iter().enumerate() {
            if sec.rungs.is_empty() {
                continue;
            }
            let pts: Vec<f32> = sec.rungs.iter().flat_map(|r| centre(r).map(r1)).collect();
            let mut widths: Vec<f32> = sec.rungs.iter().map(|r| ((r[0][0] - r[1][0]).powi(2) + (r[0][2] - r[1][2]).powi(2)).sqrt()).collect();
            widths.sort_by(f32::total_cmp);
            let next: Vec<String> = sec.forward.iter().map(|(fh, fs)| format!("{fh}:{fs}")).collect();
            lanes.push(serde_json::json!({"id": format!("{h}:{s}"), "pts": pts, "width": r1(widths[widths.len() / 2]), "speed": r1(sec.speed), "traffic": sec.vehicles_per_minute, "next": next}));
        }
        for (i, v) in hull.parked.iter().enumerate() {
            let m = &v.transform;
            parked.push(serde_json::json!({"id": format!("{h}:{i}"), "pos": [r1(m[12]), r1(m[13]), r1(m[14])], "dir": [(m[8] * 1000.0).round() / 1000.0, (m[10] * 1000.0).round() / 1000.0]}));
        }
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(out, serde_json::to_vec(&serde_json::json!({"lanes": lanes, "parked": parked}))?)?;
    println!("traffic-json: {} lanes, {} parked cars -> {}", lanes.len(), parked.len(), out.display());
    Ok(())
}
