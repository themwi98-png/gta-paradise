//! Edits to Rockstar's own data files, made from the player's copies:
//!
//! - `bpx handling`: Burnout-style handling (crash damage, speed, grip).
//! - `bpx strip-base-map`: Los Santos taken out of the level's file lists
//!   and out of Rockstar's patch packs.
//! - `bpx popzone`: GTA's population zones from Burnout's district map.
//! - `bpx ground-materials`: sand, dirt, grass... no longer slow cars down.
//!
//! Rockstar's files are kept byte for byte apart from the edits.

use anyhow::{bail, Context, Result};
use bnd2::Bundle;
use bpworld::Res;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

// --- Handling -------------------------------------------------------------

/// What happens to one handling field.
#[derive(Clone, Copy)]
enum Change {
    /// Rockstar's value times this, but not above the limit (unless
    /// Rockstar's own value is already higher).
    Scale(f64, f64),
    /// The same value for every vehicle.
    Set(f64),
    /// Scaled by the grip multiplier (`--grip`), with this limit.
    Grip(f64),
    /// Scaled by the brake multiplier (`--brakes`), with this limit.
    Brakes(f64),
}

/// Grip multiplier when none is given (the player's `@car_grip`).
pub const GRIP_DEFAULT: f64 = 1.2;
/// Brake multiplier when none is given (the player's `@car_brakes`).
pub const BRAKES_DEFAULT: f64 = 2.5;

/// HF_TYRES_CAN_CLIP in a vehicle's strHandlingFlags (hex): the tyres may
/// sink into the ground and kerbs instead of the car being lifted over every
/// bump. Rockstar sets it on a few low cars.
const HF_TYRES_CAN_CLIP: u32 = 0x2_0000;

/// Sets HF_TYRES_CAN_CLIP in every `<strHandlingFlags>`; returns the new
/// text and how many vehicles gained it.
fn set_tyres_clip(text: &str) -> (String, usize) {
    const OPEN: &str = "<strHandlingFlags>";
    let mut out = String::with_capacity(text.len());
    let mut n = 0;
    let mut rest = text;
    while let Some(i) = rest.find(OPEN) {
        let (head, tail) = rest.split_at(i + OPEN.len());
        out.push_str(head);
        let end = tail.find('<').unwrap_or(tail.len());
        match u32::from_str_radix(tail[..end].trim(), 16) {
            Ok(flags) if flags & HF_TYRES_CAN_CLIP == 0 => {
                out.push_str(&format!("{:X}", flags | HF_TYRES_CAN_CLIP));
                n += 1;
            }
            _ => out.push_str(&tail[..end]),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    (out, n)
}

/// How much harder crashes hit and how much faster cars go (Rockstar's
/// values average about 0.8 for deformation, 1.0 for collision and 1.5 for
/// engine damage).
const HANDLING: [(&str, Change); 9] = [
    // Size and depth of dents: one deep value for every car (as in the
    // "Better Deformation" handling mod). Scaling Rockstar's values instead
    // left most cars shallower and trucks (4.0 x 6) torn apart.
    ("fDeformationDamageMult", Change::Set(6.2)),
    // Body health lost per crash.
    ("fCollisionDamageMult", Change::Scale(5.0, 10.0)),
    // Engine health lost per crash: half Rockstar's, so a smashed-up car
    // keeps driving, Burnout style; fire and a dead engine take a lot of
    // punishment (hard crashes still wreck it outright).
    ("fEngineDamageMult", Change::Scale(0.5, 5.0)),
    // Top speed (the gearing's flat-out speed).
    ("fInitialDriveMaxFlatVel", Change::Scale(1.35, 500.0)),
    // Engine power: acceleration, and reaching that top speed.
    ("fInitialDriveForce", Change::Scale(1.35, 3.0)),
    // Stronger brakes to match. The multiplier is `--brakes` (BRAKES_DEFAULT).
    ("fBrakeForce", Change::Brakes(3.0)),
    // Grip: peak cornering grip, and grip while sliding (so slides are
    // catchable at speed). The multiplier is `--grip` (GRIP_DEFAULT).
    ("fTractionCurveMax", Change::Grip(5.0)),
    ("fTractionCurveMin", Change::Grip(5.0)),
    // Less wheelspin pulling away (the extra engine power spins the tyres).
    ("fLowSpeedTractionLossMult", Change::Scale(0.6, 5.0)),
];

/// Scales the handling fields in one handling.meta; returns the new text and
/// how many values changed. A field is `<name` + whitespace + `value="` +
/// number + `"`.
fn scale_handling(text: &str, grip: f64, brakes: f64) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut n = 0;
    let mut rest = text;
    'outer: while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let tail = &rest[lt + 1..];
        for (name, change) in HANDLING {
            let Some(after) = tail.strip_prefix(name) else { continue };
            let ws = after.len() - after.trim_start().len();
            if ws == 0 {
                continue;
            }
            let Some(num_start) = after[ws..].strip_prefix("value=\"") else { continue };
            let num_len = num_start.find(|c: char| !(c.is_ascii_digit() || "-.eE+".contains(c))).unwrap_or(num_start.len());
            if num_len == 0 || !num_start[num_len..].starts_with('"') {
                continue;
            }
            let Ok(v) = num_start[..num_len].parse::<f64>() else { continue };
            let head_len = 1 + name.len() + ws + "value=\"".len();
            out.push_str(&rest[lt..lt + head_len]);
            let new = match change {
                Change::Scale(scale, cap) => (v * scale).min(v.max(cap)),
                Change::Set(value) => value,
                Change::Grip(cap) => (v * grip).min(v.max(cap)),
                Change::Brakes(cap) => (v * brakes).min(v.max(cap)),
            };
            out.push_str(&format!("{new:.6}"));
            n += 1;
            rest = &num_start[num_len..];
            continue 'outer;
        }
        out.push('<');
        rest = tail;
    }
    out.push_str(rest);
    (out, n)
}

