//! The GTA Paradise setup window: reminds the player what to install first,
//! finds (or lets them pick) GTA V Legacy, Burnout Paradise Remastered and
//! CodeWalker, then builds and installs with a step-by-step progress list.
//! `--dev` runs from the project folder, like the console version. The
//! player also picks the map: Paradise City, or GTA's own Los Santos with
//! only the Burnout driving.

#![windows_subsystem = "windows"]

use eframe::egui;
use paradise_setup::{find, steps, Games, Layout};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const GREEN: egui::Color32 = egui::Color32::from_rgb(70, 180, 90);
const RED: egui::Color32 = egui::Color32::from_rgb(220, 80, 70);
const AMBER: egui::Color32 = egui::Color32::from_rgb(230, 170, 40);

/// One folder the player confirms.
struct Folder {
    label: &'static str,
    hint: &'static str,
    text: String,
    /// The checked folder (CodeWalker's may be a subfolder), or why not.
    status: Result<PathBuf, String>,
    check: fn(&std::path::Path) -> anyhow::Result<PathBuf>,
}

impl Folder {
    fn new(label: &'static str, hint: &'static str, found: Option<PathBuf>, check: fn(&std::path::Path) -> anyhow::Result<PathBuf>) -> Self {
        let mut f = Folder { label, hint, text: found.map(|p| p.display().to_string()).unwrap_or_default(), status: Err(String::new()), check };
        f.recheck();
        f
    }

    fn recheck(&mut self) {
        let t = self.text.trim().trim_matches('"');
        self.status = if t.is_empty() { Err("not found yet: pick its folder".into()) } else { (self.check)(std::path::Path::new(t)).map_err(|e| e.to_string()) };
    }
}

/// What the build thread reports.
#[derive(Default)]
struct Progress {
    states: Vec<Option<steps::State>>,
    running_since: Option<Instant>,
    finished: bool,
    error: Option<String>,
    log_tail: Vec<String>,
}

struct App {
    dev: bool,
    layout: Result<Layout, String>,
    folders: [Folder; 3],
    titles: Vec<&'static str>,
    progress: Option<Arc<Mutex<Progress>>>,
    /// The map to install (starts as the one installed now).
    map: steps::Map,
}

impl App {
    fn new(dev: bool) -> Self {
        let layout = Layout::new(dev).and_then(|l| l.check().map(|_| l)).map_err(|e| format!("{e:#}"));
        let [gta, burnout, codewalker] = paradise_setup::remembered();
        let gta_found = find::detect_gta(gta.clone());
        let map = gta_found.as_deref().map_or(steps::Map::Paradise, steps::installed_map);
        App {
            dev,
            layout,
            folders: [
                Folder::new("GTA V (Legacy)", "the folder with GTA5.exe", gta_found, find::check_gta),
                Folder::new("Burnout Paradise Remastered", "the folder with BurnoutPR.exe", find::detect_burnout(burnout), find::check_burnout),
                Folder::new("CodeWalker 3.0", "the unzipped CodeWalker folder", find::detect_codewalker(codewalker), find::check_codewalker),
            ],
            titles: steps::titles(),
            progress: None,
            map,
        }
    }

