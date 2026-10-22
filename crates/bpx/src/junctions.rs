//! Road junctions, for paradise.asi's traffic control and the viewer's
//! Junctions panel. Three kinds:
//!
//! - Burnout's own traffic lights (TrafficData junctions; id `L` + Burnout's
//!   id in hex): the light cycle (each state's time and which light sets it
//!   stops), the stop lines each light set holds and the traffic light
//!   models it switches.
//! - Junctions without lights, found where lanes cross (id `A` + position):
//!   traffic there gives way to cars already in the junction.
//! - Junctions marked by hand in the viewer (`M`), from
//!   tools/edits/junction_edits.json, which can also switch the others off
//!   or resize them.
//!
//! `bpx gta-junctions <game-dir> <out-dir>` writes `junctions.json` (every
//! built-in junction, without the hand edits, for the viewer) and
//! `paradise_junctions.txt` (with them, for the plugin; installed next to
//! GTA5.exe). Both are in GTA world coordinates.

use crate::edits::JunctionEdits;
use crate::gta::Placement;
use crate::paths::load_traffic_edited;
use anyhow::Result;
use bpworld::traffic::{Section, Traffic};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// How far back along the lanes from a stop line cars count as waiting at it (m).
const BACK_LEN: f32 = 70.0;
/// A junction reaches this far (m) past its outermost stop line or crossing.
const LIT_MARGIN: f32 = 3.0;
const FOUND_MARGIN: f32 = 6.0;
const MIN_RADIUS: f32 = 8.0;
const MAX_RADIUS: f32 = 45.0;
/// Crossing lanes at heights this far apart (m) are on a bridge, not a junction.
const SAME_LEVEL: f32 = 3.0;
/// Lanes crossing at less than this angle (sine) are just drifting together.
const MIN_CROSS_SIN: f32 = 0.34;
/// Crossings this close (m) belong to one junction.
const CLUSTER: f32 = 18.0;
/// Burnout's light state times are in tenths of a second.
const STATE_TIME: f32 = 0.1;

struct Stop {
    controller: usize,
    pos: [f32; 3],
    dir: [f32; 2],
    half_width: f32,
    /// From the stop line back along the lane (and the lane before it).
    back: Vec<[f32; 3]>,
}

struct Junction {
    id: String,
    /// 'L' Burnout's lights, 'A' found, 'M' marked by hand.
    kind: char,
    centre: [f32; 3],
    radius: f32,
    /// (seconds, stopped light sets bit mask).
    states: Vec<(f32, u8)>,
    stops: Vec<Stop>,
    lights: Vec<Light>,
}

/// A traffic light: the GTA traffic light prop placed for it (gta-props),
/// whose lamps the plugin switches, or, with none, Burnout's own lamp
/// positions (colour 0 red, 1 amber, 2 green) for the plugin to draw.
struct Light {
    set: usize,
    pos: [f32; 3],
    prop: Option<(String, [f32; 3])>,
    lamps: Vec<(u8, [f32; 3])>,
}

/// A GTA traffic light prop this close (m) to a Burnout light is its stand-in.
const PROP_MATCH: f32 = 8.0;

/// Placed GTA traffic light props (position, model) from gta-props' list.
fn traffic_props(csv: &Path) -> Vec<([f32; 3], String)> {
    let Ok(text) = std::fs::read_to_string(csv) else {
        eprintln!("junctions: no {} (traffic lights won't switch their GTA props)", csv.display());
        return Vec::new();
    };
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            let model = f.get(3)?.trim();
            if !(model.starts_with("prop_traffic_0") || model.starts_with("prop_traffic_lightset")) {
                return None;
            }
            Some(([f[0].parse().ok()?, f[1].parse().ok()?, f[2].parse().ok()?], model.to_string()))
        })
        .collect()
}