/// `bpx handling <handling_src-dir> <out-dir> [--grip x] [--brakes x] [--tyres-clip on|off]`:
/// Rockstar's handling files with the fields in HANDLING scaled (and with
/// `--tyres-clip on`, HF_TYRES_CAN_CLIP for every vehicle), at the paths
/// update.rpf uses:
///     common/data/handling.meta                     base-game vehicles
///     dlc_patch/<pack>/common/data/handling.meta    each DLC pack's vehicles
/// (update.rpf's dlc_patch folder overrides a pack's files; where Rockstar
/// already ships a patch, that patch is the one scaled.) The source folder:
///     _update/update/update.rpf/common/data/handling.meta
///     _update/update/update.rpf/dlc_patch/<pack>/common/data/handling.meta
///     <pack>/handling.meta
pub fn handling(src: &Path, out: &Path, grip: f64, brakes: f64, tyres_clip: bool) -> Result<()> {
    let upd = src.join("_update/update/update.rpf");
    let mut files = vec![(upd.join("common/data/handling.meta"), out.join("common/data/handling.meta"))];
    let mut packs: Vec<PathBuf> = std::fs::read_dir(src)?.filter_map(|e| Some(e.ok()?.path())).collect();
    packs.sort();
    for d in packs {
        let name = d.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if name.starts_with('_') || !d.join("handling.meta").is_file() {
            continue;
        }
        let rel = Path::new("dlc_patch").join(name.to_lowercase()).join("common/data/handling.meta");
        let patch = upd.join(&rel);
        files.push((if patch.is_file() { patch } else { d.join("handling.meta") }, out.join(rel)));
    }
    let (mut total, mut clip) = (0, 0);
    for (s, t) in &files {
        let text = std::fs::read_to_string(s).with_context(|| format!("reading {}", s.display()))?;
        let (mut text, n) = scale_handling(&text, grip, brakes);
        if tyres_clip {
            let (t, c) = set_tyres_clip(&text);
            text = t;
            clip += c;
        }
        std::fs::create_dir_all(t.parent().unwrap())?;
        std::fs::write(t, text)?;
        total += n;
    }
    println!("handling: {} files, {total} values scaled, tyres can clip on {clip} more vehicles -> {}", files.len(), out.display());
    Ok(())
}

// --- Los Santos out of the file lists -------------------------------------

fn read_text(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text))
}

