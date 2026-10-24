mod collision;
mod colsrc;
mod edits;
mod export;
mod fill;
mod gta;
mod gtadata;
mod gtavis;
mod junctions;
mod minimap;
mod navmesh;
mod paths;
mod props;
mod repair;
mod shops;
mod tex;
mod water;

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

const USAGE: &str = "usage:
  bpx info <bundle>...
  bpx dump <bundle> <type-hex> [count] [bytes]
  bpx export <game-dir> <out-dir> <unit|all> [unit...]
  bpx overview <game-dir> <out-dir> <unit|all> [unit...]   (low detail, untextured)
  bpx handling <handling_src-dir> <out-dir> [--grip x] [--brakes x] [--tyres-clip on|off]   (Burnout-style crash damage, speed, grip, brakes)
  bpx strip-base-map <list.txt> <images.meta> <gta5.meta> <out-dir> | pack <content.xml> <out>
  bpx popzone <burnout-dir> <original popzone.ipl> <out popzone.ipl>
  bpx ground-materials <original materials.dat> <out materials.dat>   (no ground slows cars)
  bpx col-split <stock WORLDCOL.BIN> <combined collision> <out.pcx>
  bpx col-compare <stock WORLDCOL.BIN> <modded WORLDCOL.BIN> [extra.pcx]   (what a modded map changes)
  bpx col-modcompat <stock WORLDCOL.BIN> <modded WORLDCOL.BIN> <extra.pcx> <map name>
      (added pieces that clash with a modded map's collision -> tools/collision/modded_maps.json)
  bpx col-fill <collision> <burnout-dir> <out.bin> [--png <overview.png>]
      (collision for Burnout's visible ground where there's none; BPX_FILL / out/collision_fill)
  bpx col-gen <collision> <burnout-dir> <out.bin> [--png <overview.png>]
      (all collision beyond the map's own from Burnout's visible models: ground, cliffs,
       structures, mountains; replaces the added pieces with BPX_ADDED=none BPX_FILL=<out.bin>)
  bpx col-gen-compare <collision> <gen.bin> <out.png>   (old added pieces against generated collision, top-down)
  bpx col-fill-view <fill.bin> <out-dir>   (the fill as the viewer's Hole fill layer)
  bpx col-fill-survey <collision> <burnout-dir> <fill.bin> <x> <z> <r>   (what isn't covered near a Burnout point)
  bpx col-fill-ground-ids <stock burnout-dir> <out.txt> [<modded burnout-dir>]   (ground model ids, for modded maps' unnamed units)
  bpx collision <game-dir|WORLDCOL file> <out-dir> <unit|all> [unit...] [--base <WORLDCOL>]
  bpx surfaces <game-dir> [unit-step]   (report what each collision surface id is)
  bpx materials <game-dir> [unit-step]  (report material texture-slot roles per shader)
  bpx gta-collision <game-dir|WORLDCOL file> <out-dir> [--offset x,y,z] [--scale s] [--heading deg] [--walls skip|vehicles|solid] [--prefix p]
      (GTA V .ybn.xml per unit; convert with tools/cwconv)
  bpx gta-test-platform <out-dir> <name> <x> <y> <top-z> [size]   (flat test collision slab)
  bpx gta-visual <game-dir> <assets-out> <meta-out> [--units 1,2,..|all] [--offset x,y,z] [--scale s] [--heading deg] [--lod-dist m] [--lod-far m|off] [--prefix p]
      (GTA V .ydr/.ytd/.ytyp/.ymap XML per unit; convert with tools/cwconv)
  bpx gta-paths <game-dir> <out-dir> [--offset x,y,z] [--lanes]
      (GTA traffic paths, nodes0..1023.ynd.xml: two-way roads built from Burnout's
      traffic lanes; --lanes: one one-way road per lane, the earlier way)
  bpx gta-junctions <game-dir> <out-dir> [--offset x,y,z] [--props paradise_props.csv]
      (junctions.json for the viewer and paradise_junctions.txt for the plugin:
      Burnout's traffic lights, junctions where lanes cross, ones marked by hand)
  bpx gta-cars <vehicles.meta-dir> <out-file>
      (paradise_cars.txt for the plugin's junkyard: every road vehicle's model name)
  bpx gta-shops <game-dir> <out-dir> [--offset x,y,z] [--lots junkyard_lots.csv]
      (paradise_shops.txt for the plugin: Burnout's junkyards, auto repairs,
      gas stations and paint shops; `bpx shops <game-dir>` lists every trigger region)
  bpx gta-cargens <game-dir> <meta-out> [--offset x,y,z]
      (Burnout's parked cars as GTA car generator ymaps)
  bpx gta-navmesh <game-dir|WORLDCOL file> <out-dir> [--offset x,y,z]
      (GTA pedestrian navmesh tiles, navmesh[x][y].ynv.xml, built with Recast)
  bpx traffic-json <game-dir> <out.json>
      (Burnout lanes and parked cars for the viewer traffic editor)
  bpx traffic <game-dir> <out.csv> [--offset x,y,z]
      (Burnout traffic lanes: statistics and placed lane centre lines)
  bpx water-meshes <game-dir> <out.csv> [--offset x,y,z]
      (placed vertices of Burnout's visible water meshes, for water levels)
  bpx gta-water <game-dir|WORLDCOL file> <gta water.xml> <out water.xml> [--offset x,y,z] [--lakes water_meshes.csv]
      (adds sea quads around Paradise City / over replaced Los Santos)
  bpx gta-minimap <game-dir|WORLDCOL file> <out-dir> [--offset x,y,z] [--ymt <minimap.ymt.pso.xml>] [--water <water.xml>] [--names <gta tile names.csv>]
      (GTA's radar tiles, zoomed-out map images and minimap.ymt for Paradise City)";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A modded map can ship bundles without debug names: name their
    // resources from the game's other bundles.
    if matches!(args.first().map(String::as_str), Some("export" | "overview" | "gta-visual" | "props" | "gta-props" | "water-meshes" | "materials")) {
        if let Some(game) = args.get(1).map(PathBuf::from).filter(|p| p.is_dir()) {
            bnd2::set_fallback_names(bnd2::debug_names(&game));
        }
    }
    match args.first().map(String::as_str) {
        Some("info") => info(&args[1..]),
        Some("vd-dump") if args.len() >= 4 => vd_dump(&PathBuf::from(&args[1]), &args[2], &args[3]),
        Some("backdrop-dump") if args.len() >= 2 => backdrop_dump(&PathBuf::from(&args[1])),
        Some("pvs-dump") if args.len() >= 2 => pvs_dump(&PathBuf::from(&args[1])),
        Some("inst-dump") if args.len() >= 2 => inst_dump(&PathBuf::from(&args[1])),
        Some("mat-dump") if args.len() >= 3 => mat_dump(&PathBuf::from(&args[1]), &args[2], args.get(3).map(String::as_str)),
        Some("dump") => dump(&args[1..]),
        Some("export") if args.len() >= 4 => export(&args[1..], false),
        Some("overview") if args.len() >= 4 => export(&args[1..], true),
        Some("surfaces") if args.len() >= 2 => {
            let step = args.get(2).map_or(Ok(5), |s| s.parse())?;
            collision::analyse_surfaces(&PathBuf::from(&args[1]), step)
        }
        Some("materials") if args.len() >= 2 => {
            let step = args.get(2).map_or(Ok(8), |s| s.parse())?;
            collision::analyse_materials(&PathBuf::from(&args[1]), step)
        }
        Some("gta-cars") if args.len() >= 3 => gtadata::cars(&PathBuf::from(&args[1]), &PathBuf::from(&args[2])),
        Some("handling") if args.len() >= 3 => {
            let opt = |flag: &str, default: f64| -> Result<f64> {
                match args.iter().position(|a| a == flag) {
                    Some(i) => Ok(args.get(i + 1).with_context(|| format!("{flag} needs a value"))?.parse()?),
                    None => Ok(default),
                }
            };
            let (grip, brakes) = (opt("--grip", gtadata::GRIP_DEFAULT)?, opt("--brakes", gtadata::BRAKES_DEFAULT)?);
            let tyres_clip = match args.iter().position(|a| a == "--tyres-clip") {
                Some(i) => args.get(i + 1).map(String::as_str) != Some("off"),
                None => false,
            };
            gtadata::handling(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), grip, brakes, tyres_clip)
        }
        Some("strip-base-map") if args.len() >= 4 && args[1] == "pack" => gtadata::strip_pack(&PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("strip-base-map") if args.len() >= 5 => {
            gtadata::strip_base_map(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3]), &PathBuf::from(&args[4]))
        }
        Some("ground-materials") if args.len() >= 3 => gtadata::ground_materials(&PathBuf::from(&args[1]), &PathBuf::from(&args[2])),
        Some("popzone") if args.len() >= 4 => gtadata::popzone(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("col-bounds") if args.len() >= 2 => colsrc::bounds(&PathBuf::from(&args[1])),
        Some("col-modcompat") if args.len() >= 5 => colsrc::modcompat(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3]), &args[4]),
        Some("col-compare") if args.len() >= 4 => colsrc::compare_added(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("col-compare") if args.len() >= 3 => colsrc::compare(&PathBuf::from(&args[1]), &PathBuf::from(&args[2])),
        Some("col-split") if args.len() >= 4 => colsrc::split(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("col-fill") if args.len() >= 4 => {
            let png = args.iter().position(|a| a == "--png").and_then(|i| args.get(i + 1)).map(PathBuf::from);
            fill::generate(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3]), png.as_deref(), false)
        }
        Some("col-gen") if args.len() >= 4 => {
            let png = args.iter().position(|a| a == "--png").and_then(|i| args.get(i + 1)).map(PathBuf::from);
            fill::generate(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3]), png.as_deref(), true)
        }
        Some("col-gen-compare") if args.len() >= 4 => fill::compare_gen(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("col-fill-view") if args.len() >= 3 => fill::export_viewer(&PathBuf::from(&args[1]), &PathBuf::from(&args[2])),
        Some("col-fill-ground-ids") if args.len() >= 3 => fill::ground_ids(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), args.get(3).map(PathBuf::from).as_deref()),
        Some("col-fill-survey") if args.len() >= 7 => {
            let n: Vec<f64> = args[4..7].iter().map(|s| s.parse()).collect::<Result<_, _>>()?;
            fill::survey(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3]), n[0], n[1], n[2])
        }
        Some("collision") if args.len() >= 4 => {
            let mut rest: Vec<&String> = args[3..].iter().collect();
            let mut base = None;
            if let Some(i) = rest.iter().position(|a| *a == "--base") {
                base = rest.get(i + 1).map(|s| PathBuf::from(s.as_str()));
                rest.drain(i..(i + 2).min(rest.len()));
            }
            let units: Vec<u32> = if rest.first().is_none_or(|a| *a == "all") {
                Vec::new()
            } else {
                rest.iter().map(|s| s.parse()).collect::<Result<_, _>>()?
            };
            let filter = (!units.is_empty()).then_some(units.as_slice());
            let base = base.map(|b| if b.is_dir() { b.join("WORLDCOL.BIN") } else { b });
            collision::export(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), filter, base.as_deref())
        }
        Some("gta-collision") if args.len() >= 3 => gta_collision(&args[1..]),
        Some("gta-visual") if args.len() >= 4 => gta_visual(&args[1..]),
        Some("gta-paths") if args.len() >= 3 => {
            if args.iter().any(|a| a == "--lanes") {
                paths::export(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?)
            } else {
                paths::export_roads(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?)
            }
        }
        Some("gta-junctions") if args.len() >= 3 => {
            let props = args.iter().position(|a| a == "--props").and_then(|i| args.get(i + 1)).map_or_else(|| PathBuf::from("out/props/paradise_props.csv"), PathBuf::from);
            junctions::export(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?, &props)
        }
        Some("gta-shops") if args.len() >= 3 => {
            let lots = args.iter().position(|a| a == "--lots").and_then(|i| args.get(i + 1)).map(PathBuf::from);
            shops::export(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?, lots.as_deref())
        }
        Some("shops") if args.len() >= 2 => shops::dump(&PathBuf::from(&args[1])),
        Some("gta-cargens") if args.len() >= 3 => paths::export_cargens(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?, "bp_"),
        Some("gta-props") if args.len() >= 3 => props::export_gta(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?, args.iter().position(|a| a == "--col").and_then(|i| args.get(i + 1)).map(PathBuf::from).as_deref()),
        Some("props") if args.len() >= 3 => props::export_csv(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?),
        Some("gta-minimap") if args.len() >= 3 => {
            let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(PathBuf::from);
            minimap::export(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?, flag("--ymt").as_deref(), flag("--water").as_deref(), flag("--names").as_deref())
        }
        Some("gta-navmesh") if args.len() >= 3 => navmesh::export(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?),
        Some("traffic-json") if args.len() >= 3 => paths::export_json(&PathBuf::from(&args[1]), &PathBuf::from(&args[2])),
        Some("traffic") if args.len() >= 3 => paths::dump(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?),
        Some("water-meshes") if args.len() >= 3 => {
            let mut placement = gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
            if let Some(i) = args.iter().position(|a| a == "--offset") {
                let v: Vec<f32> = args.get(i + 1).context("--offset needs x,y,z")?.split(',').map(|s| s.trim().parse()).collect::<Result<_, _>>()?;
                anyhow::ensure!(v.len() == 3, "--offset takes x,y,z");
                placement.offset = [v[0], v[1], v[2]];
            }
            gtavis::water_meshes(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), placement)
        }
        Some("gta-water") if args.len() >= 4 => {
            let mut placement = gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
            if let Some(i) = args.iter().position(|a| a == "--offset") {
                let v: Vec<f32> = args.get(i + 1).context("--offset needs x,y,z")?.split(',').map(|s| s.trim().parse()).collect::<Result<_, _>>()?;
                anyhow::ensure!(v.len() == 3, "--offset takes x,y,z");
                placement.offset = [v[0], v[1], v[2]];
            }
            let lakes = args.iter().position(|a| a == "--lakes").map(|i| args.get(i + 1).map(PathBuf::from).context("--lakes needs a file")).transpose()?;
            water::generate(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3]), placement, lakes.as_deref())
        }
        Some("gta-test-platform") if args.len() >= 6 => {
            let f = |i: usize| args[i].parse::<f32>();
            let size = args.get(6).map_or(Ok(100.0), |s| s.parse())?;
            gta::export_test_platform(&PathBuf::from(&args[1]), &args[2], [f(3)?, f(4)?, f(5)?], size)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(())
        }
    }
}