fn centre(r: &[[f32; 3]; 2]) -> [f32; 3] {
    [(r[0][0] + r[1][0]) * 0.5, (r[0][1] + r[1][1]) * 0.5, (r[0][2] + r[1][2]) * 0.5]
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn section(t: &Traffic, h: u16, s: u8) -> Option<&Section> {
    t.hulls.get(h as usize)?.sections.get(s as usize).filter(|sec| !sec.rungs.is_empty())
}

/// A stop line on a lane: Burnout stores where it is as the rung number in
/// 8.8 fixed point (side-by-side lanes' stop lines line up to ~0.2 m read
/// that way; read as metres along the lane they're metres apart).
fn stop_on(t: &Traffic, sec: &Section, param: u16, controller: usize, place: &Placement) -> Option<Stop> {
    let c: Vec<[f32; 3]> = sec.rungs.iter().map(|r| place.apply(centre(r))).collect();
    if c.len() < 2 {
        return None;
    }
    let r = param as f32 / 256.0;
    let i = (r.floor() as usize).min(c.len() - 2);
    let f = (r - i as f32).clamp(0.0, 1.0);
    let pos = lerp(c[i], c[i + 1], f);
    let d = [c[i + 1][0] - c[i][0], c[i + 1][1] - c[i][1]];
    let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-3);
    let rung = &sec.rungs[i];
    let half_width = ((rung[0][0] - rung[1][0]).powi(2) + (rung[0][2] - rung[1][2]).powi(2)).sqrt() * 0.5;
    // Back from the stop line: this lane's rungs, then the lane before it
    // (the one ending where this one starts).
    let mut back = vec![pos];
    let mut run = 0.0;
    let mut pts: Vec<[f32; 3]> = c[..=i].iter().rev().copied().collect();
    let mut lane = sec;
    for _ in 0..8 {
        for p in pts {
            let step = dist2(*back.last().unwrap(), p);
            if step < 0.5 {
                continue;
            }
            run += step;
            back.push(p);
            if run >= BACK_LEN {
                break;
            }
        }
        if run >= BACK_LEN {
            break;
        }
        let first = place.apply(centre(&lane.rungs[0]));
        let prev = lane
            .backward
            .iter()
            .filter_map(|&(h, s)| section(t, h, s))
            .map(|p| (p, dist2(place.apply(centre(p.rungs.last().unwrap())), first)))
            .filter(|(_, gap)| *gap < 4.0)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((p, _)) = prev else { break };
        lane = p;
        pts = lane.rungs.iter().rev().map(|r| place.apply(centre(r))).collect();
    }
    Some(Stop { controller, pos, dir: [d[0] / l, d[1] / l], half_width: half_width.clamp(1.5, 4.0), back })
}

/// Burnout's traffic light junctions.
fn lit_junctions(t: &Traffic, place: &Placement, props: &[([f32; 3], String)]) -> Vec<Junction> {
    let mut out = Vec::new();
    for hull in &t.hulls {
        for j in &hull.junctions {
            let mut stops = Vec::new();
            let mut lights = Vec::new();
            for (k, ctl) in j.controllers.iter().enumerate() {
                for &(h, s) in &ctl.stop_lines {
                    let Some(h_ref) = t.hulls.get(h as usize) else { continue };
                    let Some(&param) = h_ref.stop_lines.get(s as usize) else { continue };
                    let Some(sec) = h_ref.sections.iter().find(|sec| sec.stop_lines.contains(&s) && !sec.rungs.is_empty()) else { continue };
                    stops.extend(stop_on(t, sec, param, k, place));
                }
                for &l in &ctl.lights {
                    let Some(light) = t.lights.get(l as usize) else { continue };
                    let pos = place.apply(light.position);
                    let prop = props
                        .iter()
                        .map(|(p, m)| (((p[0] - pos[0]).powi(2) + (p[1] - pos[1]).powi(2) + (p[2] - pos[2]).powi(2)).sqrt(), p, m))
                        .filter(|(d, _, _)| *d < PROP_MATCH)
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                        .map(|(_, p, m)| (m.clone(), *p));
                    // Lamps: offsets in the light's own axes, turned by its
                    // rotation about Burnout's up axis.
                    let (sin, cos) = light.y_rotation.sin_cos();
                    let lamps = if prop.is_some() {
                        Vec::new()
                    } else {
                        t.light_kinds
                            .get(light.kind as usize)
                            .into_iter()
                            .flatten()
                            .map(|&(colour, o)| {
                                let local = [o[0] * cos + o[2] * sin, o[1], -o[0] * sin + o[2] * cos];
                                (colour, place.apply([light.position[0] + local[0], light.position[1] + local[1], light.position[2] + local[2]]))
                            })
                            .collect()
                    };
                    lights.push(Light { set: k, pos, prop, lamps });
                }
            }
            if stops.is_empty() {
                continue;
            }
            let n = stops.len() as f32;
            let c = stops.iter().fold([0.0; 3], |a, s| [a[0] + s.pos[0] / n, a[1] + s.pos[1] / n, a[2] + s.pos[2] / n]);
            let radius = stops.iter().map(|s| dist2(s.pos, c)).fold(0.0, f32::max) + LIT_MARGIN;
            out.push(Junction {
                id: format!("L{:x}", j.id),
                kind: 'L',
                centre: c,
                radius: radius.clamp(MIN_RADIUS, MAX_RADIUS),
                states: j.states.iter().map(|&(time, mask)| (time as f32 * STATE_TIME, mask)).collect(),
                stops,
                lights,
            });
        }
    }
    out
}

