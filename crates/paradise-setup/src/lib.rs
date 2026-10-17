//! GTA Paradise setup: builds the mod from the player's own copies of
//! Burnout Paradise Remastered and GTA V (Legacy) and installs it.
//!
//! Nothing from either game ships with the mod. Every step converts the
//! player's files, the same way the development scripts in tools/ do (the
//! same steps, in the same order). Steps whose inputs haven't changed since
//! the last run are skipped, so an update only redoes what it has to.
//!
//! Two front ends share this: `paradise-setup` (the setup window players
//! use) and `paradise-setup-cli` (the same in a console, for testing).
//!
//! Release layout, next to the setup exe:
//!   tools/bpx.exe, tools/cwconv.exe   the converters
//!   mod/paradise.asi                  the in-game plugin
//!   data/tools/...                    the mod's own data (hand edits,
//!                                     added collision, prop table, settings)
//! Working files go to %LOCALAPPDATA%\GTA-Paradise\work.

pub mod find;
pub mod steps;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Bump to make every player's next run rebuild everything.
pub const SETUP_VERSION: &str = "1";

/// Where everything is.
pub struct Layout {
    /// Working folder (like the project folder: tools/..., out/...).
    pub root: PathBuf,
    /// The mod's own data (a tools/ folder), copied into `root` first.
    pub data: Option<PathBuf>,
    pub bpx: PathBuf,
    pub cwconv: PathBuf,
    pub asi: PathBuf,
}

/// The three folders on the player's PC.
#[derive(Clone)]
pub struct Games {
    pub gta: PathBuf,
    pub burnout: PathBuf,
    pub codewalker: PathBuf,
}

impl Layout {
    /// `dev`: run from the project folder (tools built locally, working
    /// files in out/); otherwise from the release folder.
    pub fn new(dev: bool) -> Result<Layout> {
        let exe_dir = std::env::current_exe()?.parent().context("no exe folder")?.to_path_buf();
        if dev {
            // target/release/paradise-setup.exe -> the project folder.
            let root = exe_dir
                .ancestors()
                .find(|d| d.join("Cargo.toml").is_file() && d.join("tools").is_dir())
                .context("--dev: project folder not found")?;
            return Ok(Layout {
                root: root.to_path_buf(),
                data: None,
                bpx: root.join("target/release/bpx.exe"),
                cwconv: root.join("tools/cwconv/bin/Release/net8.0-windows/cwconv.exe"),
                asi: root.join("target/release/paradise_asi.dll"),
            });
        }
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).context("LOCALAPPDATA not set")?;
        Ok(Layout {
            root: local.join("GTA-Paradise").join("work"),
            data: Some(exe_dir.join("data")),
            bpx: exe_dir.join("tools/bpx.exe"),
            cwconv: exe_dir.join("tools/cwconv.exe"),
            asi: exe_dir.join("mod/paradise.asi"),
        })
    }

    /// Are the converters and plugin there (is the mod folder complete)?
    pub fn check(&self) -> Result<()> {
        for (what, p) in [("converter", &self.bpx), ("GTA tool", &self.cwconv), ("plugin", &self.asi)] {
            if !p.is_file() {
                anyhow::bail!("the {what} is missing ({}); is the mod folder complete?", p.display());
            }
        }
        Ok(())
    }

    /// Copies the mod's data into the working folder (changed files only).
    pub fn prepare(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        if let Some(data) = &self.data {
            sync_data(data, &self.root)?;
        }
        Ok(())
    }
}

fn sync_data(from: &Path, to: &Path) -> Result<()> {
    for entry in std::fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&dst)?;
            sync_data(&src, &dst)?;
        } else if std::fs::read(&dst).ok() != Some(std::fs::read(&src)?) {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Folders the player confirmed last time, and the working folder the mod
/// was built in (`work`: the Paradise Viewer loads the mod's map edits from
/// its tools\edits).
fn config_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("GTA-Paradise").join("setup.json"))
}

/// The remembered folders (gta, burnout, codewalker), where known.
pub fn remembered() -> [Option<PathBuf>; 3] {
    let config: serde_json::Value =
        config_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::json!({}));
    ["gta", "burnout", "codewalker"].map(|k| config[k].as_str().map(PathBuf::from))
}

pub fn remember(g: &Games, layout: &Layout) -> Result<()> {
    let Some(p) = config_path() else { return Ok(()) };
    std::fs::create_dir_all(p.parent().unwrap())?;
    let config = serde_json::json!({ "gta": g.gta, "burnout": g.burnout, "codewalker": g.codewalker, "work": layout.root });
    std::fs::write(p, serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}

/// A command that doesn't open a console window of its own (the setup
/// window runs the converters, `reg` and `tasklist`).
pub fn command(exe: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new(exe);
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c
}

/// Reads a line from the console (the console front end only).
pub fn ask(question: &str) -> String {
    use std::io::Write;
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    s.trim().trim_matches('"').to_string()
}