/// bpx gta-collision <col> <out-dir> [--offset x,y,z] [--scale s] [--heading deg] [--walls skip|vehicles|solid] [--prefix p]
fn gta_collision(args: &[String]) -> Result<()> {
    let mut placement = gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
    let mut walls = gta::Walls::Vehicles;
    let mut prefix = "bp_".to_string();
    let mut i = 2;
    while i < args.len() {
        let val = || args.get(i + 1).with_context(|| format!("{} needs a value", args[i]));
        match args[i].as_str() {
            "--offset" => {
                let v: Vec<f32> = val()?.split(',').map(|s| s.trim().parse()).collect::<Result<_, _>>()?;
                anyhow::ensure!(v.len() == 3, "--offset takes x,y,z");
                placement.offset = [v[0], v[1], v[2]];
                i += 1;
            }
            "--scale" => {
                placement.scale = val()?.parse()?;
                i += 1;
            }
            "--heading" => {
                placement.heading = val()?.parse()?;
                i += 1;
            }
            "--prefix" => {
                prefix = val()?.clone();
                i += 1;
            }
            "--walls" => {
                walls = match val()?.as_str() {
                    "skip" => gta::Walls::Skip,
                    "vehicles" => gta::Walls::Vehicles,
                    "solid" => gta::Walls::Solid,
                    w => anyhow::bail!("--walls {w}: expected skip, vehicles or solid"),
                };
                i += 1;
            }
            a => anyhow::bail!("unknown option {a}"),
        }
        i += 1;
    }
    let opts = gta::Options { placement, invisible_walls: walls, prefix };
    gta::export_collision(&PathBuf::from(&args[0]), &PathBuf::from(&args[1]), &opts)
}