/// Junctions without lights: where lanes cross at one level, away from the
/// lit junctions.
fn found_junctions(t: &Traffic, place: &Placement, lit: &[Junction]) -> Vec<Junction> {
    // Lanes as indices, with the lanes each one joins (ahead, behind, beside).
    let mut index: HashMap<(u16, u8), usize> = HashMap::new();
    let mut lanes: Vec<&Section> = Vec::new();
    for (h, hull) in t.hulls.iter().enumerate() {
        for (s, sec) in hull.sections.iter().enumerate() {
            if !sec.rungs.is_empty() {
                index.insert((h as u16, s as u8), lanes.len());
                lanes.push(sec);
            }
        }
    }
    let mut joined: HashSet<(usize, usize)> = HashSet::new();
    for (h, hull) in t.hulls.iter().enumerate() {
        for (s, sec) in hull.sections.iter().enumerate() {
            let Some(&a) = index.get(&(h as u16, s as u8)) else { continue };
            let others = sec.forward.iter().chain(&sec.backward).copied().chain(sec.left.iter().chain(&sec.right).map(|nb| (h as u16, nb.section)));
            for r in others {
                if let Some(&b) = index.get(&r) {
                    joined.insert((a.min(b), a.max(b)));
                }
            }
        }
    }
    // Lane centre line segments, bucketed on a grid.
    const CELL: f32 = 16.0;
    let mut segs: Vec<(usize, [f32; 3], [f32; 3])> = Vec::new();
    for (lane, sec) in lanes.iter().enumerate() {
        let c: Vec<[f32; 3]> = sec.rungs.iter().map(|r| place.apply(centre(r))).collect();
        for w in c.windows(2) {
            segs.push((lane, w[0], w[1]));
        }
    }
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, (_, a, b)) in segs.iter().enumerate() {
        let (x0, x1) = ((a[0].min(b[0]) / CELL).floor() as i32, (a[0].max(b[0]) / CELL).floor() as i32);
        let (y0, y1) = ((a[1].min(b[1]) / CELL).floor() as i32, (a[1].max(b[1]) / CELL).floor() as i32);
        for x in x0..=x1 {
            for y in y0..=y1 {
                grid.entry((x, y)).or_default().push(i);
            }
        }
    }
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut crossings: Vec<[f32; 3]> = Vec::new();
    for list in grid.values() {
        for (k, &i) in list.iter().enumerate() {
            for &j in &list[k + 1..] {
                let (la, a0, a1) = segs[i];
                let (lb, b0, b1) = segs[j];
                if la == lb || joined.contains(&(la.min(lb), la.max(lb))) || !seen.insert((i.min(j), i.max(j))) {
                    continue;
                }
                let da = [a1[0] - a0[0], a1[1] - a0[1]];
                let db = [b1[0] - b0[0], b1[1] - b0[1]];
                let den = da[0] * db[1] - da[1] * db[0];
                let (la_len, lb_len) = ((da[0] * da[0] + da[1] * da[1]).sqrt(), (db[0] * db[0] + db[1] * db[1]).sqrt());
                if den.abs() < MIN_CROSS_SIN * la_len * lb_len || la_len < 1e-3 || lb_len < 1e-3 {
                    continue;
                }
                let w = [b0[0] - a0[0], b0[1] - a0[1]];
                let ta = (w[0] * db[1] - w[1] * db[0]) / den;
                let tb = (w[0] * da[1] - w[1] * da[0]) / den;
                if !(0.0..=1.0).contains(&ta) || !(0.0..=1.0).contains(&tb) {
                    continue;
                }
                let (pa, pb) = (lerp(a0, a1, ta), lerp(b0, b1, tb));
                if (pa[2] - pb[2]).abs() > SAME_LEVEL {
                    continue;
                }
                crossings.push(lerp(pa, pb, 0.5));
            }
        }
    }
    // Crossings near each other make one junction.
    let mut parent: Vec<usize> = (0..crossings.len()).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        let mut i = i;
        while p[i] != r {
            let next = p[i];
            p[i] = r;
            i = next;
        }
        r
    }
    let mut cgrid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, p) in crossings.iter().enumerate() {
        cgrid.entry(((p[0] / CLUSTER).floor() as i32, (p[1] / CLUSTER).floor() as i32)).or_default().push(i);
    }
    for (i, p) in crossings.iter().enumerate() {
        let (cx, cy) = ((p[0] / CLUSTER).floor() as i32, (p[1] / CLUSTER).floor() as i32);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &j in cgrid.get(&(cx + dx, cy + dy)).into_iter().flatten() {
                    if j > i && dist2(*p, crossings[j]) <= CLUSTER && (p[2] - crossings[j][2]).abs() <= SAME_LEVEL {
                        let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                        parent[a] = b;
                    }
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<[f32; 3]>> = HashMap::new();
    for (i, p) in crossings.iter().enumerate() {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(*p);
    }
    let mut out: Vec<Junction> = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();
    let mut groups: Vec<Vec<[f32; 3]>> = groups.into_values().collect();
    groups.sort_by(|a, b| a[0][0].total_cmp(&b[0][0]).then(a[0][1].total_cmp(&b[0][1])));
    for pts in groups {
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &pts {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        let z = pts.iter().map(|p| p[2]).sum::<f32>() / pts.len() as f32;
        let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, z];
        if lit.iter().any(|j| dist2(j.centre, c) < j.radius + 5.0 && (j.centre[2] - c[2]).abs() < 10.0) {
            continue;
        }
        let radius = (pts.iter().map(|p| dist2(*p, c)).fold(0.0, f32::max) + FOUND_MARGIN).clamp(MIN_RADIUS, MAX_RADIUS);
        let mut id = format!("A{:.0}_{:.0}", c[0], c[1]);
        while !ids.insert(id.clone()) {
            id.push('b');
        }
        out.push(Junction { id, kind: 'A', centre: c, radius, states: Vec::new(), stops: Vec::new(), lights: Vec::new() });
    }
    println!("junctions: {} lane crossings at one level -> {} junctions without lights", crossings.len(), out.len());
    out
}

fn json_point(p: [f32; 3]) -> String {
    format!("[{:.2},{:.2},{:.2}]", p[0], p[1], p[2])
}

/// `bpx gta-junctions`.
pub fn export(game: &Path, out_dir: &Path, place: Placement, props_csv: &Path) -> Result<()> {
    let t = load_traffic_edited(game)?;
    let props = traffic_props(props_csv);
    let lit = lit_junctions(&t, &place, &props);
    let found = found_junctions(&t, &place, &lit);
    let stops: usize = lit.iter().map(|j| j.stops.len()).sum();
    let lights: usize = lit.iter().map(|j| j.lights.len()).sum();
    let with_prop = lit.iter().flat_map(|j| &j.lights).filter(|l| l.prop.is_some()).count();
    println!("junctions: {} with Burnout's traffic lights ({stops} stop lines, {lights} lights, {with_prop} of them GTA props)", lit.len());
    std::fs::create_dir_all(out_dir)?;

    // For the viewer: all built-in junctions, unedited.
    let mut js = String::new();
    let _ = write!(js, "{{\"placement\":{{\"offset\":[{},{},{}],\"scale\":{},\"heading\":{}}},\"junctions\":[", place.offset[0], place.offset[1], place.offset[2], place.scale, place.heading);
    for (k, j) in lit.iter().chain(&found).enumerate() {
        if k > 0 {
            js.push(',');
        }
        let states: Vec<String> = j.states.iter().map(|(s, m)| format!("[{s:.1},{m}]")).collect();
        let stops: Vec<String> = j
            .stops
            .iter()
            .map(|s| format!("{{\"c\":{},\"pos\":{},\"dir\":[{:.3},{:.3}],\"w\":{:.2}}}", s.controller, json_point(s.pos), s.dir[0], s.dir[1], s.half_width))
            .collect();
        let lights: Vec<String> = j.lights.iter().map(|l| format!("{{\"c\":{},\"pos\":{}}}", l.set, json_point(l.pos))).collect();
        let _ = write!(
            js,
            "\n{{\"id\":\"{}\",\"kind\":\"{}\",\"pos\":{},\"radius\":{:.1},\"states\":[{}],\"stops\":[{}],\"lights\":[{}]}}",
            j.id,
            if j.kind == 'L' { "lights" } else { "found" },
            json_point(j.centre),
            j.radius,
            states.join(","),
            stops.join(","),
            lights.join(",")
        );
    }
    js.push_str("\n]}\n");
    std::fs::write(out_dir.join("junctions.json"), js)?;

    // For the plugin: with the hand edits.
    let edits = JunctionEdits::load()?;
    let mut all: Vec<Junction> = lit.into_iter().chain(found).filter(|j| !edits.off.contains(&j.id)).collect();
    for j in &mut all {
        if let Some(&r) = edits.radius.get(&j.id) {
            j.radius = r.clamp(2.0, 100.0);
        }
    }
    for (i, a) in edits.added.iter().enumerate() {
        all.push(Junction { id: format!("M{i}"), kind: 'M', centre: [a[0], a[1], a[2]], radius: a[3].clamp(2.0, 100.0), states: Vec::new(), stops: Vec::new(), lights: Vec::new() });
    }
    let mut txt = String::from(
        "# GTA Paradise junctions, made by bpx gta-junctions (Burnout's traffic lights, junctions\n\
         # found where lanes cross, and ones marked in the viewer). Read by paradise.asi.\n\
         # J id kind(L lights, A found, M marked) x y z radius\n\
         # S seconds stopped-light-sets-mask      (light states, in order)\n\
         # T light-set x y z dirx diry half-width  then the lane back from it: x y z ...\n\
         # G light-set model x y z                 (a traffic light: the GTA prop standing for it)\n\
         # C light-set colour(0 red 1 amber 2 green) x y z   (a lamp of a light with no GTA prop)\n",
    );
    for j in &all {
        let _ = writeln!(txt, "J {} {} {:.2} {:.2} {:.2} {:.1}", j.id, j.kind, j.centre[0], j.centre[1], j.centre[2], j.radius);
        for (s, m) in &j.states {
            let _ = writeln!(txt, "S {s:.1} {m}");
        }
        for s in &j.stops {
            let _ = write!(txt, "T {} {:.2} {:.2} {:.2} {:.3} {:.3} {:.2}", s.controller, s.pos[0], s.pos[1], s.pos[2], s.dir[0], s.dir[1], s.half_width);
            for p in &s.back[1..] {
                let _ = write!(txt, " {:.1} {:.1} {:.1}", p[0], p[1], p[2]);
            }
            txt.push('\n');
        }
        for l in &j.lights {
            if let Some((model, p)) = &l.prop {
                let _ = writeln!(txt, "G {} {model} {:.2} {:.2} {:.2}", l.set, p[0], p[1], p[2]);
            }
            for (colour, p) in &l.lamps {
                let _ = writeln!(txt, "C {} {colour} {:.2} {:.2} {:.2}", l.set, p[0], p[1], p[2]);
            }
        }
    }
    std::fs::write(out_dir.join("paradise_junctions.txt"), txt)?;
    let count = |k: char| all.iter().filter(|j| j.kind == k).count();
    println!(
        "junctions: {} for the plugin ({} lights, {} found, {} marked) -> {}",
        all.len(),
        count('L'),
        count('A'),
        count('M'),
        out_dir.join("paradise_junctions.txt").display()
    );
    Ok(())
}
