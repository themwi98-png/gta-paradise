mod collision;
mod colsrc;
mod edits;
mod export;
mod gta;
mod gtadata;
mod gtavis;
mod navmesh;
mod paths;
mod props;
mod repair;
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
  bpx handling <handling_src-dir> <out-dir>   (Burnout-style crash damage, speed, grip)
  bpx strip-base-map <list.txt> <images.meta> <gta5.meta> <out-dir> | pack <content.xml> <out>
  bpx popzone <burnout-dir> <original popzone.ipl> <out popzone.ipl>
  bpx col-split <stock WORLDCOL.BIN> <combined collision> <out.pcx>
  bpx collision <game-dir|WORLDCOL file> <out-dir> <unit|all> [unit...] [--base <WORLDCOL>]
  bpx surfaces <game-dir> [unit-step]   (report what each collision surface id is)
  bpx materials <game-dir> [unit-step]  (report material texture-slot roles per shader)
  bpx gta-collision <game-dir|WORLDCOL file> <out-dir> [--offset x,y,z] [--scale s] [--heading deg] [--keep-walls] [--prefix p]
      (GTA V .ybn.xml per unit; convert with tools/cwconv)
  bpx gta-test-platform <out-dir> <name> <x> <y> <top-z> [size]   (flat test collision slab)
  bpx gta-visual <game-dir> <assets-out> <meta-out> [--units 1,2,..|all] [--offset x,y,z] [--scale s] [--heading deg] [--lod-dist m] [--prefix p]
      (GTA V .ydr/.ytd/.ytyp/.ymap XML per unit; convert with tools/cwconv)
  bpx gta-paths <game-dir> <out-dir> [--offset x,y,z] [--lanes]
      (GTA traffic paths, nodes0..1023.ynd.xml: two-way roads built from Burnout's
      traffic lanes; --lanes: one one-way road per lane, the earlier way)
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
      (adds sea quads around Paradise City / over replaced Los Santos)";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("info") => info(&args[1..]),
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
        Some("handling") if args.len() >= 3 => gtadata::handling(&PathBuf::from(&args[1]), &PathBuf::from(&args[2])),
        Some("strip-base-map") if args.len() >= 4 && args[1] == "pack" => gtadata::strip_pack(&PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("strip-base-map") if args.len() >= 5 => {
            gtadata::strip_base_map(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3]), &PathBuf::from(&args[4]))
        }
        Some("popzone") if args.len() >= 4 => gtadata::popzone(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
        Some("col-split") if args.len() >= 4 => colsrc::split(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), &PathBuf::from(&args[3])),
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
        Some("gta-cargens") if args.len() >= 3 => paths::export_cargens(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?, "bp_"),
        Some("gta-props") if args.len() >= 3 => props::export_gta(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?, args.iter().position(|a| a == "--col").and_then(|i| args.get(i + 1)).map(PathBuf::from).as_deref()),
        Some("props") if args.len() >= 3 => props::export_csv(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]), parse_placement(&args)?),
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

/// bpx gta-collision <col> <out-dir> [--offset x,y,z] [--scale s] [--heading deg] [--keep-walls] [--prefix p]
fn gta_collision(args: &[String]) -> Result<()> {
    let mut placement = gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
    let mut keep_walls = false;
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
            "--keep-walls" => keep_walls = true,
            a => anyhow::bail!("unknown option {a}"),
        }
        i += 1;
    }
    let opts = gta::Options { placement, keep_invisible_walls: keep_walls, prefix };
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

/// bpx gta-visual <game> <assets-out> <meta-out> [--units ..] [placement opts] [--lod-dist m] [--prefix p]
fn gta_visual(args: &[String]) -> Result<()> {
    let game = PathBuf::from(&args[0]);
    let mut placement = gta::Placement { offset: [0.0; 3], scale: 1.0, heading: 0.0 };
    let mut lod_dist = 1500.0;
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
    let opts = gtavis::Options { placement, lod_dist, prefix };
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