    fn games(&self) -> Option<Games> {
        let [g, b, c] = [0, 1, 2].map(|i| self.folders[i].status.clone().ok());
        Some(Games { gta: g?, burnout: b?, codewalker: c? })
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(games) = self.games() else { return };
        let progress = Arc::new(Mutex::new(Progress { states: vec![None; self.titles.len()], ..Default::default() }));
        self.progress = Some(progress.clone());
        let (dev, ctx, map) = (self.dev, ctx.clone(), self.map);
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                let layout = Layout::new(dev)?;
                paradise_setup::remember(&games, &layout)?;
                layout.prepare()?;
                let report = |i: usize, _: &str, state: steps::State| {
                    let mut p = progress.lock().unwrap();
                    p.running_since = (state == steps::State::Running).then(Instant::now);
                    p.states[i] = Some(state);
                    ctx.request_repaint();
                };
                let r = steps::run_all(&layout, &games, None, false, Some(map), &report);
                if r.is_err() {
                    progress.lock().unwrap().log_tail = steps::log_tail(&layout, 12);
                }
                r
            })();
            let mut p = progress.lock().unwrap();
            p.finished = true;
            p.error = result.err().map(|e| format!("{e:#}"));
            ctx.request_repaint();
        });
    }

    fn before_you_start(&self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).fill(ui.visuals().faint_bg_color).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("Before you start").strong().color(AMBER));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("1. Install ScriptHookV into GTA V Legacy:");
                ui.hyperlink_to("dev-c.com/gtav/scripthookv", find::SCRIPTHOOKV_URL);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("2. Install OpenIV, then in OpenIV: Tools > ASI Manager > install OpenIV.ASI:");
                ui.hyperlink_to("openiv.com", find::OPENIV_URL);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("3. Download CodeWalker 3.0 and unzip it anywhere (e.g. Downloads):");
                ui.hyperlink_to("gta5-mods.com", find::CODEWALKER_URL);
            });
            ui.label("4. Close GTA V.");
            ui.add_space(4.0);
            ui.label(egui::RichText::new("GTA V Enhanced and GTA Online are never touched. Nothing from either game is included in this download.").small().weak());
        });
    }

    fn folders_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Your games").strong());
        for f in &mut self.folders {
            ui.add_space(6.0);
            ui.label(f.label);
            ui.horizontal(|ui| {
                let edit = egui::TextEdit::singleline(&mut f.text).hint_text(f.hint).desired_width(ui.available_width() - 90.0);
                if ui.add(edit).changed() {
                    f.recheck();
                }
                if ui.button("Browse…").clicked() {
                    let mut dialog = rfd::FileDialog::new().set_title(format!("Pick {}", f.label));
                    if let Ok(p) = &f.status {
                        dialog = dialog.set_directory(p);
                    }
                    if let Some(p) = dialog.pick_folder() {
                        f.text = p.display().to_string();
                        f.recheck();
                    }
                }
            });
            match &f.status {
                Ok(_) => ui.label(egui::RichText::new("✔ found").color(GREEN).small()),
                Err(e) => ui.label(egui::RichText::new(format!("✖ {e}")).color(RED).small()),
            };
        }
        // The loaders, once GTA's folder is known.
        if let Ok(gta) = &self.folders[0].status {
            let (shv, openiv) = find::loaders(gta);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (ok, name) in [(shv, "ScriptHookV"), (openiv, "OpenIV.asi")] {
                    let (mark, colour) = if ok { ("✔", GREEN) } else { ("✖", RED) };
                    ui.label(egui::RichText::new(format!("{mark} {name}")).color(colour).small());
                }
                if !(shv && openiv) {
                    ui.label(egui::RichText::new("missing in the GTA folder: see Before you start").color(RED).small());
                }
            });
        }
    }

    fn progress_ui(&self, ui: &mut egui::Ui, p: &Progress) {
        for (i, title) in self.titles.iter().enumerate() {
            ui.horizontal(|ui| {
                match &p.states[i] {
                    None => ui.label(egui::RichText::new("·").weak()),
                    Some(steps::State::Running) => ui.add(egui::Spinner::new().size(12.0)),
                    Some(steps::State::Failed) => ui.label(egui::RichText::new("✖").color(RED)),
                    Some(steps::State::NotNeeded) => ui.label(egui::RichText::new("–").weak()),
                    Some(_) => ui.label(egui::RichText::new("✔").color(GREEN)),
                };
                ui.label(*title);
                let note = match &p.states[i] {
                    Some(steps::State::Running) => p.running_since.map(|t| mmss(t.elapsed().as_secs())).unwrap_or_default(),
                    Some(steps::State::Done(s)) => mmss(*s),
                    Some(steps::State::UpToDate) => "up to date".into(),
                    Some(steps::State::NotNeeded) => "not needed for Los Santos".into(),
                    _ => String::new(),
                };
                ui.label(egui::RichText::new(note).weak());
            });
        }
    }
}