fn export(args: &[String], overview: bool) -> Result<()> {
    let game = PathBuf::from(&args[0]);
    let out = PathBuf::from(&args[1]);
    let units: Vec<u32> = if args[2] == "all" {
        let mut v: Vec<u32> = std::fs::read_dir(&game)?
            .filter_map(|e| {
                let n = e.ok()?.file_name().into_string().ok()?;
                n.strip_prefix("TRK_UNIT")?.strip_suffix("_GR.BNDL")?.parse().ok()
            })
            .collect();
        v.sort();
        v
    } else {
        args[2..].iter().map(|s| s.parse()).collect::<Result<_, _>>()?
    };
    let textures = export::TextureIndex::load(&game)?;
    let mut ex = export::Exporter::new(game, out, &textures, overview);
    for u in units {
        if let Err(e) = ex.export_unit(u) {
            eprintln!("unit {u}: {e:#}");
        }
    }
    Ok(())
}

/// Hex-dumps the first resources of a given type, with their import tables.
fn dump(args: &[String]) -> Result<()> {
    let b = bnd2::Bundle::open(&args[0])?;
    let ty = u32::from_str_radix(args[1].trim_start_matches("0x"), 16)?;
    let count: usize = args.get(2).map_or(Ok(1), |s| s.parse())?;
    let bytes: usize = args.get(3).map_or(Ok(256), |s| s.parse())?;
    for e in b.entries.iter().filter(|e| e.type_id == ty).take(count) {
        let main = b.chunk(e, 0)?;
        println!(
            "== {:016X} {} sizes {:?} import_offset {:#x}",
            e.id,
            b.name_of(e.id).unwrap_or(""),
            e.uncompressed_size,
            e.import_offset
        );
        for (id, off) in b.imports(e, &main) {
            let ty = b.find(id).map(|t| format!("{:#x}", t.type_id)).unwrap_or("external".into());
            println!("   import @{off:#06x} -> {id:016X} ({ty}) {}", b.name_of(id).unwrap_or(""));
        }
        for (i, row) in main[..main.len().min(bytes)].chunks(16).enumerate() {
            let hex: Vec<String> = row.iter().map(|x| format!("{x:02x}")).collect();
            let floats: Vec<String> = row
                .chunks(4)
                .filter(|c| c.len() == 4)
                .map(|c| format!("{:>10.3}", f32::from_le_bytes(c.try_into().unwrap())))
                .collect();
            println!("   {:04x}: {}  | {}", i * 16, hex.join(" "), floats.join(" "));
        }
    }
    Ok(())
}

