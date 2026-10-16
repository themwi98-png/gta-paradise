//! The build and install steps (the same as tools/build_dlc.sh,
//! tools/build_crash.sh and tools/install_dlc.sh), run in the working
//! folder, which is laid out like the project folder (tools/..., out/...).
//!
//! Each step has a fingerprint: the tools, the game folders, the mod data it
//! reads and the fingerprints of the steps it builds on. A step whose
//! fingerprint matches its last successful run, and whose results are still
//! there, is skipped.

use crate::{Games, Layout, SETUP_VERSION};
use anyhow::{bail, Context, Result};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Burnout -> GTA placement (9.9 puts Paradise's sea at GTA's sea level).
const OFFSET: [&str; 2] = ["--offset", "0,0,9.9"];
const COL: &str = "tools/collision/paradise_extra.pcx";

struct Step {
    name: &'static str,
    title: &'static str,
    /// Mod data it reads (files or folders, relative to the working folder).
    inputs: &'static [&'static str],
    /// Steps whose results it builds on.
    deps: &'static [&'static str],
    /// Results that must exist for it to count as done.
    outputs: &'static [&'static str],
    /// Anything else its fingerprint covers (files outside the working folder).
    extra: fn(&Ctx) -> String,
    run: fn(&Ctx) -> Result<()>,
}

fn nothing(_: &Ctx) -> String {
    String::new()
}

/// A multiplier baked into the car data: `key` from the player's
/// paradise.ini in the GTA folder, else the mod's default.
fn baked(c: &Ctx, key: &str, default: f64) -> f64 {
    let read = |p: &Path| -> Option<f64> {
        let text = std::fs::read_to_string(p).ok()?;
        let line = text.lines().find(|l| l.split_whitespace().next().is_some_and(|k| k.eq_ignore_ascii_case(key)))?;
        line.split_whitespace().nth(1)?.parse::<f64>().ok()
    };
    read(&c.games.gta.join("paradise.ini")).or_else(|| read(&c.path("tools/paradise.ini"))).unwrap_or(default).clamp(0.5, 5.0)
}

/// `@car_grip` and `@car_brakes`, as given to `bpx handling`. (Part of the
/// handling step's fingerprint, so changing either redoes that step.)
fn handling_args(c: &Ctx) -> String {
    format!("--grip {} --brakes {}", baked(c, "@car_grip", 1.2), baked(c, "@car_brakes", 1.75))
}

/// What the install step's fingerprint covers outside the working folder:
/// the plugin, Rockstar's files it starts from (a GTA update changes them)
/// and the installed files themselves (gone or changed: install again).
fn installed(c: &Ctx) -> String {
    let g = &c.games.gta;
    let mut ids = vec![file_id(&c.layout.asi)];
    for rel in ["GTA5.exe", "update/update.rpf", "mods/update/update.rpf", "mods/update/x64/dlcpacks/bpparadise/dlc.rpf", "paradise.asi", "paradise_col.bin", "paradise_props.csv"] {
        ids.push(file_id(&g.join(rel)));
    }
    // A deleted paradise.ini (to get the defaults back) is put back. Its
    // contents aren't covered: changing settings shouldn't reinstall.
    ids.push(g.join("paradise.ini").is_file().to_string());
    ids.join("|")
}

