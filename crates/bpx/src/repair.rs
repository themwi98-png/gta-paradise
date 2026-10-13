//! Collision mesh repair for GTA's physics.
//!
//! Burnout builds its world collision from many separate pieces (polygon
//! soups per road/terrain section) that meet with tiny gaps and staggered
//! vertices instead of shared edges. Burnout's car physics doesn't care, but
//! GTA's treats every unshared edge as a real edge, so wheels bump over the
//! seams. This welds near-coincident vertices and splits edges at T-junctions
//! so adjacent pieces share edges and GTA can link them as neighbours.

use std::collections::{HashMap, HashSet};

pub type V3 = [f32; 3];

#[derive(Debug, Clone, Copy)]
pub struct Tri {
    pub mat: u16,
    pub v: [u32; 3],
}

#[derive(Debug, Default)]
pub struct Stats {
    pub welded: usize,
    pub tjunction_splits: usize,
    pub degenerate_removed: usize,
    pub duplicates_removed: usize,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

/// Shared vertex pool with tolerance welding (grid hash, cell = tolerance).
pub struct Pool {
    pub verts: Vec<V3>,
    grid: HashMap<[i32; 3], Vec<u32>>,
    tol: f32,
    pub welded: usize,
}

impl Pool {
    pub fn new(tol: f32) -> Self {
        Pool { verts: Vec::new(), grid: HashMap::new(), tol, welded: 0 }
    }

    fn cell(&self, p: V3) -> [i32; 3] {
        p.map(|c| (c / self.tol).floor() as i32)
    }

    /// Returns an existing vertex within tolerance, or adds `p`.
    pub fn add(&mut self, p: V3) -> u32 {
        let c = self.cell(p);
        let mut best: Option<(f32, u32)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = self.grid.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &i in list {
                            let d = len(sub(self.verts[i as usize], p));
                            if d <= self.tol && best.is_none_or(|b| d < b.0) {
                                best = Some((d, i));
                            }
                        }
                    }
                }
            }
        }
        if let Some((d, i)) = best {
            if d > 0.0 {
                self.welded += 1;
            }
            return i;
        }
        let i = self.verts.len() as u32;
        self.verts.push(p);
        self.grid.entry(c).or_default().push(i);
        i
    }
}