/// The value of the first `<filename>` in `s`, trimmed.
fn filename(s: &str) -> Option<&str> {
    let a = s.find("<filename>")? + "<filename>".len();
    let rest = &s[a..];
    let end = rest.find('<')?;
    if !rest[end..].starts_with("</filename>") {
        return None;
    }
    let v = rest[..end].trim();
    (!v.is_empty()).then_some(v)
}

/// Drops `<Item>...</Item>` blocks (with their indent and line end) for
/// which `drop(block)` says so. `innermost`: only blocks with no `<Item>`
/// inside; otherwise each block runs to the first `</Item>`.
fn strip_items(text: &str, innermost: bool, mut drop: impl FnMut(&str) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    let mut search = 0;
    // End of the last block found: indentation is only taken back to it.
    let mut last_end = 0;
    while let Some(i) = text[search..].find("<Item>").map(|i| search + i) {
        let body_start = i + "<Item>".len();
        let Some(j) = text[body_start..].find("</Item>").map(|j| body_start + j) else { break };
        if innermost && text[body_start..j].contains("<Item>") {
            search = body_start;
            continue;
        }
        let mut start = i;
        while start > last_end && matches!(text.as_bytes()[start - 1], b' ' | b'\t') {
            start -= 1;
        }
        let mut end = j + "</Item>".len();
        while end < text.len() && matches!(text.as_bytes()[end], b' ' | b'\t') {
            end += 1;
        }
        if text[end..].starts_with('\r') {
            end += 1;
        }
        if text[end..].starts_with('\n') {
            end += 1;
        }
        if drop(&text[body_start..j]) {
            out.push_str(&text[pos..start]);
            pos = end;
        }
        search = end;
        last_end = end;
    }
    out.push_str(&text[pos..]);
    out
}

/// Is `path` (forward slashes) inside a Los Santos map folder, or a map
/// additions archive?
fn ls_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = p[from..].find("levels/gta5/").map(|i| from + i) {
        let r = &p[i + "levels/gta5/".len()..];
        if ["_citye/", "_cityw/", "_hills/"].iter().any(|d| r.starts_with(d)) {
            return true;
        }
        let seg = &r[..r.find('/').unwrap_or(r.len())];
        if let Some(a) = seg.find("_additions") {
            if seg[a + "_additions".len()..].contains(".rpf") {
                return true;
            }
        }
        from = i + 1;
    }
    false
}

/// `bpx strip-base-map pack <content.xml> <out>`: Rockstar's patch packs
/// (patchday*ng, patch2026_01...) re-add updated copies of Los Santos
/// sectors and map additions; every innermost `<Item>` naming one goes (the
/// data file entries and their filesToEnable / filesToInvalidate entries).
pub fn strip_pack(content: &Path, out: &Path) -> Result<()> {
    let text = read_text(content)?;
    let mut removed = 0;
    let text = strip_items(&text, true, |body| {
        let path = filename(body).unwrap_or(body).trim().replace('\\', "/");
        let hit = !path.contains('<') && ls_path(&path);
        removed += hit as usize;
        hit
    });
    std::fs::write(out, text)?;
    println!("{}: removed {removed} Los Santos item(s)", content.file_name().unwrap_or_default().to_string_lossy());
    Ok(())
}