const STEPS: &[Step] = &[
    Step {
        name: "collision",
        title: "Collision",
        inputs: &[COL, "tools/edits/collision_edits.json"],
        deps: &[],
        outputs: &["out/gta_col", "out/gta_col_xml/paradise_col.bin"],
        extra: nothing,
        run: collision,
    },
    Step { name: "visual", title: "City models and textures (the long one)", inputs: &[], deps: &[], outputs: &["out/gta_vis", "out/meta_vis"], extra: nothing, run: visual },
    Step { name: "cargens", title: "Parked cars", inputs: &["tools/edits/traffic_edits.json"], deps: &[], outputs: &["out/meta_cargens"], extra: nothing, run: cargens },
    Step {
        name: "props",
        title: "Street props",
        inputs: &["tools/props_map.csv", COL, "tools/edits/collision_edits.json"],
        deps: &[],
        outputs: &["out/meta_props", "out/props/paradise_props.csv"],
        extra: nothing,
        run: props,
    },
    Step { name: "pack", title: "Map package", inputs: &[], deps: &["collision", "visual", "cargens", "props"], outputs: &["out/dlc/bpparadise/dlc.rpf"], extra: nothing, run: pack },
    Step {
        name: "water",
        title: "Sea and lakes",
        inputs: &[COL, "tools/edits/collision_edits.json", "tools/edits/water_edits.json"],
        deps: &[],
        outputs: &["out/water_paradise.xml"],
        extra: nothing,
        run: water,
    },
    Step { name: "paths", title: "Roads and traffic", inputs: &["tools/edits/traffic_edits.json", COL], deps: &[], outputs: &["out/gta_paths"], extra: nothing, run: paths },
    Step {
        name: "navmesh",
        title: "Pedestrian paths",
        inputs: &[COL, "tools/edits/collision_edits.json", "tools/edits/sidewalk_edits.json"],
        deps: &[],
        outputs: &["out/gta_navmesh"],
        extra: nothing,
        run: navmesh,
    },
    Step { name: "handling", title: "Car handling", inputs: &[], deps: &[], outputs: &["out/crash/update/common/data/handling.meta"], extra: handling_args, run: handling },
    Step {
        name: "install",
        title: "Installing into GTA V",
        inputs: &["tools/paradise.ini", "tools/edits/spawn_blocks.json", "tools/base_map_disable.txt", "tools/base_map_packs.txt"],
        deps: &["pack", "water", "paths", "navmesh", "handling"],
        outputs: &[],
        extra: installed,
        run: install,
    },
];

pub struct Ctx<'a> {
    layout: &'a Layout,
    games: &'a Games,
    log: std::cell::RefCell<std::fs::File>,
}

impl Ctx<'_> {
    fn path(&self, rel: &str) -> PathBuf {
        self.layout.root.join(rel)
    }

    fn gta(&self) -> String {
        self.games.gta.to_string_lossy().into_owned()
    }

    fn burnout(&self) -> String {
        self.games.burnout.to_string_lossy().into_owned()
    }

    fn note(&self, line: &str) {
        let _ = writeln!(self.log.borrow_mut(), "{line}");
    }

    /// Runs a tool in the working folder, its output going to the log.
    fn tool(&self, exe: &Path, args: &[&str]) -> Result<()> {
        self.note(&format!("> {} {}", exe.file_name().unwrap_or_default().to_string_lossy(), args.join(" ")));
        let log = self.log.borrow_mut().try_clone()?;
        let status = crate::command(exe)
            .args(args)
            .current_dir(&self.layout.root)
            .env("CODEWALKER_DIR", &self.games.codewalker)
            .env("BPX_STOCK_COL", self.games.burnout.join("WORLDCOL.BIN"))
            .env("DOTNET_NOLOGO", "1")
            .stdout(log.try_clone()?)
            .stderr(log)
            .status()
            .with_context(|| format!("starting {}", exe.display()))?;
        if !status.success() {
            bail!("{} {} failed", exe.file_name().unwrap_or_default().to_string_lossy(), args.first().unwrap_or(&""));
        }
        Ok(())
    }

    fn bpx(&self, args: &[&str]) -> Result<()> {
        self.tool(&self.layout.bpx, args)
    }

    fn cw(&self, args: &[&str]) -> Result<()> {
        self.tool(&self.layout.cwconv, args)
    }

    /// Deletes working-folder paths (files or folders), if there.
    fn clear(&self, rels: &[&str]) -> Result<()> {
        for rel in rels {
            let p = self.path(rel);
            if p.is_dir() {
                std::fs::remove_dir_all(&p).with_context(|| format!("clearing {}", p.display()))?;
            } else if p.is_file() {
                std::fs::remove_file(&p)?;
            }
        }
        Ok(())
    }
}

// --- Fingerprints ---------------------------------------------------------

/// FNV-1a, 64-bit (stable from build to build, unlike std's hasher).
#[derive(Clone, Copy)]
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn add(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        // Separator, so ("ab","c") and ("a","bc") differ.
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0100_0000_01b3);
    }
}

