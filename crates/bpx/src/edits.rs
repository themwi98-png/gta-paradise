//! Hand edits made in tools/viewer, applied whenever the GTA data is built.
//!
//! Collision: the viewer's collision editor exports deleted polygons as
//! `{"format": "paradise-collision-deletions-v1", "edits": [{"source":
//! "extra", "unit": 144, "polygon": 483}, ...]}`, where `polygon` is the
//! polygon's index in its unit's PolygonSoupList (the glTF "polygonIds" the
//! collision export writes). Saved as tools/edits/collision_edits.json (or
//! the file named by BPX_COLLISION_EDITS), they are skipped by every step
//! that reads the collision: GTA collision, water and navmesh.
//!
//! Traffic: the viewer's traffic editor exports lane and parked-car edits
//! (see TrafficEdits), saved as tools/edits/traffic_edits.json (or
//! BPX_TRAFFIC_EDITS) and applied before GTA roads and parked cars are built.
//!
//! Sidewalks: the viewer's sidewalk editor exports drawn shapes where the
//! pedestrian navmesh's footpath is removed or added (see SidewalkEdits),
//! saved as tools/edits/sidewalk_edits.json (or BPX_SIDEWALK_EDITS) and
//! applied by `bpx gta-navmesh`.
//!
//! Water: the viewer's water editor exports areas where no water should be
//! generated (see WaterEdits), saved as tools/edits/water_edits.json (or
//! BPX_WATER_EDITS) and applied by `bpx gta-water`.

use anyhow::{Context, Result};
use bpworld::traffic::{Section, StaticVehicle, Traffic};
use bpworld as bw;
use std::collections::HashSet;
use std::path::PathBuf;

pub const DEFAULT_COLLISION_EDITS: &str = "tools/edits/collision_edits.json";

#[derive(Default)]
pub struct CollisionEdits {
    deleted: HashSet<(u32, u32)>,
}

impl CollisionEdits {
    /// Loads the edits file if there is one (none: nothing deleted).
    pub fn load() -> Result<Self> {
        let path = std::env::var_os("BPX_COLLISION_EDITS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_COLLISION_EDITS));
        if !path.exists() {
            return Ok(Self::default());
        }
        let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?).with_context(|| format!("reading {}", path.display()))?;
        let mut deleted = HashSet::new();
        for e in doc["edits"].as_array().context("collision edits: no \"edits\" list")? {
            if e["source"].as_str().unwrap_or("extra") != "extra" {
                continue;
            }
            let unit = e["unit"].as_u64().context("collision edit without unit")? as u32;
            let polygon = e["polygon"].as_u64().context("collision edit without polygon")? as u32;
            deleted.insert((unit, polygon));
        }
        if !deleted.is_empty() {
            eprintln!("collision edits: {} deleted polygon(s) from {}", deleted.len(), path.display());
        }
        Ok(Self { deleted })
    }

    /// A unit's collision polygons with the deleted ones removed.
    pub fn polys(&self, unit: u32, polys: Vec<bw::CollisionPoly>) -> Vec<bw::CollisionPoly> {
        if self.deleted.is_empty() {
            return polys;
        }
        polys.into_iter().enumerate().filter(|(i, _)| !self.deleted.contains(&(unit, *i as u32))).map(|(_, p)| p).collect()
    }
}

pub const DEFAULT_TRAFFIC_EDITS: &str = "tools/edits/traffic_edits.json";

/// Snap a drawn lane's end to an existing lane end / start this close (m).
const SNAP: f32 = 3.0;
/// Drawn lanes are split into steps this long (m) that follow the road.
const DRAPE_STEP: f32 = 6.0;
/// From one step to the next the road rises or falls at most this much (m).
const DRAPE_CLIMB: f32 = 2.5;
/// Collision the drawn lanes are laid on (BPX_COL overrides).
const DEFAULT_COL: &str = "tools/collision/paradise_extra.pcx";