/// `bpx inst-dump <game-dir>`: per unit, the instance list's array size and
/// complete count, and the entries past the count (model, backdrop zone,
/// position) with the backdrop models' names.
fn inst_dump(game: &std::path::Path) -> Result<()> {
    bnd2::set_fallback_names(bnd2::debug_names(game));
    let bd = bnd2::Bundle::open(game.join("GLOBALBACKDROPS.BNDL"))?;
    let mut zone_hist = std::collections::BTreeMap::new();
    for unit in 0..430 {
        let Ok(b) = bnd2::Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL"))) else { continue };
        let Some(e) = b.entries.iter().find(|e| e.type_id == bpworld::types::INSTANCE_LIST) else { continue };
        let r = bpworld::Res::load(&b, e)?;
        let m = &r.main;
        let (arr, size, n) = (bnd2::u32_at(m, 0) as usize, bnd2::u32_at(m, 4) as usize, bnd2::u32_at(m, 8) as usize);
        let mut extra = Vec::new();
        for i in 0..size {
            let o = arr + i * 0x50;
            if o + 0x50 > m.len() {
                break;
            }
            let zone = bnd2::u16_at(m, o + 4) as i16;
            *zone_hist.entry((i >= n, zone == -1)).or_insert(0) += 1;
            if i >= n {
                let model = r.import(o);
                let name = model.and_then(|id| bd.name_of(id).or_else(|| b.name_of(id))).map(crate::export::short_name).unwrap_or_default();
                extra.push(format!("{name}(zone {zone}, pos {:.0},{:.0},{:.0})", bnd2::f32_at(m, o + 0x40), bnd2::f32_at(m, o + 0x44), bnd2::f32_at(m, o + 0x48)));
            }
        }
        if !extra.is_empty() {
            println!("unit {unit}: {size} entries, {n} complete; extra: {}", extra.join("; "));
        }
    }
    println!("(past count, zone -1) histogram: {zone_hist:?}");
    Ok(())
}

