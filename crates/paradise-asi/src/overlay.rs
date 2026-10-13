//! In-game preview of the Paradise collision: draws nearby collision
//! triangles as translucent, flat-shaded polygons coloured by surface, from
//! `paradise_col.bin` (written by `bpx gta-collision`). Lets you see the
//! otherwise invisible map, and doubles as a collision debugging view.

use crate::natives as n;
use crate::shv::Vec3;

struct Tri {
    p: [[f32; 3]; 3],
    rgba: [i32; 4],
    wall: bool,
}

pub struct Overlay {
    tile: f32,
    tiles: std::collections::HashMap<(i32, i32), (usize, usize)>,
    tris: Vec<Tri>,
}

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// Base colour per GTA collision material index (materials.dat v12).
fn material_rgb(mat: u16) -> [f32; 3] {
    match mat {
        4 => [95.0, 95.0, 100.0],     // TARMAC (roads)
        174 => [185.0, 175.0, 150.0], // CONCRETE_PAVEMENT (sidewalks)
        1 => [150.0, 150.0, 150.0],   // CONCRETE
        47 => [70.0, 150.0, 60.0],    // GRASS
        50 => [40.0, 100.0, 40.0],    // BUSHES
        9 => [125.0, 100.0, 75.0],    // ROCK
        11 | 12 => [140.0, 125.0, 110.0], // STONE / COBBLESTONE
        18 => [215.0, 195.0, 125.0],  // SAND_LOOSE
        31 | 34 => [145.0, 135.0, 115.0], // GRAVEL
        56 | 62 => [235.0, 140.0, 40.0], // METAL / CHAINLINK (barriers)
        69 | 70 => [155.0, 105.0, 55.0], // WOOD
        86 => [60.0, 200.0, 210.0],   // PLASTIC
        112 => [150.0, 200.0, 255.0], // GLASS
        _ => [200.0, 60.0, 200.0],    // anything else: magenta
    }
}

impl Overlay {
    pub fn load(path: &str) -> Option<Overlay> {
        let b = std::fs::read(path).ok()?;
        if b.len() < 16 || &b[0..4] != b"PCOL" || u32_at(&b, 4) != 1 {
            return None;
        }
        let tile = f32_at(&b, 8);
        let ntiles = u32_at(&b, 12) as usize;
        let mut tiles = std::collections::HashMap::with_capacity(ntiles);
        let mut o = 16;
        for _ in 0..ntiles {
            let tx = u32_at(&b, o) as i32;
            let ty = u32_at(&b, o + 4) as i32;
            tiles.insert((tx, ty), (u32_at(&b, o + 8) as usize, u32_at(&b, o + 12) as usize));
            o += 16;
        }
        let count = (b.len() - o) / 40;
        let sun = [0.35f32, 0.25, 0.9];
        let sun_len = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt();
        let mut tris = Vec::with_capacity(count);
        for i in 0..count {
            let r = o + i * 40;
            let mut p = [[0f32; 3]; 3];
            for (k, v) in p.iter_mut().flatten().enumerate() {
                *v = f32_at(&b, r + k * 4);
            }
            let mat = u16::from_le_bytes([b[r + 36], b[r + 37]]);
            let wall = b[r + 38] == 1;
            // Flat shading so slopes and steps are visible.
            let u = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
            let w = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
            let nrm = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
            let nl = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt().max(1e-6);
            let lit = ((nrm[0] * sun[0] + nrm[1] * sun[1] + nrm[2] * sun[2]) / (nl * sun_len)).abs();
            let shade = 0.45 + 0.55 * lit;
            let base = material_rgb(mat);
            let rgba = [
                (base[0] * shade) as i32,
                (base[1] * shade) as i32,
                (base[2] * shade) as i32,
                if wall { 110 } else { 150 },
            ];
            tris.push(Tri { p, rgba, wall });
        }
        Some(Overlay { tile, tiles, tris })
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// Draws floors within `floor_r` and walls within `wall_r` of `at`, up to
    /// `budget` triangles (each drawn with both windings, as DRAW_POLY only
    /// renders one side).
    pub fn draw(&self, at: Vec3, floor_r: f32, wall_r: f32, budget: usize) -> usize {
        let r = floor_r.max(wall_r);
        let (tx0, tx1) = (((at.x - r) / self.tile).floor() as i32, ((at.x + r) / self.tile).floor() as i32);
        let (ty0, ty1) = (((at.y - r) / self.tile).floor() as i32, ((at.y + r) / self.tile).floor() as i32);
        // Nearest tiles first so the budget is spent close to the player.
        let mut keys: Vec<(i32, i32)> = (tx0..=tx1).flat_map(|x| (ty0..=ty1).map(move |y| (x, y))).collect();
        let centre = |k: &(i32, i32)| {
            let cx = (k.0 as f32 + 0.5) * self.tile - at.x;
            let cy = (k.1 as f32 + 0.5) * self.tile - at.y;
            cx * cx + cy * cy
        };
        keys.sort_by(|a, b| centre(a).partial_cmp(&centre(b)).unwrap());
        let mut drawn = 0;
        for k in keys {
            let Some(&(first, count)) = self.tiles.get(&k) else { continue };
            for t in &self.tris[first..first + count] {
                let c = [(t.p[0][0] + t.p[1][0] + t.p[2][0]) / 3.0, (t.p[0][1] + t.p[1][1] + t.p[2][1]) / 3.0];
                let (dx, dy) = (c[0] - at.x, c[1] - at.y);
                let lim = if t.wall { wall_r } else { floor_r };
                if dx * dx + dy * dy > lim * lim {
                    continue;
                }
                let [a, b, c3] = t.p;
                n::draw_poly(a, b, c3, t.rgba);
                n::draw_poly(a, c3, b, t.rgba);
                drawn += 1;
                if drawn >= budget {
                    return drawn;
                }
            }
        }
        drawn
    }
}
