//! GTA Paradise setup in a console (for testing; players use the window).
//!
//!   paradise-setup-cli                     find the games, build, install
//!   paradise-setup-cli --gta <dir> --burnout <dir> --codewalker <dir>
//!   paradise-setup-cli --only props,install   just those steps
//!   paradise-setup-cli --rebuild           ignore what's up to date
//!   paradise-setup-cli --yes               don't ask before installing
//!   paradise-setup-cli --dev               run from the project folder

use anyhow::{bail, Context, Result};
use paradise_setup::{find, steps, Games, Layout};
use std::io::Write as _;
use std::path::PathBuf;

struct Args {
    gta: Option<PathBuf>,
    burnout: Option<PathBuf>,
    codewalker: Option<PathBuf>,
    only: Option<Vec<String>>,
    rebuild: bool,
    yes: bool,
    dev: bool,
}

fn parse_args() -> Result<Args> {
    let mut a = Args { gta: None, burnout: None, codewalker: None, only: None, rebuild: false, yes: false, dev: false };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--gta" => a.gta = Some(value()?.into()),
            "--burnout" => a.burnout = Some(value()?.into()),
            "--codewalker" => a.codewalker = Some(value()?.into()),
            "--only" => a.only = Some(value()?.split(',').map(|s| s.trim().to_lowercase()).collect()),
            "--rebuild" => a.rebuild = true,
            "--yes" | "-y" => a.yes = true,
            "--dev" => a.dev = true,
            "-h" | "--help" => {
                println!("{}", include_str!("paradise-setup-cli.rs").lines().skip(2).take(6).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
                std::process::exit(0);
            }
            _ => bail!("unknown option {arg} (try --help)"),
        }
    }
    Ok(a)
}

fn run() -> Result<()> {
    let args = parse_args()?;
    println!("GTA Paradise setup\n");
    let layout = Layout::new(args.dev)?;
    layout.check()?;

    let [gta, burnout, codewalker] = paradise_setup::remembered();
    let games = Games {
        gta: find::gta(args.gta.or(gta))?,
        burnout: find::burnout(args.burnout.or(burnout))?,
        codewalker: find::codewalker(args.codewalker.or(codewalker))?,
    };
    find::check_loaders(&games.gta)?;
    println!("GTA V (Legacy):              {}", games.gta.display());
    println!("Burnout Paradise Remastered: {}", games.burnout.display());
    println!("CodeWalker:                  {}\n", games.codewalker.display());
    paradise_setup::remember(&games)?;
    layout.prepare()?;

    if !args.yes {
        println!("This builds Paradise City from your games (the first time takes a while and needs");
        println!("about 10 GB free in {}), then installs it into the GTA folder above", layout.root.display());
        println!("(its mods folder, plus paradise.* files and the plugin next to GTA5.exe).");
        if !paradise_setup::ask("Continue? [y/N] ").eq_ignore_ascii_case("y") {
            println!("Nothing changed.");
            return Ok(());
        }
    }
    let total = steps::titles().len();
    let report = |i: usize, title: &str, state: steps::State| match state {
        steps::State::Running => {
            print!("[{:>2}/{total}] {title:<44}", i + 1);
            let _ = std::io::stdout().flush();
        }
        steps::State::UpToDate => println!("[{:>2}/{total}] {title:<44}up to date", i + 1),
        steps::State::Done(s) => println!("done ({}:{:02})", s / 60, s % 60),
        steps::State::Failed => println!("failed"),
    };
    if let Err(e) = steps::run_all(&layout, &games, args.only.as_deref(), args.rebuild, &report) {
        eprintln!("\nLast lines of {}:", steps::log_path(&layout).display());
        for l in steps::log_tail(&layout, 15) {
            eprintln!("  {l}");
        }
        return Err(e);
    }
    println!("\nDone. Start GTA V (story mode) to play in Paradise City.");
    Ok(())
}

fn main() {
    let interactive = std::env::args().all(|a| a != "--yes" && a != "-y");
    if let Err(e) = run() {
        eprintln!("\nSetup stopped: {e:#}");
        if interactive {
            paradise_setup::ask("\nPress Enter to close.");
        }
        std::process::exit(1);
    }
    if interactive {
        paradise_setup::ask("\nPress Enter to close.");
    }
}