/// `bpx pvs-dump <game-dir>`: PVS.BNDL's zones: id and corner points (x, z).
fn pvs_dump(game: &std::path::Path) -> Result<()> {
    let b = bnd2::Bundle::open(game.join("PVS.BNDL"))?;
    let e = b.entries.first().context("empty PVS.BNDL")?;
    let r = bpworld::Res::load(&b, e)?;
    let m = &r.main;
    let u32_at = |o: usize| bnd2::u32_at(m, o);
    let (zones, n) = (u32_at(4) as usize, u32_at(0x10) as usize);
    println!("zone,id,cx,cz,points");
    for i in 0..n {
        let z = zones + i * 0x30;
        let pts = u32_at(z) as usize;
        let id = bnd2::u64_at(m, z + 0x10);
        let np = bnd2::u16_at(m, z + 0x1A) as usize;
        // Vector2 assumed 8 bytes; also try 16.
        let p: Vec<(f32, f32)> = (0..np).map(|k| (bnd2::f32_at(m, pts + k * 16), bnd2::f32_at(m, pts + k * 16 + 4))).collect();
        let (cx, cz) = (p.iter().map(|q| q.0).sum::<f32>() / np.max(1) as f32, p.iter().map(|q| q.1).sum::<f32>() / np.max(1) as f32);
        println!("{i},{id},{cx:.1},{cz:.1},{:?}", p.iter().map(|q| (q.0.round(), q.1.round())).collect::<Vec<_>>());
    }
    Ok(())
}