/// Adds a file's contents, or a folder's files (sorted), or "missing".
fn add_path(h: &mut Fnv, p: &Path) -> Result<()> {
    if p.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(p)?.filter_map(|e| Some(e.ok()?.path())).collect();
        entries.sort();
        for e in entries {
            h.add(e.file_name().unwrap_or_default().to_string_lossy().as_bytes());
            add_path(h, &e)?;
        }
    } else if p.is_file() {
        h.add(&std::fs::read(p)?);
    } else {
        h.add(b"missing");
    }
    Ok(())
}

fn stamp_path(c: &Ctx, step: &str) -> PathBuf {
    c.path("out/stamps").join(step)
}

fn fingerprint(c: &Ctx, step: &Step, tools: Fnv) -> Result<String> {
    let mut h = tools;
    h.add(step.name.as_bytes());
    for rel in step.inputs {
        h.add(rel.as_bytes());
        add_path(&mut h, &c.path(rel))?;
    }
    for d in step.deps {
        h.add(std::fs::read(stamp_path(c, d)).unwrap_or_default().as_slice());
    }
    h.add((step.extra)(c).as_bytes());
    Ok(format!("{:016x}", h.0))
}

// --- Running --------------------------------------------------------------

/// How a step went, for the window or console showing progress.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Running,
    UpToDate,
    /// Done, in this many seconds.
    Done(u64),
    Failed,
}

/// Progress: step number (from 0), its title, and how it's going.
pub type Report<'a> = &'a (dyn Fn(usize, &str, State) + Sync);

/// The steps' titles, in order.
pub fn titles() -> Vec<&'static str> {
    STEPS.iter().map(|s| s.title).collect()
}

pub fn log_path(layout: &Layout) -> PathBuf {
    layout.root.join("out/setup.log")
}

/// The last `n` lines of the log (shown when a step fails).
pub fn log_tail(layout: &Layout, n: usize) -> Vec<String> {
    let text = std::fs::read_to_string(log_path(layout)).unwrap_or_default();
    let mut tail: Vec<String> = text.lines().rev().take(n).map(str::to_string).collect();
    tail.reverse();
    tail
}

pub fn run_all(layout: &Layout, games: &Games, only: Option<&[String]>, rebuild: bool, report: Report) -> Result<()> {
    if let Some(only) = only {
        for o in only {
            if !STEPS.iter().any(|s| s.name == o) {
                bail!("no step called {o} (steps: {})", STEPS.iter().map(|s| s.name).collect::<Vec<_>>().join(", "));
            }
        }
    }
    std::fs::create_dir_all(layout.root.join("out/stamps"))?;
    let log_path = log_path(layout);
    let log = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
    let c = Ctx { layout, games, log: std::cell::RefCell::new(log) };
    c.note(&format!("\n===== setup {SETUP_VERSION} ====="));

    // What every step depends on: this setup, the tools and the game folders.
    let mut tools = Fnv::new();
    tools.add(SETUP_VERSION.as_bytes());
    tools.add(&std::fs::read(&layout.bpx)?);
    tools.add(&std::fs::read(&layout.cwconv)?);
    for g in [&games.gta, &games.burnout, &games.codewalker] {
        tools.add(g.to_string_lossy().as_bytes());
    }

    for (i, step) in STEPS.iter().enumerate() {
        if only.is_some_and(|o| !o.iter().any(|n| n == step.name)) {
            continue;
        }
        let fp = fingerprint(&c, step, tools)?;
        let stamp = stamp_path(&c, step.name);
        let done = std::fs::read_to_string(&stamp).ok().as_deref() == Some(fp.as_str()) && step.outputs.iter().all(|o| c.path(o).exists());
        if done && !rebuild {
            report(i, step.title, State::UpToDate);
            continue;
        }
        report(i, step.title, State::Running);
        let _ = std::fs::remove_file(&stamp);
        c.note(&format!("--- {} ---", step.name));
        let t = Instant::now();
        if let Err(e) = (step.run)(&c) {
            report(i, step.title, State::Failed);
            return Err(e.context(format!("step '{}'", step.name)));
        }
        // Fingerprinted again afterwards: installing changes the files the
        // install step's fingerprint covers.
        std::fs::write(&stamp, fingerprint(&c, step, tools)?)?;
        report(i, step.title, State::Done(t.elapsed().as_secs()));
    }
    Ok(())
}