fn mmss(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(format!("GTA Paradise {VERSION}"));
                ui.label("Burnout Paradise City in GTA V story mode.");
                ui.add_space(8.0);
                if let Err(e) = &self.layout {
                    ui.colored_label(RED, e);
                    return;
                }

                let Some(progress) = self.progress.clone() else {
                    self.before_you_start(ui);
                    ui.add_space(10.0);
                    self.folders_ui(ui);
                    ui.add_space(14.0);
                    let ready = self.games().is_some_and(|g| find::loaders(&g.gta) == (true, true));
                    ui.label(egui::RichText::new("Map").strong());
                    ui.radio_value(&mut self.map, steps::Map::Paradise, "Paradise City (replaces Los Santos)");
                    ui.radio_value(&mut self.map, steps::Map::LosSantos, "Los Santos, with only the Burnout driving");
                    ui.label(
                        egui::RichText::new("Los Santos keeps GTA's own map, water, traffic and people, with the Burnout car handling, crashes, wrecks, boost and drifting. Switch any time by installing again.")
                            .small()
                            .weak(),
                    );
                    ui.add_space(8.0);
                    let note = match self.map {
                        steps::Map::Paradise => "The first install converts the whole city: about 15-20 minutes and 10 GB of free disk space. Updates only redo what changed.",
                        steps::Map::LosSantos => "Only the driving is installed: a few minutes.",
                    };
                    ui.label(egui::RichText::new(note).small().weak());
                    ui.add_space(6.0);
                    if ui.add_enabled(ready, egui::Button::new(egui::RichText::new("Install").strong()).min_size(egui::vec2(140.0, 32.0))).clicked() {
                        if steps::gta_running() {
                            rfd::MessageDialog::new().set_title("GTA Paradise").set_description("GTA V is running. Close it, then press Install again.").show();
                        } else {
                            self.start(ctx);
                        }
                    }
                    return;
                };

                let p = progress.lock().unwrap();
                ui.label(egui::RichText::new("Installing").strong());
                ui.add_space(6.0);
                self.progress_ui(ui, &p);
                ui.add_space(12.0);
                if !p.finished {
                    ui.label(egui::RichText::new("You can leave this running. Don't start GTA V until it's done.").weak());
                    ctx.request_repaint_after(std::time::Duration::from_millis(500));
                    return;
                }
                match &p.error {
                    None => {
                        ui.colored_label(GREEN, egui::RichText::new("Done! Start GTA V and load story mode to play in Paradise City.").strong());
                    }
                    Some(e) => {
                        ui.colored_label(RED, format!("Setup stopped: {e}"));
                        ui.add_space(6.0);
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            for l in &p.log_tail {
                                ui.label(egui::RichText::new(l).monospace().small());
                            }
                        });
                        if let Ok(layout) = &self.layout {
                            if ui.button("Open the full log").clicked() {
                                let _ = std::process::Command::new("notepad").arg(steps::log_path(layout)).spawn();
                            }
                        }
                    }
                }
                ui.add_space(6.0);
                let failed = p.error.is_some();
                drop(p);
                ui.horizontal(|ui| {
                    if failed && ui.button("Back").clicked() {
                        self.progress = None;
                    }
                    if ui.button("Close").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });
    }
}

fn main() -> eframe::Result {
    let dev = std::env::args().any(|a| a == "--dev");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("GTA Paradise Setup").with_inner_size([660.0, 760.0]).with_min_inner_size([520.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native("GTA Paradise Setup", options, Box::new(move |_cc| Ok(Box::new(App::new(dev)))))
}