/// `bpx strip-base-map <list.txt> <images.meta> <gta5.meta> <out-dir>`:
/// removes Los Santos / Blaine County map archives from the level's data
/// file lists, so the game never mounts them. images.meta declares every
/// map archive before any DLC loads, which is why DLC filesToDisable could
/// not remove them: items whose `<filename>` is in the list go. gta5.meta
/// loses the permanent .ityp files inside dropped archives, and the
/// scenario manifests (scenario points are all placed for Los Santos).
/// Inputs should be the pristine game files, so running again is harmless.
pub fn strip_base_map(list: &Path, images: &Path, gta5: &Path, out: &Path) -> Result<()> {
    let names: HashSet<String> = read_text(list)?
        .lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let dirs: HashSet<String> = names.iter().map(|p| format!("{}/", p.rsplit_once('/').map_or(p.as_str(), |(d, _)| d))).collect();
    std::fs::create_dir_all(out)?;

    let mut removed = Vec::new();
    let text = strip_items(&read_text(images)?, false, |body| {
        let hit = filename(body).is_some_and(|f| names.contains(&f.to_lowercase()));
        if hit {
            removed.push(filename(body).unwrap().to_string());
        }
        hit
    });
    std::fs::write(out.join("images.meta"), text)?;
    println!("images.meta: removed {} archive(s)", removed.len());

    // Permanent ityps directly in (or under) a dropped sector folder, e.g.
    // _citye/citye_xr.ityp or _citye/downtown_01/..._permanent.ityp.
    const AREA: [&str; 3] = ["platform:/levels/gta5/_citye/", "platform:/levels/gta5/_cityw/", "platform:/levels/gta5/_hills/"];
    let mut removed = Vec::new();
    let text = strip_items(&read_text(gta5)?, false, |body| {
        let Some(f) = filename(body) else { return false };
        let fl = f.to_lowercase();
        let parent = format!("{}/", fl.rsplit_once('/').map_or(fl.as_str(), |(d, _)| d));
        let hit = fl.ends_with(".ityp") && AREA.iter().any(|a| fl.starts_with(a)) && dirs.iter().any(|d| fl.starts_with(d.as_str()) || d.starts_with(&parent));
        if hit {
            removed.push(f.to_string());
        }
        hit
    });
    let text = strip_items(&text, false, |body| {
        let Some(f) = filename(body) else { return false };
        let hit = f.to_lowercase().ends_with("/sp_manifest.#mt");
        if hit {
            removed.push(f.to_string());
        }
        hit
    });
    std::fs::write(out.join("gta5.meta"), text)?;
    println!("gta5.meta: removed {} item(s): {}", removed.len(), removed.join(", "));
    Ok(())
}

// --- Population zones -----------------------------------------------------

/// Where the district grid lies in GTA's world (lined up by hand in
/// the viewer's district_aligner.html (paradise-viewer); the mainland and the island are
/// painted at different scales): cell size x / y, west and north edge.
struct Align {
    cx: f64,
    cy: f64,
    x0: f64,
    y0: f64,
}
const MAINLAND: Align = Align { cx: 32.267, cy: 23.767, x0: -4199.0, y0: 3837.0 };
const ISLAND: Align = Align { cx: 18.117, cy: 14.517, x0: -1031.0, y0: 1711.0 };
const ISLAND_IDS: std::ops::Range<u8> = 18..23;

/// Burnout district -> Los Santos zone label (all LSPD-policed, picked for
/// character).
const LABEL: [&str; 23] = [
    "DelPe", "Mirr", "Koreat", "Beach", "PBluff", "CHIL", "CHIL", "CHIL", "Elysian", "Rockf", "Elysian", "CHIL", "CHIL", "Richm", "Downt", "LMesa",
    "Murri", "Vesp", "DelPe", "DeLBe", "Alta", "Beach", "PBluff",
];

/// Burnout's district map (DISTRICTS.DAT): one district id per cell, rows
/// north to south (255: none).
pub fn districts(game: &Path) -> Result<(usize, usize, Vec<u8>)> {
    let b = Bundle::open(game.join("DISTRICTS.DAT"))?;
    let e = b.entries.first().context("DISTRICTS.DAT is empty")?;
    let r = Res::load(&b, e)?;
    let d = &r.main;
    if d.len() < 0x14 {
        bail!("DISTRICTS.DAT: unexpected layout");
    }
    let (w, h) = (u16::from_le_bytes([d[0x10], d[0x11]]) as usize, u16::from_le_bytes([d[0x12], d[0x13]]) as usize);
    let cells = d.get(0x14..0x14 + w * h).context("DISTRICTS.DAT: grid cut short")?;
    Ok((w, h, cells.to_vec()))
}

/// Greedy rectangles over a grid mask: (row0, col0, row1, col1).
fn boxes(mask: &[bool], w: usize, h: usize) -> Vec<(usize, usize, usize, usize)> {
    let mut used = vec![false; w * h];
    let mut out = Vec::new();
    for r in 0..h {
        for c in 0..w {
            if !mask[r * w + c] || used[r * w + c] {
                continue;
            }
            let mut c1 = c;
            while c1 + 1 < w && mask[r * w + c1 + 1] && !used[r * w + c1 + 1] {
                c1 += 1;
            }
            let mut r1 = r;
            while r1 + 1 < h && (c..=c1).all(|k| mask[(r1 + 1) * w + k] && !used[(r1 + 1) * w + k]) {
                r1 += 1;
            }
            for rr in r..=r1 {
                for k in c..=c1 {
                    used[rr * w + k] = true;
                }
            }
            out.push((r, c, r1, c1));
        }
    }
    out
}