// --- The steps ------------------------------------------------------------

fn collision(c: &Ctx) -> Result<()> {
    c.clear(&["out/gta_col_xml", "out/gta_col"])?;
    c.bpx(&["gta-collision", COL, "out/gta_col_xml", OFFSET[0], OFFSET[1]])?;
    // Streaming test slab above Paleto Bay (outside the Burnout footprint).
    c.bpx(&["gta-test-platform", "out/gta_col_xml", "bp_test_platform", "-150", "6450", "1015"])?;
    c.cw(&["convert", "out/gta_col_xml", "out/gta_col"])
}

fn visual(c: &Ctx) -> Result<()> {
    c.clear(&["out/gta_vis_xml", "out/gta_vis", "out/meta_vis"])?;
    c.bpx(&["gta-visual", &c.burnout(), "out/gta_vis_xml", "out/meta_vis", "--units", "all", OFFSET[0], OFFSET[1]])?;
    c.cw(&["convert", "out/gta_vis_xml", "out/gta_vis"])
}

fn cargens(c: &Ctx) -> Result<()> {
    c.clear(&["out/meta_cargens"])?;
    c.bpx(&["gta-cargens", &c.burnout(), "out/meta_cargens", OFFSET[0], OFFSET[1]])
}

fn props(c: &Ctx) -> Result<()> {
    if !c.path("out/props/gta_archetypes.csv").is_file() {
        std::fs::create_dir_all(c.path("out/props"))?;
        c.cw(&["archetypes", &c.gta(), "x64*.rpf", "levels/gta5/props/", "out/props/gta_archetypes.csv"])?;
    }
    c.clear(&["out/meta_props"])?;
    c.bpx(&["gta-props", &c.burnout(), "out/meta_props", OFFSET[0], OFFSET[1], "--col", COL])
}

/// Copies the files in `from` into `to` (recursively).
fn copy_into(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let e = e?;
        if e.file_type()?.is_dir() {
            copy_into(&e.path(), &to.join(e.file_name()))?;
        } else {
            std::fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    Ok(())
}

fn pack(c: &Ctx) -> Result<()> {
    c.clear(&["out/gta_meta_xml", "out/gta_meta", "out/dlc"])?;
    for part in ["out/meta_vis", "out/meta_cargens", "out/meta_props"] {
        copy_into(&c.path(part), &c.path("out/gta_meta_xml"))?;
    }
    c.cw(&["convert", "out/gta_meta_xml", "out/gta_meta"])?;
    c.cw(&["pack", "bpparadise", "out/dlc", "out/gta_col", "bpparadise_vis=out/gta_vis", "bpparadise_metadata=out/gta_meta:meta"])
}

fn water(c: &Ctx) -> Result<()> {
    if !c.path("out/gta_water.xml").is_file() {
        c.cw(&["extract", &c.gta(), r"common.rpf\data\levels\gta5\water.xml", "out/gta_water.xml"])?;
    }
    c.bpx(&["water-meshes", &c.burnout(), "out/water_meshes.csv", OFFSET[0], OFFSET[1]])?;
    c.bpx(&["gta-water", COL, "out/gta_water.xml", "out/water_paradise.xml", "--lakes", "out/water_meshes.csv", OFFSET[0], OFFSET[1]])
}

fn paths(c: &Ctx) -> Result<()> {
    c.clear(&["out/gta_paths_xml", "out/gta_paths"])?;
    c.bpx(&["gta-paths", &c.burnout(), "out/gta_paths_xml", OFFSET[0], OFFSET[1]])?;
    c.cw(&["convert", "out/gta_paths_xml", "out/gta_paths"])
}

fn navmesh(c: &Ctx) -> Result<()> {
    c.clear(&["out/gta_navmesh_xml", "out/gta_navmesh"])?;
    c.bpx(&["gta-navmesh", COL, "out/gta_navmesh_xml", OFFSET[0], OFFSET[1]])?;
    c.cw(&["convert", "out/gta_navmesh_xml", "out/gta_navmesh"])?;
    // GTA's own vehicle navmeshes (trains, boats, trailers) go in too.
    if !c.path("out/vehicle_navmeshes").is_dir() {
        c.cw(&["rpf-extract", &c.gta(), "x64f.rpf", r"navmeshes[0-9]\.rpf/[^\[/]+\.ynv$", "out/vehicle_navmeshes"])?;
    }
    for e in std::fs::read_dir(c.path("out/vehicle_navmeshes"))? {
        let e = e?;
        if e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("ynv")) {
            std::fs::copy(e.path(), c.path("out/gta_navmesh").join(e.file_name()))?;
        }
    }
    Ok(())
}