/// Lays a drawn lane on the road surface: long straight bits are split into
/// DRAPE_STEP steps and each step takes the collision floor nearest the
/// height of the step before (so a lane drawn straight across a curved
/// bridge follows the deck, not a line under it, and an overpass above or a
/// road below isn't jumped to). The clicked points stay where they are.
/// Points are Burnout's (x, y up, z); `floors` are in Burnout's x / -z
/// plane with y heights.
fn drape(pts: &[[f32; 3]], floors: &crate::props::Floors) -> Vec<[f32; 3]> {
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b[0] - a[0]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
        let steps = (len / DRAPE_STEP).ceil().max(1.0) as usize;
        for k in 1..steps {
            let t = k as f32 / steps as f32;
            let (x, z) = (a[0] + (b[0] - a[0]) * t, a[2] + (b[2] - a[2]) * t);
            let prev = out.last().unwrap()[1];
            let y = floors
                .heights(x, -z)
                .into_iter()
                .filter(|h| (h - prev).abs() <= DRAPE_CLIMB)
                .min_by(|p, q| (p - prev).abs().total_cmp(&(q - prev).abs()))
                .unwrap_or(a[1] + (b[1] - a[1]) * t);
            out.push([x, y, z]);
        }
        out.push(b);
    }
    out
}

/// Traffic edits from the viewer's traffic editor (`paradise-traffic-edits-v1`),
/// in Burnout coordinates (y up), keyed by Burnout lane id "hull:section" and
/// parked car id "hull:index":
///
/// ```json
/// { "lanes": { "217:36": { "reverse": true, "delete": false, "shift": 0.5, "speed": 13.4 } },
///   "connections": [ { "from": "217:36", "to": "217:40" } ],
///   "newLanes": [ { "points": [[x, y, z], ...], "width": 4.5, "speed": 13.4 } ],
///   "parked": { "12:3": { "delete": true }, "12:4": { "pos": [x, y, z], "dir": [dx, dz] } },
///   "newParked": [ { "pos": [x, y, z], "dir": [dx, dz] } ] }
/// ```
///
/// `shift` moves a lane sideways (metres, + = to the right of its driving
/// direction). A connection is a short lane from one lane's end to another's
/// start. Drawn lanes snap to lane ends / starts within SNAP metres.
pub struct TrafficEdits {
    doc: serde_json::Value,
    path: PathBuf,
}

/// Right of the driving direction in Burnout's x/z plane, for a lane heading
/// along (tx, tz): GTA maps Burnout (x, y, z) to (x, -z, y).
pub fn right_of(t: [f32; 2]) -> [f32; 2] {
    let l = (t[0] * t[0] + t[1] * t[1]).sqrt().max(1e-6);
    [-t[1] / l, t[0] / l]
}

fn centre(r: &[[f32; 3]; 2]) -> [f32; 3] {
    [(r[0][0] + r[1][0]) * 0.5, (r[0][1] + r[1][1]) * 0.5, (r[0][2] + r[1][2]) * 0.5]
}

fn parse_id(s: &str) -> Option<(usize, usize)> {
    let (a, b) = s.split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn vec3(v: &serde_json::Value) -> Option<[f32; 3]> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
}

fn dir2(v: &serde_json::Value) -> Option<[f32; 2]> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32])
}

/// Sets a parked car's position and / or facing (rows: 0 side, 1 up,
/// 2 forward, 3 position).
fn set_transform(m: &mut [f32; 16], pos: Option<[f32; 3]>, dir: Option<[f32; 2]>) {
    if let Some(p) = pos {
        m[12] = p[0];
        m[13] = p[1];
        m[14] = p[2];
    }
    if let Some(d) = dir {
        let l = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-6);
        let (fx, fz) = (d[0] / l, d[1] / l);
        m[0..3].copy_from_slice(&[fz, 0.0, -fx]);
        m[4..7].copy_from_slice(&[0.0, 1.0, 0.0]);
        m[8..11].copy_from_slice(&[fx, 0.0, fz]);
    }
}

