//! Finding the player's GTA V (Legacy), Burnout Paradise Remastered and
//! CodeWalker folders: the one given or remembered, else the usual install
//! places (Steam libraries, Rockstar / Epic / EA defaults, the registry),
//! else the player is asked. Only reads; never changes anything.

use crate::ask;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// Every Steam library folder (from Steam's libraryfolders.vdf).
fn steam_libraries() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for steam in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        let Ok(text) = std::fs::read_to_string(Path::new(steam).join(r"steamapps\libraryfolders.vdf")) else { continue };
        out.push(PathBuf::from(steam));
        for line in text.lines() {
            let parts: Vec<&str> = line.split('"').collect();
            // 	"path"		"H:\\SteamLibrary"
            if parts.len() >= 4 && parts[1] == "path" {
                out.push(PathBuf::from(parts[3].replace(r"\\", r"\")));
            }
        }
    }
    out.dedup();
    out
}

/// A string value from the registry (`reg query`), if present.
fn registry(key: &str, value: &str) -> Option<PathBuf> {
    let out = crate::command("reg").args(["query", key, "/v", value]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.trim_start().starts_with(value))?;
    let path = line.split("REG_SZ").nth(1)?.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Asks for a folder until `ok` accepts it (or the player gives up).
fn ask_folder(what: &str, ok: &dyn Fn(&Path) -> Result<PathBuf>) -> Result<PathBuf> {
    loop {
        let typed = ask(&format!("Where is {what}? Paste its folder (or press Enter to stop): "));
        if typed.is_empty() {
            bail!("{what} wasn't found");
        }
        match ok(Path::new(&typed)) {
            Ok(p) => return Ok(p),
            Err(e) => println!("  {e}"),
        }
    }
}

/// Picks among `candidates` that pass `ok`: the given one if any, else the
/// only one found, else the player chooses / types one.
fn pick(what: &str, given: Option<PathBuf>, candidates: Vec<PathBuf>, ok: &dyn Fn(&Path) -> Result<PathBuf>) -> Result<PathBuf> {
    if let Some(g) = given {
        match ok(&g) {
            Ok(p) => return Ok(p),
            Err(e) => println!("{e}"),
        }
    }
    let mut found: Vec<PathBuf> = candidates.iter().filter_map(|c| ok(c).ok()).collect();
    found.sort();
    found.dedup();
    match found.len() {
        0 => {
            println!("{what} wasn't found in the usual places.");
            ask_folder(what, ok)
        }
        1 => Ok(found.remove(0)),
        _ => {
            println!("Found more than one {what}:");
            for (i, f) in found.iter().enumerate() {
                println!("  {}. {}", i + 1, f.display());
            }
            loop {
                let n = ask(&format!("Which one? [1-{}, or paste another folder] ", found.len()));
                if let Ok(i) = n.parse::<usize>() {
                    if (1..=found.len()).contains(&i) {
                        return Ok(found[i - 1].clone());
                    }
                }
                if !n.is_empty() {
                    match ok(Path::new(&n)) {
                        Ok(p) => return Ok(p),
                        Err(e) => println!("  {e}"),
                    }
                }
            }
        }
    }
}

/// GTA V Legacy: GTA5.exe, and never the Enhanced edition (GTA Online's).
pub fn check_gta(p: &Path) -> Result<PathBuf> {
    if p.join("GTA5_Enhanced.exe").is_file() {
        bail!("this is GTA V Enhanced; GTA Paradise only installs into GTA V Legacy");
    }
    if !p.join("GTA5.exe").is_file() {
        bail!("no GTA5.exe in this folder");
    }
    Ok(p.to_path_buf())
}

fn gta_candidates() -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = steam_libraries().into_iter().map(|l| l.join(r"steamapps\common\Grand Theft Auto V")).collect();
    let rockstar = r"HKLM\SOFTWARE\WOW6432Node\Rockstar Games\Grand Theft Auto V";
    c.extend(["InstallFolder", "InstallFolderSteam", "InstallFolderEpic"].iter().filter_map(|v| registry(rockstar, v)));
    c.push(PathBuf::from(r"C:\Program Files\Rockstar Games\Grand Theft Auto V"));
    c.push(PathBuf::from(r"C:\Program Files\Epic Games\GTAV"));
    c
}

pub fn gta(given: Option<PathBuf>) -> Result<PathBuf> {
    pick("GTA V (Legacy)", given, gta_candidates(), &check_gta)
}

/// Burnout Paradise Remastered (PC): BurnoutPR.exe and WORLDCOL.BIN.
pub fn check_burnout(p: &Path) -> Result<PathBuf> {
    if !p.join("BurnoutPR.exe").is_file() || !p.join("WORLDCOL.BIN").is_file() {
        bail!("not Burnout Paradise Remastered (no BurnoutPR.exe / WORLDCOL.BIN here)");
    }
    Ok(p.to_path_buf())
}

fn burnout_candidates() -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = steam_libraries().into_iter().map(|l| l.join(r"steamapps\common\Burnout Paradise Remastered")).collect();
    for base in [r"C:\Program Files\EA Games", r"C:\Program Files (x86)\Origin Games", r"C:\Program Files\Origin Games"] {
        c.push(Path::new(base).join("Burnout Paradise Remastered"));
    }
    c
}