fn edge_key(a: u32, b: u32) -> (u32, u32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn area2(verts: &[V3], t: &Tri) -> f32 {
    let [a, b, c] = t.v.map(|i| verts[i as usize]);
    len(cross(sub(b, a), sub(c, a)))
}

/// Splits triangle edges that have other vertices lying on them (within
/// `tol`), so neighbouring pieces end up sharing exact edges. Only edges used
/// by a single triangle are considered (shared edges are already fine).
pub fn fix_tjunctions(verts: &[V3], tris: &mut Vec<Tri>, tol: f32, stats: &mut Stats) {
    // Vertex grid for edge queries.
    let cell = 2.0f32;
    let mut grid: HashMap<[i32; 2], Vec<u32>> = HashMap::new();
    let used: HashSet<u32> = tris.iter().flat_map(|t| t.v).collect();
    for &i in &used {
        let p = verts[i as usize];
        grid.entry([(p[0] / cell).floor() as i32, (p[1] / cell).floor() as i32]).or_default().push(i);
    }

    for _pass in 0..3 {
        let mut count: HashMap<(u32, u32), u32> = HashMap::new();
        for t in tris.iter() {
            for k in 0..3 {
                *count.entry(edge_key(t.v[k], t.v[(k + 1) % 3])).or_default() += 1;
            }
        }
        let mut out = Vec::with_capacity(tris.len());
        let mut changed = false;
        for t in tris.iter() {
            // Find the first open edge with vertices on it; split into a fan.
            let mut split = None;
            for k in 0..3 {
                let (ia, ib) = (t.v[k], t.v[(k + 1) % 3]);
                if count[&edge_key(ia, ib)] != 1 {
                    continue;
                }
                let (a, b) = (verts[ia as usize], verts[ib as usize]);
                let d = sub(b, a);
                let l2 = dot(d, d);
                if l2 < 1e-8 {
                    continue;
                }
                let mut on: Vec<(f32, u32)> = Vec::new();
                let (x0, x1) = ((a[0].min(b[0]) - tol) / cell, (a[0].max(b[0]) + tol) / cell);
                let (y0, y1) = ((a[1].min(b[1]) - tol) / cell, (a[1].max(b[1]) + tol) / cell);
                for gx in x0.floor() as i32..=x1.floor() as i32 {
                    for gy in y0.floor() as i32..=y1.floor() as i32 {
                        let Some(list) = grid.get(&[gx, gy]) else { continue };
                        for &i in list {
                            if t.v.contains(&i) {
                                continue;
                            }
                            let p = verts[i as usize];
                            let s = dot(sub(p, a), d) / l2;
                            if s <= 0.001 || s >= 0.999 {
                                continue;
                            }
                            let proj = [a[0] + d[0] * s, a[1] + d[1] * s, a[2] + d[2] * s];
                            if len(sub(p, proj)) <= tol {
                                on.push((s, i));
                            }
                        }
                    }
                }
                if !on.is_empty() {
                    on.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
                    on.dedup_by_key(|x| x.1);
                    split = Some((k, on));
                    break;
                }
            }
            match split {
                Some((k, on)) => {
                    let (ia, ib, ic) = (t.v[k], t.v[(k + 1) % 3], t.v[(k + 2) % 3]);
                    let mut prev = ia;
                    for &(_, p) in &on {
                        out.push(Tri { mat: t.mat, v: [prev, p, ic] });
                        prev = p;
                    }
                    out.push(Tri { mat: t.mat, v: [prev, ib, ic] });
                    stats.tjunction_splits += on.len();
                    changed = true;
                }
                None => out.push(*t),
            }
        }
        *tris = out;
        if !changed {
            break;
        }
    }
}

/// Removes zero-area triangles and exact duplicates (same vertices, same
/// winding).
pub fn cleanup(verts: &[V3], tris: &mut Vec<Tri>, stats: &mut Stats) {
    let before = tris.len();
    tris.retain(|t| t.v[0] != t.v[1] && t.v[1] != t.v[2] && t.v[0] != t.v[2] && area2(verts, t) > 1e-6);
    stats.degenerate_removed += before - tris.len();
    let mut seen = HashSet::new();
    let before = tris.len();
    tris.retain(|t| {
        // Canonical rotation keeps winding (both windings of walls survive).
        let r = (0..3).min_by_key(|&k| t.v[k]).unwrap();
        seen.insert([t.v[r], t.v[(r + 1) % 3], t.v[(r + 2) % 3]])
    });
    stats.duplicates_removed += before - tris.len();
}

/// Orders triangles along a Morton (Z-order) curve of their centroids, so
/// splitting the list into chunks keeps neighbours together.
pub fn spatial_sort(verts: &[V3], tris: &mut [Tri]) {
    if tris.is_empty() {
        return;
    }
    let centre = |t: &Tri| {
        let [a, b, c] = t.v.map(|i| verts[i as usize]);
        [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0]
    };
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for t in tris.iter() {
        let c = centre(t);
        for k in 0..2 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let morton = |c: [f32; 2]| -> u64 {
        let q = |k: usize| (((c[k] - lo[k]) / (hi[k] - lo[k]).max(1e-3)) * 65535.0) as u64;
        let (x, y) = (q(0), q(1));
        let mut m = 0u64;
        for bit in 0..16 {
            m |= ((x >> bit) & 1) << (2 * bit) | ((y >> bit) & 1) << (2 * bit + 1);
        }
        m
    };
    tris.sort_by_cached_key(|t| morton(centre(t)));
}
