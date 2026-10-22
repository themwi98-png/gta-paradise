//! Burnout's drive-thru shops (junkyards, auto repairs, gas stations, paint
//! shops), for paradise.asi's map blips and shop actions.
//!
//! They're GenericRegions in TRIGGERS.DAT's TriggerData (32-bit layout from
//! burnout.wiki Trigger_Data): each is a box (position, rotation about each
//! axis, size) that the game acts on when the player drives into it.
//!
//! `bpx gta-shops <game-dir> <out-dir>` writes `paradise_shops.txt` (one shop
//! per line: kind, centre x y z, then the box's three axes as unit vectors
//! (along, across, up), then its half size along each; GTA world
//! coordinates), installed next to GTA5.exe. Most boxes stand upright, but
//! mods' boxes can be tilted (OTN has a 282 m gas station strip on a slope).
//!
//! With `--lots <csv>` (x,y,z,heading in GTA coordinates; marked in game
//! with F9), `lot x y z heading` lines are added too: the display lot next
//! to a junkyard where its car list's cars are shown (Burnout's lot
//! positions aren't in its data files).

use crate::gta::Placement;
use anyhow::{ensure, Context, Result};
use bnd2::{f32_at, u32_at, Bundle};
use std::fmt::Write as _;
use std::path::Path;

const TRIGGER_DATA: u32 = 0x10003;
const GENERIC_REGION_SIZE: usize = 0x38;

/// GenericRegion types the plugin acts on, and the names it reads.
const KINDS: [(u8, &str); 4] = [(0, "junkyard"), (2, "gas"), (3, "repair"), (4, "paint")];

struct Region {
    kind: u8,
    id: i32,
    pos: [f32; 3],
    rot: [f32; 3],
    size: [f32; 3],
    enabled: u8,
}

fn regions(game: &Path) -> Result<Vec<Region>> {
    let b = Bundle::open(game.join("TRIGGERS.DAT"))?;
    let e = b.entries.iter().find(|e| e.type_id == TRIGGER_DATA).context("TRIGGERS.DAT has no TriggerData")?;
    let d = b.chunk(e, 0)?;
    let version = u32_at(&d, 0);
    ensure!(version == 42, "TriggerData version {version}, expected 42");
    let at = u32_at(&d, 0x44) as usize;
    let count = u32_at(&d, 0x48) as usize;
    ensure!(at + count * GENERIC_REGION_SIZE <= d.len(), "generic regions run past the data");
    Ok((0..count)
        .map(|i| {
            let o = at + i * GENERIC_REGION_SIZE;
            let v = |k: usize| [f32_at(&d, o + k), f32_at(&d, o + k + 4), f32_at(&d, o + k + 8)];
            Region { kind: d[o + 0x36], id: u32_at(&d, o + 0x24) as i32, pos: v(0), rot: v(0xC), size: v(0x18), enabled: d[o + 0x2B] }
        })
        .collect())
}

/// Prints every generic region (type, id, box) for checking the layout.
pub fn dump(game: &Path) -> Result<()> {
    let mut counts = std::collections::BTreeMap::new();
    let identity = Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
    let floors = crate::props::Floors::load(Path::new(crate::edits::DEFAULT_COL), &identity).ok();
    for r in regions(game)? {
        if let (Some(f), true) = (&floors, KINDS.iter().any(|(t, _)| *t == r.kind)) {
            let mut h = f.heights(r.pos[0], -r.pos[2]);
            h.sort_by(f32::total_cmp);
            println!("  floors under centre: {:?} (box y {:.1}, half height {:.1})", h.iter().map(|z| (z * 10.0).round() / 10.0).collect::<Vec<_>>(), r.pos[1], r.size[1] * 0.5);
        }
        *counts.entry(r.kind).or_insert(0) += 1;
        println!("type {:2} id {:8} en {} pos {:9.1} {:7.1} {:9.1} rot {:6.2} {:6.2} {:6.2} size {:6.1} {:6.1} {:6.1}", r.kind, r.id, r.enabled, r.pos[0], r.pos[1], r.pos[2], r.rot[0], r.rot[1], r.rot[2], r.size[0], r.size[1], r.size[2]);
    }
    println!("{counts:?}");
    Ok(())
}

/// A box's rotation matrix (Burnout space; its columns are the box's own
/// axes). Burnout's rotation is Euler angles applied z, then y, then x
/// (R = Rx Ry Rz): the only order that keeps every stock shop's box upright.
fn rotation(rot: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx) = rot[0].sin_cos();
    let (sy, cy) = rot[1].sin_cos();
    let (sz, cz) = rot[2].sin_cos();
    [
        [cy * cz, -cy * sz, sy],
        [sx * sy * cz + cx * sz, -sx * sy * sz + cx * cz, -sx * cy],
        [-cx * sy * cz + sx * sz, cx * sy * sz + sx * cz, cx * cy],
    ]
}

pub fn export(game: &Path, out_dir: &Path, place: Placement, lots: Option<&Path>) -> Result<()> {
    let mut txt = String::from("# kind x y z  along(xyz) across(xyz) up(xyz)  half_along half_across half_up (GTA world; from Burnout's TRIGGERS.DAT)\n");
    let mut counts = [0; KINDS.len()];
    for r in regions(game)? {
        let Some(k) = KINDS.iter().position(|(t, _)| *t == r.kind) else { continue };
        counts[k] += 1;
        let c = place.apply(r.pos);
        let m = rotation(r.rot);
        let _ = write!(txt, "{} {:.2} {:.2} {:.2}", KINDS[k].1, c[0], c[1], c[2]);
        // Burnout's box x, z, y: along, across, up in GTA.
        for i in [0, 2, 1] {
            let a = place.apply_dir([m[0][i], m[1][i], m[2][i]]);
            let _ = write!(txt, "  {:.4} {:.4} {:.4}", a[0], a[1], a[2]);
        }
        let _ = writeln!(txt, "  {:.2} {:.2} {:.2}", r.size[0] * 0.5, r.size[2] * 0.5, r.size[1] * 0.5);
    }
    let mut lot_count = 0;
    if let Some(path) = lots {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        for line in text.lines().skip(1) {
            let v: Vec<f32> = line.split(',').take(4).filter_map(|f| f.trim().parse().ok()).collect();
            if let [x, y, z, heading] = v[..] {
                let _ = writeln!(txt, "lot {x:.2} {y:.2} {z:.2} {heading:.1}");
                lot_count += 1;
            }
        }
    }
    std::fs::create_dir_all(out_dir)?;
    std::fs::write(out_dir.join("paradise_shops.txt"), txt)?;
    let summary: Vec<String> = KINDS.iter().zip(counts).map(|((_, n), c)| format!("{c} {n}")).collect();
    println!("shops: {}, {lot_count} junkyard display lots", summary.join(", "));
    Ok(())
}