/// `bpx popzone <burnout-dir> <original popzone.ipl> <out popzone.ipl>`:
/// each Burnout district becomes a Los Santos city zone (as boxes merged
/// from the district grid); everything else falls into one large ocean box
/// listed last.
pub fn popzone(game: &Path, orig: &Path, out: &Path) -> Result<()> {
    let (w, h, grid) = districts(game)?;
    let orig = String::from_utf8_lossy(&std::fs::read(orig)?).into_owned();
    let mut zone_of: BTreeMap<String, String> = BTreeMap::new();
    for line in orig.lines() {
        let p: Vec<&str> = line.split(',').map(str::trim).collect();
        if p.len() >= 9 {
            zone_of.entry(p[7].to_string()).or_insert_with(|| p[0].to_string());
        }
    }
    let zone = |label: &str| zone_of.get(label).cloned().with_context(|| format!("popzone.ipl has no {label} zone"));
    let mut lines = vec!["zone".to_string()];
    for (d, label) in LABEL.iter().enumerate() {
        let a = if ISLAND_IDS.contains(&(d as u8)) { &ISLAND } else { &MAINLAND };
        let mask: Vec<bool> = grid.iter().map(|&g| g as usize == d).collect();
        for (r0, c0, r1, c1) in boxes(&mask, w, h) {
            let x0 = a.x0 + c0 as f64 * a.cx;
            let x1 = a.x0 + (c1 + 1) as f64 * a.cx;
            let y1 = a.y0 - r0 as f64 * a.cy;
            let y0 = a.y0 - (r1 + 1) as f64 * a.cy;
            lines.push(format!("{}, {x0:.2}, {y0:.2}, -200.0, {x1:.2}, {y1:.2}, 1500.0, {label}, 0", zone(label)?));
        }
    }
    let n = lines.len() - 1;
    lines.push(format!("{}, -16000, -16000, -200.0, 16000, 16000, 1500.0, OCEANA, 0", zone("OCEANA")?));
    lines.push("end".to_string());
    std::fs::write(out, lines.join("\r\n") + "\r\n")?;
    println!("popzone: {n} district boxes + ocean fallback");
    Ok(())
}

// --- Ground materials -----------------------------------------------------

/// Columns of a materials.dat row (whitespace separated): name, filter, fx
/// group, vfx disturbance, rumble profile, react weapon, friction,
/// elasticity, density, tyre grip, wet grip, tyre drag, top speed mult...
const TYRE_DRAG: usize = 11;
const TOP_SPEED_MULT: usize = 12;

/// `bpx ground-materials <original materials.dat> <out>`: Burnout's off-road is as
/// fast as the road, so no ground drags at the tyres (tyre drag 0) or caps
/// the top speed (top speed multiplier at least 1). Grip is left as it is.
pub fn ground_materials(orig: &Path, out: &Path) -> Result<()> {
    let text = String::from_utf8_lossy(&std::fs::read(orig)?).into_owned();
    let mut changed = Vec::new();
    let mut lines = Vec::new();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            lines.push(line.to_string());
            continue;
        }
        // Fields with their positions, so the spacing is kept.
        let mut fields: Vec<(usize, usize)> = Vec::new();
        let mut start = None;
        for (i, ch) in line.char_indices() {
            match (ch.is_whitespace(), start) {
                (false, None) => start = Some(i),
                (true, Some(s)) => {
                    fields.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            fields.push((s, line.len()));
        }
        let num = |k: usize| fields.get(k).and_then(|&(a, b)| line[a..b].parse::<f32>().ok());
        let (Some(drag), Some(top)) = (num(TYRE_DRAG), num(TOP_SPEED_MULT)) else {
            lines.push(line.to_string());
            continue;
        };
        if drag <= 0.0 && top >= 1.0 {
            lines.push(line.to_string());
            continue;
        }
        let name = &line[fields[0].0..fields[0].1];
        changed.push(format!("{name} (drag {drag}, top speed x{top})"));
        // Later field first, so the earlier one's position still holds.
        let mut l = line.to_string();
        if top < 1.0 {
            let (a, b) = fields[TOP_SPEED_MULT];
            l.replace_range(a..b, "1.0");
        }
        if drag > 0.0 {
            let (a, b) = fields[TYRE_DRAG];
            l.replace_range(a..b, "0.0");
        }
        lines.push(l);
    }
    anyhow::ensure!(!changed.is_empty(), "{}: no material rows found", orig.display());
    std::fs::write(out, lines.concat())?;
    println!("materials: {} ground types no longer slow cars: {}", changed.len(), changed.join(", "));
    Ok(())
}