pub fn burnout(given: Option<PathBuf>) -> Result<PathBuf> {
    pick("Burnout Paradise Remastered", given, burnout_candidates(), &check_burnout)
}

/// The first of `given` and the usual places that passes `ok`, without
/// asking (for the setup window, which lets the player change it).
fn detect(given: Option<PathBuf>, candidates: Vec<PathBuf>, ok: &dyn Fn(&Path) -> Result<PathBuf>) -> Option<PathBuf> {
    given.into_iter().chain(candidates).find_map(|c| ok(&c).ok())
}

pub fn detect_gta(given: Option<PathBuf>) -> Option<PathBuf> {
    detect(given, gta_candidates(), &check_gta)
}

pub fn detect_burnout(given: Option<PathBuf>) -> Option<PathBuf> {
    detect(given, burnout_candidates(), &check_burnout)
}

pub fn detect_codewalker(given: Option<PathBuf>) -> Option<PathBuf> {
    detect(given, codewalker_candidates(), &check_codewalker)
}

/// The folder holding CodeWalker.Core.dll under `p` (it, or up to two
/// levels down: the gta5-mods zip unpacks into a folder of its own).
fn codewalker_in(p: &Path, depth: u32) -> Option<PathBuf> {
    if p.join("CodeWalker.Core.dll").is_file() {
        return Some(p.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(p).ok()?.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).find_map(|e| codewalker_in(&e.path(), depth - 1))
}

/// CodeWalker 3.0 (downloaded by the player from gta5-mods.com).
pub fn check_codewalker(p: &Path) -> Result<PathBuf> {
    codewalker_in(p, 2).ok_or_else(|| anyhow::anyhow!("no CodeWalker.Core.dll in this folder"))
}

fn codewalker_candidates() -> Vec<PathBuf> {
    let mut c = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for place in ["Downloads", "Desktop", "Documents"] {
            let Ok(entries) = std::fs::read_dir(home.join(place)) else { continue };
            for e in entries.filter_map(|e| e.ok()) {
                if e.file_name().to_string_lossy().to_lowercase().contains("codewalker") && e.path().is_dir() {
                    c.push(e.path());
                }
            }
        }
    }
    c
}

pub const CODEWALKER_URL: &str = "https://www.gta5-mods.com/tools/codewalker-gtav-interactive-3d-map";
pub const SCRIPTHOOKV_URL: &str = "http://www.dev-c.com/gtav/scripthookv/";
pub const OPENIV_URL: &str = "https://openiv.com/";

pub fn codewalker(given: Option<PathBuf>) -> Result<PathBuf> {
    let r = pick("CodeWalker", given, codewalker_candidates(), &check_codewalker);
    if r.is_err() {
        println!("CodeWalker 3.0 is free: https://www.gta5-mods.com/tools/codewalker-gtav-interactive-3d-map");
        println!("Download it, unzip it anywhere, then run setup again.");
    }
    r
}

/// Are ScriptHookV (with its dinput8.dll ASI loader) and OpenIV.asi in the
/// GTA folder?
pub fn loaders(gta: &Path) -> (bool, bool) {
    (gta.join("ScriptHookV.dll").is_file() && gta.join("dinput8.dll").is_file(), gta.join("OpenIV.asi").is_file())
}

/// ScriptHookV and OpenIV.asi must be in the GTA folder for the mod to load.
pub fn check_loaders(gta: &Path) -> Result<()> {
    let mut missing = Vec::new();
    if !gta.join("ScriptHookV.dll").is_file() || !gta.join("dinput8.dll").is_file() {
        missing.push("ScriptHookV (ScriptHookV.dll and dinput8.dll): http://www.dev-c.com/gtav/scripthookv/");
    }
    if !gta.join("OpenIV.asi").is_file() {
        missing.push("OpenIV with its mods folder set up (OpenIV.asi; in OpenIV: Tools > ASI Manager > install OpenIV.ASI): https://openiv.com/");
    }
    if !missing.is_empty() {
        for m in &missing {
            println!("Missing in {}: {m}", gta.display());
        }
        bail!("install the above into GTA V first");
    }
    Ok(())
}
