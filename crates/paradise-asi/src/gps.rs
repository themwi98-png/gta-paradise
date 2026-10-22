//! GPS: GTA's own route finder never finds a route on the Paradise roads
//! (GET_GPS_BLIP_ROUTE_FOUND stays false, though CALCULATE_TRAVEL_DISTANCE
//! finds one; every node and link flag that differs from Los Santos has
//! been ruled out). So the plugin finds the route itself, over
//! `paradise_roads.txt` (the same road network as GTA's, written by bpx
//! gta-paths; links only the way traffic may drive them), and draws it on
//! the radar and pause map as a custom GPS route in the waypoint's colour.
//!
//! - A route is found (A*, shortest by distance) when the player sets a
//!   waypoint, from the road nearest the player (preferring one going the
//!   way they're heading) to the road nearest the waypoint.
//! - Leaving it (over OFF_ROUTE m from it) finds a new one, at most every
//!   REROUTE_MS; so does moving the waypoint. Removing it clears the route.
//! - Logged: each route (length, points drawn) or why there's none.

use crate::natives as n;
use crate::shv::Vec3;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

const FILE: &str = "paradise_roads.txt";
/// Blip sprite of the player's waypoint.
const WAYPOINT_SPRITE: i32 = 8;
/// HUD_COLOUR_WAYPOINT, and the line's width on the radar and pause map.
const ROUTE_COLOUR: i32 = 83;
const RADAR_WIDTH: i32 = 16;
const MAP_WIDTH: i32 = 16;
/// Spatial grid cell for finding the nearest road (m).
const CELL: f32 = 64.0;
/// A start or end road this far (m) away is too far.
const NEAR: f32 = 300.0;
/// A road going against the player's heading counts as this much (m)
/// further away when choosing where the route starts.
const WRONG_WAY: f32 = 40.0;
const OFF_ROUTE: f32 = 40.0;
const REROUTE_MS: i32 = 1500;
/// Points closer together than this (m) along a straight stretch are merged.
const MIN_GAP: f32 = 25.0;
/// The most points given to the custom route.
const MAX_POINTS: usize = 400;

struct Roads {
    pos: Vec<Vec3>,
    /// Links that can be driven from each node: (to, length).
    out: Vec<Vec<(u32, f32)>>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

fn cell(x: f32, y: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (y / CELL).floor() as i32)
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

impl Roads {
    fn load() -> Option<Self> {
        let text = std::fs::read_to_string(FILE).ok()?;
        let mut pos = Vec::new();
        let mut out: Vec<Vec<(u32, f32)>> = Vec::new();
        for line in text.lines() {
            let p: Vec<&str> = line.split_whitespace().collect();
            match p.as_slice() {
                ["n", x, y, z] => {
                    pos.push(Vec3::new(x.parse().ok()?, y.parse().ok()?, z.parse().ok()?));
                    out.push(Vec::new());
                }
                ["l", a, b] => {
                    let (a, b): (usize, usize) = (a.parse().ok()?, b.parse().ok()?);
                    if a < pos.len() && b < pos.len() {
                        out[a].push((b as u32, dist(pos[a], pos[b])));
                    }
                }
                _ => {}
            }
        }
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, p) in pos.iter().enumerate() {
            grid.entry(cell(p.x, p.y)).or_default().push(i as u32);
        }
        let links: usize = out.iter().map(Vec::len).sum();
        crate::log(&format!("gps: {} road nodes, {links} drivable links from {FILE}", pos.len()));
        Some(Self { pos, out, grid })
    }