/// Vehicle types offered in the junkyard: what drives on roads.
const ROAD_TYPES: [&str; 6] = ["VEHICLE_TYPE_CAR", "VEHICLE_TYPE_BIKE", "VEHICLE_TYPE_QUADBIKE", "VEHICLE_TYPE_AMPHIBIOUS_AUTOMOBILE", "VEHICLE_TYPE_AMPHIBIOUS_QUADBIKE", "VEHICLE_TYPE_SUBMARINECAR"];

/// `bpx gta-cars <src-dir> <out-file>`: every road vehicle's model name from
/// the vehicles.meta files anywhere under `src-dir` (the base game's and each
/// DLC pack's, extracted by the setup), one per line, for the junkyard's car
/// list in paradise.asi. Vehicles named in gen9_exclusive_assets_vehicles.meta
/// are left out: they're listed in Legacy's files but only Enhanced can
/// create them (the Coil Cyclone II and four others).
pub fn cars(src: &Path, out: &Path) -> Result<()> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            if p.is_dir() {
                walk(&p, files)?;
            } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("vehicles.meta") || n.eq_ignore_ascii_case("gen9_exclusive_assets_vehicles.meta")) {
                files.push(p);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(src, &mut files)?;
    let tag = |item: &str, name: &str| -> Option<String> {
        let open = format!("<{name}>");
        let start = item.find(&open)? + open.len();
        Some(item[start..start + item[start..].find('<')?].trim().to_string())
    };
    let mut models = std::collections::BTreeSet::new();
    let mut skipped = 0;
    let mut enhanced_only = HashSet::new();
    for f in &files {
        let text = std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
        if !f.file_name().is_some_and(|n| n.eq_ignore_ascii_case("vehicles.meta")) {
            if let Some(list) = text.find("<VehicleModels>").and_then(|i| Some(&text[i..i + text[i..].find("</VehicleModels>")?])) {
                enhanced_only.extend(list.split("<Item>").skip(1).filter_map(|it| Some(it[..it.find('<')?].trim().to_ascii_lowercase())));
            }
            continue;
        }
        let Some(list) = text.find("<InitDatas>").map(|i| &text[i..]) else { continue };
        // Vehicles hold nested <Item>s, so each one is taken as the text from
        // its <modelName> to the next vehicle's.
        for item in list.split("<modelName>").skip(1) {
            let item = format!("<modelName>{item}");
            let (Some(model), Some(kind)) = (tag(&item, "modelName"), tag(&item, "type")) else { continue };
            if ROAD_TYPES.contains(&kind.as_str()) {
                models.insert(model.to_ascii_lowercase());
            } else {
                skipped += 1;
            }
        }
    }
    let before = models.len();
    models.retain(|m| !enhanced_only.contains(m));
    let dropped = before - models.len();
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut txt = String::from("# road vehicle model names from GTA's vehicles.meta files (bpx gta-cars)\n");
    for m in &models {
        txt.push_str(m);
        txt.push('\n');
    }
    std::fs::write(out, txt)?;
    println!("cars: {} road vehicles from {} vehicles.meta files ({skipped} other vehicles left out, {dropped} Enhanced-only)", models.len(), files.iter().filter(|f| f.file_name().is_some_and(|n| n.eq_ignore_ascii_case("vehicles.meta"))).count());
    Ok(())
}