fn handling(c: &Ctx) -> Result<()> {
    // Rockstar's handling files, from the untouched (non-mods) archives.
    let src = "out/handling_src";
    if !c.path("out/handling_src/_update/update/update.rpf/common/data/handling.meta").is_file() {
        c.clear(&[src])?;
        c.cw(&["rpf-extract", &c.gta(), "update/update.rpf", r"handling\.meta$", "out/handling_src/_update", "--keep-paths"])?;
        let packs = c.games.gta.join(r"update\x64\dlcpacks");
        for e in std::fs::read_dir(&packs)?.filter_map(|e| e.ok()).filter(|e| e.path().join("dlc.rpf").is_file()) {
            let p = e.file_name().to_string_lossy().into_owned();
            let out = format!("out/handling_src/{p}");
            c.cw(&["rpf-extract", &c.gta(), &format!("update/x64/dlcpacks/{p}/dlc.rpf"), r"handling\.meta$", &out])?;
            if !c.path(&out).join("handling.meta").is_file() {
                c.clear(&[&out])?;
            }
        }
    }
    c.clear(&["out/crash"])?;
    let args = handling_args(c);
    let mut cmd = vec!["handling", src, "out/crash/update"];
    cmd.extend(args.split_whitespace());
    c.bpx(&cmd)
}

// --- Install --------------------------------------------------------------