fn blank_section(rungs: Vec<[[f32; 3]; 2]>, speed: f32) -> Section {
    Section { rungs, forward: Vec::new(), backward: Vec::new(), left: Vec::new(), right: Vec::new(), speed, length: 0.0, turn_left_prob: 0, turn_right_prob: 0 }
}

impl TrafficEdits {
    /// Loads the edits file, if there is one.
    pub fn load() -> Result<Option<Self>> {
        let path = std::env::var_os("BPX_TRAFFIC_EDITS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_TRAFFIC_EDITS));
        if !path.exists() {
            return Ok(None);
        }
        let doc = serde_json::from_slice(&std::fs::read(&path)?).with_context(|| format!("reading {}", path.display()))?;
        Ok(Some(Self { doc, path }))
    }

    /// Applies the edits to Burnout's traffic (before any placement).
    pub fn apply(&self, t: &mut Traffic) {
        let (mut reversed, mut deleted, mut shifted, mut speeds) = (0, 0, 0, 0);
        let mut deleted_ids = HashSet::new();
        if let Some(lanes) = self.doc["lanes"].as_object() {
            for (id, e) in lanes {
                let Some((h, s)) = parse_id(id) else { continue };
                let Some(sec) = t.hulls.get_mut(h).and_then(|hull| hull.sections.get_mut(s)) else { continue };
                if e["delete"].as_bool() == Some(true) {
                    sec.rungs.clear();
                    sec.forward.clear();
                    sec.backward.clear();
                    deleted_ids.insert((h as u16, s as u8));
                    deleted += 1;
                    continue;
                }
                if e["reverse"].as_bool() == Some(true) {
                    sec.rungs.reverse();
                    for r in &mut sec.rungs {
                        r.swap(0, 1);
                    }
                    std::mem::swap(&mut sec.forward, &mut sec.backward);
                    reversed += 1;
                }
                if let Some(d) = e["shift"].as_f64().filter(|d| *d != 0.0) {
                    let c: Vec<[f32; 3]> = sec.rungs.iter().map(centre).collect();
                    let n = c.len();
                    for (k, r) in sec.rungs.iter_mut().enumerate() {
                        let (a, b) = if k + 1 < n {
                            (c[k], c[k + 1])
                        } else if k > 0 {
                            (c[k - 1], c[k])
                        } else {
                            continue;
                        };
                        let right = right_of([b[0] - a[0], b[2] - a[2]]);
                        for p in r.iter_mut() {
                            p[0] += right[0] * d as f32;
                            p[2] += right[1] * d as f32;
                        }
                    }
                    shifted += 1;
                }
                if let Some(v) = e["speed"].as_f64() {
                    sec.speed = v as f32;
                    speeds += 1;
                }
            }
        }
        for hull in &mut t.hulls {
            for sec in &mut hull.sections {
                sec.forward.retain(|r| !deleted_ids.contains(r));
                sec.backward.retain(|r| !deleted_ids.contains(r));
            }
        }

        // Connections, and drawn lanes snapped to lane ends / starts.
        let lane = |t: &Traffic, id: &str| parse_id(id).and_then(|(h, s)| t.hulls.get(h)?.sections.get(s).filter(|sec| !sec.rungs.is_empty()).cloned());
        let mut added: Vec<Section> = Vec::new();
        for c in self.doc["connections"].as_array().into_iter().flatten() {
            let (Some(a), Some(b)) = (c["from"].as_str().and_then(|i| lane(t, i)), c["to"].as_str().and_then(|i| lane(t, i))) else { continue };
            added.push(blank_section(vec![*a.rungs.last().unwrap(), b.rungs[0]], a.speed.min(b.speed)));
        }
        let connections = added.len();
        let ends: Vec<([f32; 3], [[f32; 3]; 2])> = t.hulls.iter().flat_map(|h| &h.sections).filter_map(|s| s.rungs.last().map(|r| (centre(r), *r))).collect();
        let starts: Vec<([f32; 3], [[f32; 3]; 2])> = t.hulls.iter().flat_map(|h| &h.sections).filter_map(|s| s.rungs.first().map(|r| (centre(r), *r))).collect();
        let nearest = |list: &[([f32; 3], [[f32; 3]; 2])], p: [f32; 3]| {
            list.iter()
                .map(|(c, r)| (((c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2) + (c[2] - p[2]).powi(2)).sqrt(), *r))
                .filter(|(d, _)| *d <= SNAP)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, r)| r)
        };
        let new_lanes: Vec<&serde_json::Value> = self.doc["newLanes"].as_array().into_iter().flatten().collect();
        let floors = if new_lanes.is_empty() {
            None
        } else {
            let col = std::env::var_os("BPX_COL").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_COL));
            let identity = crate::gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
            match crate::props::Floors::load(&col, &identity) {
                Ok(f) => Some(f),
                Err(e) => {
                    eprintln!("drawn lanes not laid on the road ({}: {e:#})", col.display());
                    None
                }
            }
        };
        for l in new_lanes {
            let pts: Vec<[f32; 3]> = l["points"].as_array().into_iter().flatten().filter_map(vec3).collect();
            if pts.len() < 2 {
                continue;
            }
            let pts = match &floors {
                Some(f) => drape(&pts, f),
                None => pts,
            };
            let half = l["width"].as_f64().unwrap_or(4.5) as f32 * 0.5;
            let n = pts.len();
            let mut rungs: Vec<[[f32; 3]; 2]> = (0..n)
                .map(|k| {
                    let (a, b) = if k + 1 < n { (pts[k], pts[k + 1]) } else { (pts[k - 1], pts[k]) };
                    let r = right_of([b[0] - a[0], b[2] - a[2]]);
                    let p = pts[k];
                    [[p[0] - r[0] * half, p[1], p[2] - r[1] * half], [p[0] + r[0] * half, p[1], p[2] + r[1] * half]]
                })
                .collect();
            if let Some(r) = nearest(&ends, pts[0]) {
                rungs[0] = r;
            }
            if let Some(r) = nearest(&starts, pts[n - 1]) {
                rungs[n - 1] = r;
            }
            added.push(blank_section(rungs, l["speed"].as_f64().unwrap_or(13.4) as f32));
        }
        let drawn = added.len() - connections;
        if let Some(h0) = t.hulls.first_mut() {
            h0.sections.extend(added);
        }

        // Parked cars.
        let (mut parked_deleted, mut parked_moved, mut parked_added) = (0, 0, 0);
        let mut gone: HashSet<(usize, usize)> = HashSet::new();
        if let Some(parked) = self.doc["parked"].as_object() {
            for (id, e) in parked {
                let Some((h, i)) = parse_id(id) else { continue };
                let Some(v) = t.hulls.get_mut(h).and_then(|hull| hull.parked.get_mut(i)) else { continue };
                if e["delete"].as_bool() == Some(true) {
                    gone.insert((h, i));
                    parked_deleted += 1;
                    continue;
                }
                let (pos, dir) = (vec3(&e["pos"]), dir2(&e["dir"]));
                if pos.is_some() || dir.is_some() {
                    set_transform(&mut v.transform, pos, dir);
                    parked_moved += 1;
                }
            }
        }
        for (h, hull) in t.hulls.iter_mut().enumerate() {
            let mut i = 0;
            hull.parked.retain(|_| {
                let keep = !gone.contains(&(h, i));
                i += 1;
                keep
            });
        }
        if let Some(h0) = t.hulls.first_mut() {
            for e in self.doc["newParked"].as_array().into_iter().flatten() {
                let Some(pos) = vec3(&e["pos"]) else { continue };
                let mut transform = [0.0f32; 16];
                transform[15] = 1.0;
                set_transform(&mut transform, Some(pos), Some(dir2(&e["dir"]).unwrap_or([1.0, 0.0])));
                h0.parked.push(StaticVehicle { transform, flow_type: 0, exists_chance: 100, flags: 1 });
                parked_added += 1;
            }
        }
        eprintln!(
            "traffic edits ({}): {reversed} reversed, {deleted} deleted, {shifted} shifted, {speeds} speeds, {connections} connections, {drawn} drawn lanes; parked {parked_deleted} deleted, {parked_moved} moved, {parked_added} added",
            self.path.display()
        );
    }
}