    /// The node near `p` with the lowest score (`score(node, distance)`),
    /// within NEAR m; heights count double, so a road on a bridge above
    /// isn't taken for the one below.
    fn nearest(&self, p: Vec3, score: impl Fn(usize, f32) -> f32) -> Option<usize> {
        let r = (NEAR / CELL).ceil() as i32;
        let (cx, cy) = cell(p.x, p.y);
        let mut best: Option<(f32, usize)> = None;
        for gx in cx - r..=cx + r {
            for gy in cy - r..=cy + r {
                for &i in self.grid.get(&(gx, gy)).into_iter().flatten() {
                    let q = self.pos[i as usize];
                    let d = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2) + (2.0 * (q.z - p.z)).powi(2)).sqrt();
                    if d > NEAR {
                        continue;
                    }
                    let s = score(i as usize, d);
                    if best.is_none_or(|(b, _)| s < b) {
                        best = Some((s, i as usize));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Shortest drivable route from node `a` to node `b` (A*), as nodes.
    fn route(&self, a: usize, b: usize) -> Option<Vec<usize>> {
        let goal = self.pos[b];
        let mut cost = vec![f32::INFINITY; self.pos.len()];
        let mut from = vec![u32::MAX; self.pos.len()];
        let mut open = BinaryHeap::new();
        cost[a] = 0.0;
        open.push((Reverse(dist(self.pos[a], goal).to_bits()), a as u32));
        while let Some((_, i)) = open.pop() {
            let i = i as usize;
            if i == b {
                let mut path = vec![b];
                let mut k = b;
                while k != a {
                    k = from[k] as usize;
                    path.push(k);
                }
                path.reverse();
                return Some(path);
            }
            for &(j, len) in &self.out[i] {
                let j = j as usize;
                let c = cost[i] + len;
                if c < cost[j] {
                    cost[j] = c;
                    from[j] = i as u32;
                    // Non-negative floats order like their bits.
                    open.push((Reverse((c + dist(self.pos[j], goal)).to_bits()), j as u32));
                }
            }
        }
        None
    }
}

#[derive(Default)]
pub struct GpsWatch {
    roads: Option<Roads>,
    loaded: bool,
    waypoint: Option<Vec3>,
    /// The route drawn: its points.
    route: Vec<Vec3>,
    last_route: i32,
}

impl GpsWatch {
    pub fn update(&mut self, player: Vec3, heading: f32, now: i32) {
        if !self.loaded {
            self.loaded = true;
            self.roads = Roads::load();
            if self.roads.is_none() {
                crate::log(&format!("gps: no {FILE}; no routes"));
            }
        }
        if !n::is_waypoint_active() {
            if self.waypoint.take().is_some() {
                n::clear_gps_custom_route();
                self.route.clear();
                crate::log("gps: waypoint removed, route cleared");
            }
            return;
        }
        let wp = n::blip_info_coords(n::first_blip_info_id(WAYPOINT_SPRITE));
        let moved = self.waypoint.is_none_or(|w| (w.x - wp.x).abs() + (w.y - wp.y).abs() > 1.0);
        let off = !self.route.is_empty() && off_route(&self.route, player) > OFF_ROUTE;
        if moved || ((off || self.route.is_empty()) && now - self.last_route >= REROUTE_MS) {
            self.waypoint = Some(wp);
            self.last_route = now;
            self.find(player, heading, wp, if moved { "new waypoint" } else if off { "off the route" } else { "no route yet" });
        }
    }

    fn find(&mut self, player: Vec3, heading: f32, wp: Vec3, why: &str) {
        let Some(roads) = &self.roads else { return };
        // GTA's heading: 0 = north (+y), anticlockwise.
        let (hx, hy) = (-heading.to_radians().sin(), heading.to_radians().cos());
        let start = roads.nearest(player, |i, d| {
            let p = roads.pos[i];
            let along = roads.out[i].iter().map(|&(j, len)| {
                let q = roads.pos[j as usize];
                ((q.x - p.x) * hx + (q.y - p.y) * hy) / len.max(0.1)
            });
            d + if along.fold(-1.0f32, f32::max) < 0.0 { WRONG_WAY } else { 0.0 }
        });
        // The waypoint has no real height: the nearest road by position.
        let end = roads.nearest(Vec3::new(wp.x, wp.y, player.z), |i, _| {
            let q = roads.pos[i];
            (q.x - wp.x).hypot(q.y - wp.y)
        });
        let (Some(a), Some(b)) = (start, end) else {
            n::clear_gps_custom_route();
            self.route.clear();
            crate::log(&format!("gps ({why}): no road within {NEAR} m of the {}", if start.is_none() { "player" } else { "waypoint" }));
            return;
        };
        let Some(path) = roads.route(a, b) else {
            n::clear_gps_custom_route();
            self.route.clear();
            crate::log(&format!("gps ({why}): no drivable route from road node {a} to {b}"));
            return;
        };
        let pts: Vec<Vec3> = path.iter().map(|&i| roads.pos[i]).collect();
        let length: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        self.route = simplify(&pts);
        n::gps_custom_route(ROUTE_COLOUR, &self.route, RADAR_WIDTH, MAP_WIDTH);
        crate::log(&format!("gps ({why}): route to ({:.0}, {:.0}), {length:.0} m over {} road nodes, drawn with {} points", wp.x, wp.y, path.len(), self.route.len()));
    }
}

/// The route's points, without ones on straight stretches closer than
/// MIN_GAP to the last kept one (at most MAX_POINTS).
fn simplify(pts: &[Vec3]) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = Vec::new();
    for (k, &p) in pts.iter().enumerate() {
        let last = k + 1 == pts.len();
        if let (Some(&prev), false) = (out.last(), last) {
            let next = pts[k + 1];
            let (ax, ay) = (p.x - prev.x, p.y - prev.y);
            let (bx, by) = (next.x - p.x, next.y - p.y);
            let turn = (ax * by - ay * bx).abs() / (ax.hypot(ay) * bx.hypot(by)).max(1e-3);
            if dist(prev, p) < MIN_GAP && turn < 0.2 {
                continue;
            }
        }
        out.push(p);
    }
    if out.len() > MAX_POINTS {
        let step = out.len() as f32 / MAX_POINTS as f32;
        let last = *out.last().unwrap();
        out = (0..MAX_POINTS - 1).map(|i| out[(i as f32 * step) as usize]).chain(std::iter::once(last)).collect();
    }
    out
}

/// How far (m, flat) `p` is from the route's line.
fn off_route(route: &[Vec3], p: Vec3) -> f32 {
    route
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let l2 = (dx * dx + dy * dy).max(1e-6);
            let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0);
            (a.x + dx * t - p.x).hypot(a.y + dy * t - p.y)
        })
        .fold(f32::INFINITY, f32::min)
}