pub fn gta_running() -> bool {
    crate::command("tasklist")
        .args(["/FI", "IMAGENAME eq GTA5.exe"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("gta5.exe"))
}

/// Size and modified time of a file, to notice when Rockstar updates it.
fn file_id(p: &Path) -> String {
    std::fs::metadata(p).map(|m| format!("{} {:?}", m.len(), m.modified().ok())).unwrap_or_default()
}

/// Defaults earlier releases shipped that have since changed: a player
/// still on one of these was never asked for it, so they get the new one.
const OLD_DEFAULTS: [&str; 7] = [
    "@wreck_speed 18",
    "@prop_keep 0.9",
    "@police_top 50",
    "@air_gravity 2",
    "@downforce 1",
    "@boost_max_top 95",
    "@boost_seconds 7",
];

/// The new paradise.ini, keeping the player's own value for every setting
/// they already had (new settings come in with their defaults, and a value
/// left at an old default moves to the new one).
fn merge_ini(new: &str, old: &str) -> String {
    let key = |l: &str| l.trim_start().starts_with('@').then(|| l.split_whitespace().next().unwrap_or("").to_lowercase());
    let words = |l: &str| l.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>();
    let old_default = |l: &str| OLD_DEFAULTS.iter().any(|d| words(d) == words(l));
    let mut out = String::new();
    for line in new.lines() {
        let kept = key(line).and_then(|k| old.lines().find(|o| key(o).as_deref() == Some(k.as_str()))).filter(|o| !old_default(o));
        out.push_str(kept.unwrap_or(line));
        out.push_str("\r\n");
    }
    out
}

fn install(c: &Ctx) -> Result<()> {
    let gta = &c.games.gta;
    if gta.join("GTA5_Enhanced.exe").is_file() {
        bail!("refusing: {} is the Enhanced edition", gta.display());
    }
    if gta_running() {
        bail!("GTA V is running and locks its files; close it and try again");
    }
    let g = c.gta();
    let mods_update = gta.join(r"mods\update\update.rpf");
    let mods_update_s = mods_update.to_string_lossy().into_owned();

    std::fs::create_dir_all(gta.join(r"mods\update\x64\dlcpacks\bpparadise"))?;
    std::fs::copy(c.path("out/dlc/bpparadise/dlc.rpf"), gta.join(r"mods\update\x64\dlcpacks\bpparadise\dlc.rpf"))?;

    // The mods folder's update.rpf: a copy of Rockstar's, made again when
    // Rockstar updates theirs. One that was there before the mod (another
    // mod's) is backed up first.
    let orig_update = gta.join(r"update\update.rpf");
    let stamp = c.path("out/stamps/mods_update_source");
    let ours = std::fs::read_to_string(&stamp).ok();
    // (Not in development, where it is this project's own.)
    if mods_update.is_file() && ours.is_none() && c.layout.data.is_some() {
        let backup = gta.join(r"mods\update\update.rpf.before-paradise");
        if !backup.exists() {
            c.note("backing up the existing mods update.rpf");
            std::fs::copy(&mods_update, &backup)?;
        }
    }
    if !mods_update.is_file() || ours.as_deref().is_some_and(|s| s != file_id(&orig_update)) {
        c.note("copying update.rpf into the mods folder");
        std::fs::copy(&orig_update, &mods_update)?;
    }
    std::fs::write(&stamp, file_id(&orig_update))?;
    c.cw(&["dlclist-add", &g, "bpparadise", &mods_update_s, "--open"])?;

    // Los Santos out: its archives dropped from the level's file lists
    // (always starting from Rockstar's untouched files).
    std::fs::create_dir_all(c.path("out/base_meta"))?;
    c.cw(&["extract", &g, r"update\update.rpf\common\data\levels\gta5\images.meta", "out/base_meta/images.orig.meta"])?;
    c.cw(&["extract", &g, r"update\update.rpf\common\data\levels\gta5\gta5.meta", "out/base_meta/gta5.orig.meta"])?;
    c.bpx(&["strip-base-map", "tools/base_map_disable.txt", "out/base_meta/images.orig.meta", "out/base_meta/gta5.orig.meta", "out/base_meta"])?;
    c.cw(&["rpf-put", &g, &mods_update_s, "common/data/levels/gta5/images.meta", "out/base_meta/images.meta", "--open"])?;
    c.cw(&["rpf-put", &g, &mods_update_s, "common/data/levels/gta5/gta5.meta", "out/base_meta/gta5.meta", "--open"])?;

    // Rockstar's patch packs re-add updated Los Santos sectors: the mods
    // folder gets a copy of each with those taken out.
    let packs = std::fs::read_to_string(c.path("tools/base_map_packs.txt"))?;
    for p in packs.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let src = gta.join(format!(r"update\x64\dlcpacks\{p}\dlc.rpf"));
        if !src.is_file() {
            continue;
        }
        let dst = gta.join(format!(r"mods\update\x64\dlcpacks\{p}\dlc.rpf"));
        let stamp = c.path(&format!("out/stamps/pack_{p}"));
        if !dst.is_file() || std::fs::read_to_string(&stamp).ok() != Some(file_id(&src)) {
            std::fs::create_dir_all(dst.parent().unwrap())?;
            std::fs::copy(&src, &dst)?;
            std::fs::write(&stamp, file_id(&src))?;
        }
        let dst_s = dst.to_string_lossy().into_owned();
        let (orig, out) = (format!("out/base_meta/{p}.orig.xml"), format!("out/base_meta/{p}.xml"));
        c.cw(&["extract", &g, &format!(r"update\x64\dlcpacks\{p}\dlc.rpf\content.xml"), &orig])?;
        c.bpx(&["strip-base-map", "pack", &orig, &out])?;
        c.cw(&["rpf-put", &g, &dst_s, "content.xml", &out, "--open"])?;
        // The game still mounts some of these archives with no data file
        // naming them, so they're emptied as well.
        c.cw(&["rpf-empty", &g, &dst_s, r"levels/gta5/(_citye|_cityw|_hills)/|levels/gta5/[^/]*_additions[^/]*\.rpf$", "--open"])?;
    }

    // Roads, pedestrian paths (navmeshes0 holds ours; 1-9 are emptied).
    c.cw(&["rpf-replace", &g, &mods_update_s, "x64/levels/gta5/paths.rpf", "out/gta_paths", "--open"])?;
    c.cw(&["rpf-replace", &g, &mods_update_s, "x64/levels/gta5/navmeshes0.rpf", "out/gta_navmesh", "--open"])?;
    std::fs::create_dir_all(c.path("out/empty_dir_unused"))?;
    for i in 1..=9 {
        c.cw(&["rpf-replace", &g, &mods_update_s, &format!("x64/levels/gta5/navmeshes{i}.rpf"), "out/empty_dir_unused", "--open"])?;
    }

    // Population zones from Burnout's districts.
    c.cw(&["extract", &g, r"update\update.rpf\common\data\levels\gta5\popzone.ipl", "out/base_meta/popzone.orig.ipl"])?;
    c.bpx(&["popzone", &c.burnout(), "out/base_meta/popzone.orig.ipl", "out/popzone_paradise.ipl"])?;
    c.cw(&["rpf-put", &g, &mods_update_s, "common/data/levels/gta5/popzone.ipl", "out/popzone_paradise.ipl", "--open"])?;

    // Sand, dirt, grass... as fast as the road.
    c.cw(&["extract", &g, r"update\update.rpf\common\data\materials\materials.dat", "out/base_meta/materials.orig.dat"])?;
    c.bpx(&["ground-materials", "out/base_meta/materials.orig.dat", "out/materials_paradise.dat"])?;
    c.cw(&["rpf-put", &g, &mods_update_s, "common/data/materials/materials.dat", "out/materials_paradise.dat", "--open"])?;

    // Car handling, at its update.rpf paths.
    let crash = c.path("out/crash/update");
    let mut files = Vec::new();
    fn walk(d: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for e in std::fs::read_dir(d)? {
            let p = e?.path();
            if p.is_dir() {
                walk(&p, out)?;
            } else {
                out.push(p);
            }
        }
        Ok(())
    }
    walk(&crash, &mut files)?;
    for f in files {
        let rel = f.strip_prefix(&crash)?.to_string_lossy().replace('\\', "/");
        c.cw(&["rpf-put", &g, &mods_update_s, &rel, &f.to_string_lossy(), "--open"])?;
    }
    c.cw(&["rpf-put", &g, &mods_update_s, "common/data/levels/gta5/water.xml", "out/water_paradise.xml", "--open"])?;

    // The plugin and its files next to GTA5.exe.
    std::fs::copy(&c.layout.asi, gta.join("paradise.asi"))?;
    let new_ini = std::fs::read_to_string(c.path("tools/paradise.ini"))?;
    let ini = match std::fs::read_to_string(gta.join("paradise.ini")) {
        Ok(old) => merge_ini(&new_ini, &old),
        Err(_) => new_ini,
    };
    std::fs::write(gta.join("paradise.ini"), ini)?;
    std::fs::copy(c.path("out/gta_col_xml/paradise_col.bin"), gta.join("paradise_col.bin"))?;
    std::fs::copy(c.path("out/props/paradise_props.csv"), gta.join("paradise_props.csv"))?;
    if c.path("tools/edits/spawn_blocks.json").is_file() {
        std::fs::copy(c.path("tools/edits/spawn_blocks.json"), gta.join("paradise_spawn_blocks.json"))?;
    }
    c.note("installed");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn ini_keeps_player_values() {
        let new = "# boost\n@boost on\n@boost_fill 1.5\n@new_thing 3\n";
        let old = "@boost_fill 2.5\n@gone 1\n";
        assert_eq!(super::merge_ini(new, old), "# boost\r\n@boost on\r\n@boost_fill 2.5\r\n@new_thing 3\r\n");
    }

    #[test]
    fn ini_moves_old_defaults() {
        let new = "@boost_seconds 15\n@air_gravity 3\n";
        let old = "@boost_seconds 7\n@air_gravity 2.5\n";
        assert_eq!(super::merge_ini(new, old), "@boost_seconds 15\r\n@air_gravity 2.5\r\n");
    }
}