pub const DEFAULT_WATER_EDITS: &str = "tools/edits/water_edits.json";

/// Areas with no water, in GTA world coordinates:
///
/// ```json
/// { "format": "paradise-water-edits-v1", "deleted": [[minX, minY, maxX, maxY, z], ...] }
/// ```
///
/// Stored as areas rather than generated quads, so they still apply when
/// the water is regenerated (e.g. after collision edits).
#[derive(Default)]
pub struct WaterEdits {
    deleted: Vec<[f32; 4]>,
}

impl WaterEdits {
    /// Loads the edits file if there is one (none: nothing deleted).
    pub fn load() -> Result<Self> {
        let path = std::env::var_os("BPX_WATER_EDITS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_WATER_EDITS));
        if !path.exists() {
            return Ok(Self::default());
        }
        let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?).with_context(|| format!("reading {}", path.display()))?;
        let mut deleted = Vec::new();
        for r in doc["deleted"].as_array().context("water edits: no \"deleted\" list")? {
            let v: Vec<f32> = r.as_array().into_iter().flatten().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
            // A fifth value (the water height there) is only for the viewer.
            anyhow::ensure!(v.len() >= 4, "water edit area needs [minX, minY, maxX, maxY]");
            deleted.push([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])]);
        }
        if !deleted.is_empty() {
            eprintln!("water edits: {} deleted area(s) from {}", deleted.len(), path.display());
        }
        Ok(Self { deleted })
    }

    /// Is the point (GTA x, y) in a deleted area?
    pub fn deleted(&self, x: f32, y: f32) -> bool {
        self.deleted.iter().any(|r| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3])
    }
}

