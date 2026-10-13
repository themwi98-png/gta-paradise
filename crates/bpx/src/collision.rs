//! Exports world collision (WORLDCOL.BIN, one PolygonSoupList per track
//! unit) to glTF, one primitive per surface id so surfaces can be inspected
//! and later mapped to GTA V collision materials.

use crate::export::Gltf;
use anyhow::{Context, Result};
use bnd2::Bundle;
use bpworld::{self as bw, types, CollisionTag, Res};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// Distinct, stable colour per surface id for previews.
fn surface_colour(id: u8) -> [f32; 4] {
    let h = (id as f32 * 0.618_034) % 1.0;
    let (r, g, b) = hsv(h, 0.55, 0.85);
    [r, g, b, 1.0]
}

fn hsv(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Prints how often bit 0 ("always set") is set under each byte-order
/// interpretation of the tag, plus the most common raw values.
fn tag_debug(b: &Bundle) -> Result<()> {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    let mut total = 0usize;
    let mut rates = [0usize; 6];
    for e in b.entries.iter().filter(|e| e.type_id == types::POLYGON_SOUP_LIST) {
        for p in bw::parse_polygon_soup_list(&Res::load(b, e)?).polys {
            total += 1;
            *counts.entry(p.tag).or_default() += 1;
            let t = p.tag;
            let s = t.swap_bytes();
            let halves_swapped = ((t & 0xFFFF) as u16).swap_bytes() as u32 | ((((t >> 16) as u16).swap_bytes() as u32) << 16);
            for (k, v) in [t, t >> 16, s, s >> 16, halves_swapped, halves_swapped >> 16].iter().enumerate() {
                rates[k] += (v & 1) as usize;
            }
        }
    }
    let names = ["le lo", "le hi", "be lo", "be hi", "per-half-swap lo", "per-half-swap hi"];
    for (n, r) in names.iter().zip(rates) {
        println!("  bit0 {n:<18} {:.1}%", r as f64 * 100.0 / total as f64);
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (tag, n) in v.iter().take(12) {
        println!("  {tag:08X} x{n}");
    }
    Ok(())
}

fn apply(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|k| p[0] * m[k] + p[1] * m[4 + k] + p[2] * m[8 + k] + m[12 + k])
}

/// Infers what each surface id means by sampling collision polygons and
/// tallying the names of the nearest visual materials, plus flags and how
/// vertical the surfaces are. Prints a report; `step` samples every Nth unit.
pub fn analyse_surfaces(game: &Path, step: usize) -> Result<()> {
    let col = Bundle::open(game.join("WORLDCOL.BIN"))?;
    // surface -> (material name -> hits), flags, poly count, sum |normal.y|
    let mut names: BTreeMap<u8, HashMap<String, usize>> = BTreeMap::new();
    let mut flags: BTreeMap<u8, BTreeMap<u16, usize>> = BTreeMap::new();
    let mut upness: BTreeMap<u8, (usize, f64)> = BTreeMap::new();
    const CELL: f32 = 2.0;
    let cell = |p: [f32; 3]| [0, 1, 2].map(|k| (p[k] / CELL).floor() as i32);

    let mut units: Vec<(u32, &bnd2::Entry)> = col
        .entries
        .iter()
        .filter(|e| e.type_id == types::POLYGON_SOUP_LIST)
        .filter_map(|e| Some((col.name_of(e.id)?.strip_prefix("TRK_COL_")?.parse().ok()?, e)))
        .collect();
    units.sort_by_key(|u| u.0);

    for (unit, e) in units.into_iter().step_by(step.max(1)) {
        let mesh = bw::parse_polygon_soup_list(&Res::load(&col, e)?);
        if mesh.polys.is_empty() {
            continue;
        }
        let gfx = Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
        let Some(list) = gfx.entries.iter().find(|e| e.type_id == types::INSTANCE_LIST) else { continue };

        // Visual triangle centroids bucketed into a grid, tagged with material.
        let mut mat_names: Vec<String> = Vec::new();
        let mut grid: HashMap<[i32; 3], Vec<([f32; 3], u32)>> = HashMap::new();
        let mut geo_cache: HashMap<u64, Vec<bw::MeshGeometry>> = HashMap::new();
        for inst in bw::parse_instance_list(&Res::load(&gfx, list)?) {
            if !geo_cache.contains_key(&inst.model) {
                let geo = (|| -> Option<Vec<bw::MeshGeometry>> {
                    let m = bw::parse_model(&Res::load(&gfx, gfx.find(inst.model)?).ok()?);
                    bw::decode_renderable(&gfx, gfx.find(m.lod0()?)?).ok()
                })()
                .unwrap_or_default();
                geo_cache.insert(inst.model, geo);
            }
            for g in &geo_cache[&inst.model] {
                let name = g.material.and_then(|m| gfx.name_of(m)).map(crate::export::short_name).unwrap_or_default();
                let mi = mat_names.iter().position(|n| *n == name).unwrap_or_else(|| {
                    mat_names.push(name);
                    mat_names.len() - 1
                }) as u32;
                for t in g.indices.chunks(3) {
                    let c = t.iter().fold([0.0; 3], |a, &i| {
                        let p = apply(&inst.transform, g.positions[i as usize]);
                        [a[0] + p[0] / 3.0, a[1] + p[1] / 3.0, a[2] + p[2] / 3.0]
                    });
                    grid.entry(cell(c)).or_default().push((c, mi));
                }
            }
        }

        for p in mesh.polys.iter().step_by(7) {
            let tag = CollisionTag::decode(p.tag);
            *flags.entry(tag.surface_id).or_default().entry(tag.flags).or_default() += 1;
            let (a, b, c) = (p.verts[0], p.verts[1], p.verts[2]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            let up = upness.entry(tag.surface_id).or_default();
            up.0 += 1;
            up.1 += (n[1] / len).abs() as f64;

            let k = p.verts.len() as f32;
            let centre = p.verts.iter().fold([0.0; 3], |s, q| [s[0] + q[0] / k, s[1] + q[1] / k, s[2] + q[2] / k]);
            let cc = cell(centre);
            let mut best: Option<(f32, u32)> = None;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let Some(bucket) = grid.get(&[cc[0] + dx, cc[1] + dy, cc[2] + dz]) else { continue };
                        for (q, mi) in bucket {
                            let d = (0..3).map(|i| (q[i] - centre[i]).powi(2)).sum::<f32>();
                            if best.is_none_or(|b| d < b.0) {
                                best = Some((d, *mi));
                            }
                        }
                    }
                }
            }
            if let Some((_, mi)) = best {
                *names.entry(tag.surface_id).or_default().entry(mat_names[mi as usize].clone()).or_default() += 1;
            }
        }
    }

    for (sid, (n, up)) in &upness {
        let mut top: Vec<_> = names.get(sid).map(|m| m.iter().collect()).unwrap_or_default();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let top: Vec<String> = top.iter().take(6).map(|(k, v)| format!("{k}({v})")).collect();
        println!(
            "surface {sid:>2}: {n:>6} samples, up {:.2}, flags {:x?}\n    {}",
            up / *n as f64,
            flags[sid],
            top.join(", ")
        );
    }
    Ok(())
}

