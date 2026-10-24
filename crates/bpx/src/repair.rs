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

/// Removes zero-area triangles and duplicates (same vertices, either
/// winding: a wall Burnout has twice, back to back, is kept once).
pub fn cleanup(verts: &[V3], tris: &mut Vec<Tri>, stats: &mut Stats) {
    let before = tris.len();
    tris.retain(|t| t.v[0] != t.v[1] && t.v[1] != t.v[2] && t.v[0] != t.v[2] && area2(verts, t) > 1e-6);
    stats.degenerate_removed += before - tris.len();
    let mut seen = HashSet::new();
    let before = tris.len();
    tris.retain(|t| {
        let mut key = t.v;
        key.sort();
        seen.insert(key)
    });
    stats.duplicates_removed += before - tris.len();
}

#[derive(Debug, Default)]
pub struct WallStats {
    /// Wall triangles of closed surfaces, kept one-sided.
    pub one_sided: usize,
    /// Wall triangles made into slabs, thickened behind (floors give the
    /// front) or both ways (no front known).
    pub slab_behind: usize,
    pub slab_centred: usize,
    /// Wall surfaces whose floors disagree on the front, or that can't be
    /// wound consistently.
    pub conflicts: usize,
    pub caps: usize,
}

fn unit_normal(verts: &[V3], v: [u32; 3]) -> V3 {
    let [a, b, c] = v.map(|i| verts[i as usize]);
    let n = cross(sub(b, a), sub(c, a));
    let l = len(n);
    if l > 0.0 {
        n.map(|x| x / l)
    } else {
        [0.0; 3]
    }
}

/// Horizontal part of a wall's normal, unit length.
fn flat_normal(verts: &[V3], v: [u32; 3]) -> V3 {
    let n = unit_normal(verts, v);
    let l = (n[0] * n[0] + n[1] * n[1]).sqrt();
    if l > 0.0 {
        [n[0] / l, n[1] / l, 0.0]
    } else {
        [0.0; 3]
    }
}

/// True for the edge from v[k] to v[k + 1] when it is a triangle's bottom: the
/// triangle lies above it.
fn is_bottom(verts: &[V3], v: [u32; 3], k: usize) -> bool {
    let (a, b, c) = (verts[v[k] as usize], verts[v[(k + 1) % 3] as usize], verts[v[(k + 2) % 3] as usize]);
    let e = sub(b, a);
    let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5];
    let out = sub(mid, c);
    let l2 = dot(e, e).max(1e-12);
    let s = dot(out, e) / l2;
    let perp = [out[0] - e[0] * s, out[1] - e[1] * s, out[2] - e[2] * s];
    let l = len(perp);
    l > 0.0 && perp[2] / l < -0.7
}

fn runs(v: [u32; 3], a: u32, b: u32) -> bool {
    (0..3).any(|k| v[k] == a && v[(k + 1) % 3] == b)
}

/// Burnout's walls are two-sided: their winding says nothing about which side
/// is solid. GTA's polygons are one-sided, and two copies back to back get
/// linked to each other as neighbours instead of to the wall around them, so
/// peds and ragdolls pushed into them jitter, and a wall's top never reads as
/// a ledge to climb. Instead, each wall surface (walls joined by edges that
/// only two triangles share) is wound consistently and given its front by the
/// floors it shares edges with. A closed surface whose floors all agree (a
/// kerb, a box with a top) is kept one-sided, facing out. Any other becomes a
/// solid slab `thickness` thick, with caps on its open edges but the bottom:
/// thickened behind the front where the floors give one, else both ways.
/// `is_wall` picks the wall triangles; floors keep their winding.
pub fn solidify_walls(verts: &mut Vec<V3>, tris: &mut Vec<Tri>, is_wall: impl Fn(&[V3], &Tri) -> bool, thickness: f32, stats: &mut WallStats) {
    let n = tris.len();
    let wall: Vec<bool> = tris.iter().map(|t| is_wall(verts, t)).collect();
    let mut edges: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            edges.entry(edge_key(t.v[k], t.v[(k + 1) % 3])).or_default().push(i as u32);
        }
    }
    let on_floor = |a: u32, b: u32| edges[&edge_key(a, b)].iter().any(|&j| !wall[j as usize]);

    // Flood each surface, winding every triangle to match its first one
    // (flip), and tally the floors' votes for flipping the whole surface.
    let mut comp = vec![u32::MAX; n];
    let mut flip = vec![false; n];
    let mut out: Vec<Tri> = Vec::with_capacity(n);
    for seed in 0..n {
        if !wall[seed] || comp[seed] != u32::MAX {
            continue;
        }
        comp[seed] = seed as u32;
        let (mut members, mut stack) = (Vec::new(), vec![seed]);
        let (mut vote_flip, mut vote_keep, mut open, mut conflict) = (0.0f32, 0.0f32, false, false);
        while let Some(i) = stack.pop() {
            members.push(i);
            let t = tris[i].v;
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let list = &edges[&edge_key(a, b)];
                if list.len() == 1 {
                    open |= !is_bottom(verts, t, k);
                    continue;
                }
                if list.len() != 2 {
                    continue;
                }
                let j = if list[0] as usize == i { list[1] } else { list[0] } as usize;
                // A consistent surface runs a shared edge opposite ways.
                let want = flip[i] ^ runs(tris[j].v, a, b);
                if !wall[j] {
                    let l = len(sub(verts[a as usize], verts[b as usize]));
                    if want {
                        vote_flip += l;
                    } else {
                        vote_keep += l;
                    }
                } else if comp[j] == u32::MAX {
                    comp[j] = seed as u32;
                    flip[j] = want;
                    stack.push(j);
                } else if flip[j] != want {
                    conflict = true;
                }
            }
        }
        let faced = !conflict && ((vote_flip > 0.0) != (vote_keep > 0.0));
        stats.conflicts += (conflict || (vote_flip > 0.0 && vote_keep > 0.0)) as usize;
        let turn = faced && vote_flip > 0.0;
        let wound = |i: usize| {
            let t = tris[i];
            if flip[i] ^ turn {
                Tri { mat: t.mat, v: [t.v[0], t.v[2], t.v[1]] }
            } else {
                t
            }
        };
        if faced && !open {
            out.extend(members.iter().map(|&i| wound(i)));
            stats.one_sided += members.len();
        } else if conflict {
            // No consistent winding: each triangle is a slab of its own.
            for &i in &members {
                slab(verts, &[wound(i)], false, thickness, &on_floor, &mut out, stats);
            }
            stats.slab_centred += members.len();
        } else {
            let group: Vec<Tri> = members.iter().map(|&i| wound(i)).collect();
            slab(verts, &group, faced, thickness, &on_floor, &mut out, stats);
            if faced {
                stats.slab_behind += members.len();
            } else {
                stats.slab_centred += members.len();
            }
        }
    }
    out.extend(tris.iter().zip(&wall).filter(|(_, &w)| !w).map(|(t, _)| *t));
    *tris = out;
}