/// `bpx backdrop-dump <game-dir>`: GLOBALBACKDROPS.BNDL's models: name,
/// LOD count and distances, bounding box and triangles of the top LOD.
fn backdrop_dump(game: &std::path::Path) -> Result<()> {
    bnd2::set_fallback_names(bnd2::debug_names(game));
    let b = bnd2::Bundle::open(game.join("GLOBALBACKDROPS.BNDL"))?;
    let mut rows = Vec::new();
    for e in b.entries.iter().filter(|e| e.type_id == bpworld::types::MODEL) {
        let m = bpworld::parse_model(&bpworld::Res::load(&b, e)?);
        let name = b.name_of(e.id).map(crate::export::short_name).unwrap_or_default();
        let Some(rid) = m.lod0() else { continue };
        let Some(re) = b.find(rid) else { continue };
        let geoms = bpworld::decode_renderable(&b, re).unwrap_or_default();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        let mut tris = 0;
        for g in &geoms {
            tris += g.indices.len() / 3;
            for p in &g.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        rows.push(format!("{name},{},{:?},{tris},{:.0},{:.0},{:.0},{:.0},{:.0},{:.0}", m.renderables.len(), m.lod_distances, lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]));
    }
    let tex_bytes: usize = b.entries.iter().filter(|e| e.type_id == bpworld::types::TEXTURE).map(|e| e.uncompressed_size[1] as usize).sum();
    eprintln!("backdrop textures: {} totalling {:.1} MB", b.entries.iter().filter(|e| e.type_id == bpworld::types::TEXTURE).count(), tex_bytes as f64 / 1048576.0);
    rows.sort();
    println!("name,renderables,lod_distances,tris,minx,miny,minz,maxx,maxy,maxz");
    for r in rows {
        println!("{r}");
    }
    Ok(())
}