/// Identity of a polygon for diffing: tag plus vertices rounded to 1 mm.
fn poly_key(p: &bw::CollisionPoly) -> Vec<i32> {
    let mut k = vec![p.tag as i32];
    k.extend(p.verts.iter().flat_map(|v| v.map(|c| (c * 1000.0).round() as i32)));
    k
}

fn load_poly_keys(path: &Path) -> Result<HashMap<u32, HashSet<Vec<i32>>>> {
    let mut out: HashMap<u32, HashSet<Vec<i32>>> = HashMap::new();
    for (unit, polys) in crate::colsrc::load(path)? {
        out.entry(unit).or_default().extend(polys.iter().map(poly_key));
    }
    Ok(out)
}

/// `col` is a collision bundle (WORLDCOL.BIN or a modded/extra file) or a
/// game directory, in which case its WORLDCOL.BIN is used.
///
/// With `base` (e.g. the stock WORLDCOL.BIN), polygons not present in the
/// base are put in separate `gen_*` materials, highlighted magenta, so added
/// or auto-generated collision can be reviewed on its own.
pub fn export(col: &Path, out: &Path, units: Option<&[u32]>, base: Option<&Path>) -> Result<()> {
    let all_units = crate::colsrc::load(col)?;
    let base_polys = base.map(load_poly_keys).transpose()?;
    let mut generated = 0usize;
    let mut surface_totals: BTreeMap<u8, usize> = BTreeMap::new();
    let mut flag_totals: BTreeMap<u16, usize> = BTreeMap::new();
    let mut always_set = 0usize;
    let mut total_polys = 0usize;
    let mut out_of_box = 0usize;

    for (unit, polys) in all_units {
        let name = format!("TRK_COL_{unit}");
        let name = name.as_str();
        if units.is_some_and(|u| !u.contains(&unit)) {
            continue;
        }
        if polys.is_empty() {
            continue;
        }
        let mut mesh = bw::CollisionMesh { aabb_min: [f32::MAX; 3], aabb_max: [f32::MIN; 3], polys };
        for v in mesh.polys.iter().flat_map(|p| &p.verts) {
            for k in 0..3 {
                mesh.aabb_min[k] = mesh.aabb_min[k].min(v[k]);
                mesh.aabb_max[k] = mesh.aabb_max[k].max(v[k]);
            }
        }

        // Sanity checks: decoded vertices should sit inside the stored AABB,
        // and both tag halves have an "always set" top bit.
        let eps = 1.0;
        for p in &mesh.polys {
            always_set += (p.tag & 0x8000_8000 == 0x8000_8000) as usize;
            *flag_totals.entry(CollisionTag::decode(p.tag).flags).or_default() += 1;
            for v in &p.verts {
                if (0..3).any(|k| v[k] < mesh.aabb_min[k] - eps || v[k] > mesh.aabb_max[k] + eps) {
                    out_of_box += 1;
                }
            }
        }
        total_polys += mesh.polys.len();

        // Group triangles by (surface id, generated?).
        let empty = HashSet::new();
        let unit_base = base_polys.as_ref().map(|m| m.get(&unit).unwrap_or(&empty));
        // Per triangle, the polygon's index in its unit (glTF primitive
        // extras "polygonIds"): the stable id tools/viewer's collision editor
        // uses for deletions, which the GTA build then skips.
        let mut by_surface: BTreeMap<(u8, bool), (Vec<[f32; 3]>, Vec<u32>, Vec<u32>)> = BTreeMap::new();
        for (poly_index, p) in mesh.polys.iter().enumerate() {
            let tag = CollisionTag::decode(p.tag);
            *surface_totals.entry(tag.surface_id).or_default() += 1;
            let is_gen = unit_base.is_some_and(|s| !s.contains(&poly_key(p)));
            generated += is_gen as usize;
            let (pos, idx, ids) = by_surface.entry((tag.surface_id, is_gen)).or_default();
            let base = pos.len() as u32;
            pos.extend_from_slice(&p.verts);
            idx.extend([base, base + 1, base + 2]);
            ids.push(poly_index as u32);
            if p.verts.len() == 4 {
                idx.extend([base, base + 2, base + 3]);
                ids.push(poly_index as u32);
            }
        }

        let mut g = Gltf::default();
        let mut prims = Vec::new();
        for (&(sid, is_gen), (pos, idx, ids)) in &by_surface {
            // Material names encode the surface id ("<id>_<name>", or
            // "gen_<id>_<name>"), which the collision importer reads back.
            let (name, colour) = if is_gen {
                (format!("gen_{sid}_{}", bw::surface_name(sid)), [0.95, 0.1, 0.85, 1.0])
            } else {
                (format!("{sid}_{}", bw::surface_name(sid)), surface_colour(sid))
            };
            g.materials.push(json!({
                "name": name,
                "doubleSided": true,
                "pbrMetallicRoughness": {"baseColorFactor": colour, "metallicFactor": 0.0, "roughnessFactor": 1.0}
            }));
            let mat = g.materials.len() - 1;
            let p = g.push_vec3(pos);
            let i = g.push_indices(idx);
            prims.push(json!({"attributes": {"POSITION": p}, "indices": i, "material": mat, "extras": {"polygonIds": ids}}));
        }
        g.meshes.push(json!({"name": name, "primitives": prims}));
        g.nodes.push(json!({"name": name, "mesh": 0}));

        let dir = out.join(format!("unit{unit}"));
        std::fs::create_dir_all(&dir)?;
        let stem = format!("unit{unit}");
        std::fs::write(dir.join(format!("{stem}.bin")), &g.bin)?;
        let doc = json!({
            "asset": {"version": "2.0", "generator": "gta-paradise bpx"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": g.nodes,
            "meshes": g.meshes,
            "materials": g.materials,
            "accessors": g.accessors,
            "bufferViews": g.buffer_views,
            "buffers": [{"uri": format!("{stem}.bin"), "byteLength": g.bin.len()}],
        });
        std::fs::write(dir.join(format!("{stem}.gltf")), serde_json::to_vec(&doc)?).context("writing gltf")?;
    }

    println!("{total_polys} collision polys; {out_of_box} vertices outside their unit AABB");
    println!("tags with both 'always set' bits: {always_set}/{total_polys}");
    if base_polys.is_some() {
        println!("{generated} polys not in base collision (exported as gen_* materials)");
    }
    println!("flags (count): {flag_totals:x?}");
    if std::env::var_os("BPX_TAG_DEBUG").is_some() {
        let col = if col.is_dir() { col.join("WORLDCOL.BIN") } else { col.to_path_buf() };
        tag_debug(&Bundle::open(&col)?)?;
    }
    println!("surface ids (count):");
    for (sid, n) in surface_totals {
        println!("  {sid:>2} {:<16} -> {:<22} {n}", bw::surface_name(sid), bw::surface_gta_material(sid));
    }
    Ok(())
}

/// Report of material texture-slot roles across the map: for each Burnout
/// shader, which slot hashes occur and example texture names in each, so the
/// diffuse slot can be identified from data rather than texture names.
pub fn analyse_materials(game: &Path, step: usize) -> Result<()> {
    let textures = crate::export::TextureIndex::load(game)?;
    // shader -> slot hash -> (count, examples)
    let mut table: BTreeMap<String, BTreeMap<u32, (usize, Vec<String>)>> = BTreeMap::new();
    let mut shader_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut units: Vec<u32> = std::fs::read_dir(game)?
        .filter_map(|e| e.ok()?.file_name().into_string().ok()?.strip_prefix("TRK_UNIT")?.strip_suffix("_GR.BNDL")?.parse().ok())
        .collect();
    units.sort();
    for unit in units.into_iter().step_by(step.max(1)) {
        let b = Bundle::open(game.join(format!("TRK_UNIT{unit}_GR.BNDL")))?;
        for e in b.entries.iter().filter(|e| e.type_id == types::MATERIAL) {
            let Ok(r) = Res::load(&b, e) else { continue };
            let shader = bw::material_shader(&r)
                .and_then(|id| b.name_of(id))
                .map(|n| n.rsplit('/').next().unwrap_or(n).split(".fx").next().unwrap_or(n).to_string())
                .unwrap_or_else(|| "?".into());
            *shader_counts.entry(shader.clone()).or_default() += 1;
            for (hash, ts) in bw::material_samplers(&r) {
                let tex = ts
                    .and_then(|id| b.find(id))
                    .and_then(|te| Res::load(&b, te).ok())
                    .and_then(|tr| bw::texture_state_texture(&tr))
                    .and_then(|tid| textures.name(tid))
                    .unwrap_or_default();
                let slot = table.entry(shader.clone()).or_default().entry(hash).or_default();
                slot.0 += 1;
                if slot.1.len() < 4 && !tex.is_empty() && !slot.1.contains(&tex) {
                    slot.1.push(tex);
                }
            }
        }
    }
    for (shader, slots) in &table {
        println!("{shader} ({} materials)", shader_counts[shader]);
        for (hash, (n, ex)) in slots {
            println!("    {hash:08X} x{n:<5} {}", ex.join(", "));
        }
    }
    Ok(())
}