/// Makes a consistently wound wall surface a closed slab: its front (in place
/// when `behind`, else moved forward by half the thickness), a back facing the
/// other way, and caps on edges no other triangle of the surface shares,
/// except bottoms and edges a floor already closes.
fn slab(verts: &mut Vec<V3>, group: &[Tri], behind: bool, thickness: f32, on_floor: &impl Fn(u32, u32) -> bool, out: &mut Vec<Tri>, stats: &mut WallStats) {
    let normals: Vec<V3> = group.iter().map(|t| flat_normal(verts, t.v)).collect();
    // Per vertex: the faces' mean direction, lengthened at corners so every
    // face's slab keeps its full thickness (at most doubled).
    let mut dirs: HashMap<u32, (V3, Vec<V3>)> = HashMap::new();
    let mut order = Vec::new();
    for (t, n) in group.iter().zip(&normals) {
        for &v in &t.v {
            let e = dirs.entry(v).or_insert_with(|| {
                order.push(v);
                ([0.0; 3], Vec::new())
            });
            e.0 = [e.0[0] + n[0], e.0[1] + n[1], 0.0];
            e.1.push(*n);
        }
    }
    let (front_at, back_at) = if behind { (0.0, -thickness) } else { (thickness * 0.5, -thickness * 0.5) };
    let mut ids: HashMap<u32, (u32, u32)> = HashMap::new();
    for v in order {
        let (sum, faces) = &dirs[&v];
        let l = len(*sum);
        let d = if l > 0.3 { sum.map(|x| x / l) } else { faces[0] };
        let cos = faces.iter().map(|f| dot(*f, d)).fold(1.0f32, f32::min).max(0.5);
        let p = verts[v as usize];
        let mut at = |s: f32| {
            if s == 0.0 {
                return v;
            }
            let s = s / cos;
            verts.push([p[0] + d[0] * s, p[1] + d[1] * s, p[2]]);
            (verts.len() - 1) as u32
        };
        let f = at(front_at);
        let b = at(back_at);
        ids.insert(v, (f, b));
    }
    let mut count: HashMap<(u32, u32), u32> = HashMap::new();
    for t in group {
        for k in 0..3 {
            *count.entry(edge_key(t.v[k], t.v[(k + 1) % 3])).or_default() += 1;
        }
    }
    for t in group {
        let f = t.v.map(|v| ids[&v].0);
        let b = t.v.map(|v| ids[&v].1);
        out.push(Tri { mat: t.mat, v: f });
        out.push(Tri { mat: t.mat, v: [b[0], b[2], b[1]] });
        for k in 0..3 {
            let (a, c) = (t.v[k], t.v[(k + 1) % 3]);
            if count[&edge_key(a, c)] != 1 || on_floor(a, c) || is_bottom(verts, t.v, k) {
                continue;
            }
            // Wound to run the edge opposite to the front and the back, so
            // the slab is closed and faces out.
            let (fa, fc, ba, bc) = (ids[&a].0, ids[&c].0, ids[&a].1, ids[&c].1);
            out.push(Tri { mat: t.mat, v: [fc, fa, ba] });
            out.push(Tri { mat: t.mat, v: [fc, ba, bc] });
            stats.caps += 2;
        }
    }
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