/// `bpx vd-dump <game-dir> <unit> <material id hex>`: vertex layouts of the
/// meshes using a material, with the UV ranges of each texture coordinate set.
fn vd_dump(game: &std::path::Path, unit: &str, mat: &str) -> Result<()> {
    let b = bnd2::Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
    let mat = u64::from_str_radix(mat.trim_start_matches("0x"), 16)?;
    for e in b.entries.iter().filter(|e| e.type_id == bpworld::types::RENDERABLE) {
        let r = bpworld::Res::load(&b, e)?;
        let Ok(rend) = bpworld::parse_renderable(&r) else { continue };
        for m in rend.meshes.iter().filter(|m| m.material == Some(mat)) {
            let Some(vd) = m.vertex_descriptors.first().and_then(|id| b.find(*id)) else { continue };
            let elems = bpworld::parse_vertex_descriptor(&bpworld::Res::load(&b, vd)?);
            println!("renderable {:x}: {:?}", e.id, elems.iter().map(|el| format!("{:?}@{} fmt {}", el.usage, el.offset, el.format)).collect::<Vec<_>>());
        }
        if let Ok(geoms) = bpworld::decode_renderable(&b, e) {
            for g in geoms.iter().filter(|g| g.material == Some(mat)) {
                let range = |v: &[[f32; 2]]| v.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, t| [a[0].min(t[0]), a[1].min(t[1]), a[2].max(t[0]), a[3].max(t[1])]);
                println!("   {} verts; uv0 {:?}; uv1 {:?}", g.positions.len(), range(&g.uvs), (!g.uvs1.is_empty()).then(|| range(&g.uvs1)));
            }
        }
    }
    Ok(())
}