pub const DEFAULT_SIDEWALK_EDITS: &str = "tools/edits/sidewalk_edits.json";

/// Drawn shapes (GTA world points, outline order) where the pedestrian
/// navmesh's sidewalk (footpath) is taken away or added:
///
/// ```json
/// { "format": "paradise-sidewalk-edits-v1",
///   "remove": [[[x, y, z], [x, y, z], ...], ...],
///   "add":    [[[x, y, z], ...], ...] }
/// ```
///
/// Removed sidewalk becomes plain walkable ground; added sidewalk turns
/// ground or road inside the shape into footpath.
#[derive(Default)]
pub struct SidewalkEdits {
    pub remove: Vec<Vec<[f32; 3]>>,
    pub add: Vec<Vec<[f32; 3]>>,
}

impl SidewalkEdits {
    /// Loads the edits file if there is one (none: no changes).
    pub fn load() -> Result<Self> {
        let path = std::env::var_os("BPX_SIDEWALK_EDITS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_SIDEWALK_EDITS));
        if !path.exists() {
            return Ok(Self::default());
        }
        let doc: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?).with_context(|| format!("reading {}", path.display()))?;
        let shapes = |key: &str| -> Vec<Vec<[f32; 3]>> {
            doc[key]
                .as_array()
                .into_iter()
                .flatten()
                .map(|shape| {
                    shape
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|p| {
                            let a = p.as_array()?;
                            Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32])
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|s: &Vec<[f32; 3]>| s.len() >= 3)
                .collect()
        };
        let edits = Self { remove: shapes("remove"), add: shapes("add") };
        if !edits.remove.is_empty() || !edits.add.is_empty() {
            eprintln!("sidewalk edits: {} removed, {} added shape(s) from {}", edits.remove.len(), edits.add.len(), path.display());
        }
        Ok(edits)
    }
}