/// `bpx mat-dump <game-dir> <unit> [material id hex]`: a unit's materials:
/// shader, and per sampler its role hash, texture state and texture.
fn mat_dump(game: &std::path::Path, unit: &str, only: Option<&str>) -> Result<()> {
    bnd2::set_fallback_names(bnd2::debug_names(game));
    let file = if unit.contains('.') { unit.to_string() } else { format!("TRK_UNIT{unit}_GR.BNDL") };
    let b = bnd2::Bundle::open(game.join(file))?;
    let only = only.map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16)).transpose()?;
    for e in b.entries.iter().filter(|e| e.type_id == bpworld::types::MATERIAL) {
        if only.is_some_and(|o| o != e.id) {
            continue;
        }
        let r = bpworld::Res::load(&b, e)?;
        let shader = bpworld::material_shader(&r);
        println!("material {:x} {} shader {:x?} {}", e.id, b.name_of(e.id).unwrap_or("?"), shader, shader.and_then(|s| b.name_of(s)).unwrap_or("?"));
        println!("   main {} bytes, imports {:?}", r.main.len(), r.import_list.iter().map(|(id, off)| format!("{off:x}->{id:x}")).collect::<Vec<_>>());
        if std::env::var_os("BPX_DUMP_RAW").is_some() {
            let (n, base) = (r.main[9] as usize, bnd2::u32_at(&r.main, 0xC) as usize);
            for i in 0..n {
                let rec = &r.main[base + i * 0x14..base + i * 0x14 + 0x14];
                println!("   raw sampler {i}: {}", rec.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
            }
            println!("   head {}", r.main[..0x40.min(r.main.len())].iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
        }
        for (h, ts) in bpworld::material_samplers(&r) {
            let tex = ts.and_then(|id| b.find(id)).and_then(|te| bpworld::Res::load(&b, te).ok()).and_then(|tr| bpworld::texture_state_texture(&tr));
            println!("   sampler {h:08x} state {ts:x?} texture {tex:x?} {}", tex.and_then(|t| b.name_of(t)).unwrap_or("?"));
        }
    }
    Ok(())
}

fn info(paths: &[String]) -> Result<()> {
    for p in paths {
        let b = bnd2::Bundle::open(p)?;
        println!("{p}: {} resources, flags {:#x}", b.entries.len(), b.flags);
        let mut by_type: BTreeMap<u32, (usize, String)> = BTreeMap::new();
        for e in &b.entries {
            let t = by_type.entry(e.type_id).or_default();
            t.0 += 1;
            if let Some(d) = b.debug.get(&e.id) {
                t.1 = d.type_name.clone();
            }
        }
        for (ty, (n, name)) in &by_type {
            println!("  type {ty:#07x} x{n:<5} {name}");
        }
        for e in b.entries.iter().take(8) {
            let main = b.chunk(e, 0).context("chunk")?;
            println!(
                "  {:016X} type {:#07x} sizes {:?} imports {} {}",
                e.id,
                e.type_id,
                e.uncompressed_size,
                b.imports(e, &main).len(),
                b.name_of(e.id).unwrap_or("")
            );
        }
    }
    Ok(())
}

/// bpx gta-visual <game> <assets-out> <meta-out> [--units ..] [placement opts] [--lod-dist m] [--lod-far m|off] [--prefix p]
/// (`--lod-far off`: no backdrops, i.e. no distant scenery or stand-ins)
fn gta_visual(args: &[String]) -> Result<()> {
    let game = PathBuf::from(&args[0]);
    let mut placement = gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
    let mut lod_dist = 1500.0;
    let mut lod_far: Option<f32> = Some(15000.0);
    let mut prefix = "bp_".to_string();
    let mut units: Option<Vec<u32>> = None;
    let mut i = 3;
    while i < args.len() {
        let val = || args.get(i + 1).with_context(|| format!("{} needs a value", args[i]));
        match args[i].as_str() {
            "--offset" => {
                let v: Vec<f32> = val()?.split(',').map(|s| s.trim().parse()).collect::<Result<_, _>>()?;
                anyhow::ensure!(v.len() == 3, "--offset takes x,y,z");
                placement.offset = [v[0], v[1], v[2]];
            }
            "--scale" => placement.scale = val()?.parse()?,
            "--heading" => placement.heading = val()?.parse()?,
            "--lod-dist" => lod_dist = val()?.parse()?,
            "--lod-far" => lod_far = if val()? == "off" { None } else { Some(val()?.parse()?) },
            "--prefix" => prefix = val()?.clone(),
            "--units" => {
                let v = val()?;
                if v != "all" {
                    units = Some(v.split(',').map(|s| s.trim().parse()).collect::<Result<_, _>>()?);
                }
            }
            a => anyhow::bail!("unknown option {a}"),
        }
        i += 2;
    }
    let units = match units {
        Some(u) => u,
        None => {
            let mut v: Vec<u32> = std::fs::read_dir(&game)?
                .filter_map(|e| {
                    let n = e.ok()?.file_name().into_string().ok()?;
                    n.strip_prefix("TRK_UNIT")?.strip_suffix("_GR.BNDL")?.parse().ok()
                })
                .collect();
            v.sort();
            v
        }
    };
    let opts = gtavis::Options { placement, lod_dist, lods: lod_far.is_some(), lod_far: lod_far.unwrap_or(0.0), prefix };
    gtavis::export_visual(&game, &PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &units, &opts)
}

/// `--offset x,y,z` placement shared by the commands that only take an offset.
fn parse_placement(args: &[String]) -> Result<gta::Placement> {
    let mut placement = gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
    if let Some(i) = args.iter().position(|a| a == "--offset") {
        let v: Vec<f32> = args.get(i + 1).context("--offset needs x,y,z")?.split(',').map(|s| s.trim().parse()).collect::<Result<_, _>>()?;
        anyhow::ensure!(v.len() == 3, "--offset takes x,y,z");
        placement.offset = [v[0], v[1], v[2]];
    }
    Ok(placement)
}
