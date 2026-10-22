//! No GTA missions (`@gta_missions off`, Paradise City only): GTA's story
//! missions, strangers and freaks, random events and side activities are
//! stopped, and every map blip they or GTA's shops leave is hidden. Their
//! places are in the removed Los Santos; Paradise City is on top of them.
//!
//! - The scripts that offer them are stopped by name whenever they run
//!   (checked once a second): the story flow, the mission triggers and
//!   their blips, phone calls, and every `launcher_` / `re_` script that
//!   GTA starts near their places on the map.
//! - Blips: every sprite's blips are gone through, a few sprites a frame;
//!   ones on a place (not on a car or a person, so police and the
//!   player's own car stay) are hidden. The player's waypoint and the
//!   plugin's own blips (`keep`) stay.

use crate::natives as n;
use std::collections::HashMap;

/// Scripts stopped by exact name.
const STOP: &[&str] = &[
    "flow_controller",
    "blip_controller",
    "comms_controller",
    "randomchar_controller",
    "friends_controller",
    "shop_controller",
    "mission_repeat_controller",
    "bailbond_launcher",
    "taxilauncher",
    "ambient_tonya",
    "ambient_mrsphilips",
    "ambient_solomon",
    "ambient_ufos",
];
/// Scripts stopped by the start of their name.
const STOP_PREFIX: &[&str] = &["mission_triggerer_", "launcher_", "re_"];

/// Blip sprites never hidden: the player (6), north (7) and the player's
/// waypoint (8).
const KEEP_SPRITES: [i32; 3] = [6, 7, 8];
/// Blip sprites gone through (GTA's highest is in the 800s).
const SPRITES: i32 = 900;
const SPRITES_PER_FRAME: i32 = 60;
/// Blip types left alone: on a vehicle or a ped.
const ENTITY_BLIPS: [i32; 2] = [1, 2];

#[derive(Default)]
pub struct NoMissions {
    last_scripts: i32,
    next_sprite: i32,
    /// How often each script has been stopped, for the log.
    stopped: HashMap<String, u32>,
    hidden: u32,
    last_report: i32,
}

fn stops(name: &str) -> bool {
    STOP.contains(&name) || STOP_PREFIX.iter().any(|p| name.starts_with(p))
}

impl NoMissions {
    pub fn update(&mut self, keep: &[i32], now: i32) {
        if now - self.last_scripts >= 1000 {
            self.last_scripts = now;
            n::script_thread_iterator_reset();
            loop {
                let id = n::script_thread_next();
                if id == 0 {
                    break;
                }
                let Some(name) = n::script_name(id) else { continue };
                if stops(&name) {
                    n::terminate_thread(id);
                    let count = self.stopped.entry(name.clone()).or_insert(0);
                    *count += 1;
                    if *count == 1 {
                        crate::log(&format!("no GTA missions: stopped script {name}"));
                    }
                }
            }
        }
        let first = self.next_sprite;
        self.next_sprite = (first + SPRITES_PER_FRAME) % SPRITES;
        for sprite in first..(first + SPRITES_PER_FRAME).min(SPRITES) {
            if KEEP_SPRITES.contains(&sprite) {
                continue;
            }
            let mut blip = n::first_blip_info_id(sprite);
            while n::does_blip_exist(blip) {
                if !keep.contains(&blip) && !ENTITY_BLIPS.contains(&n::blip_info_type(blip)) && n::blip_display(blip) != 0 {
                    n::set_blip_display(blip, 0);
                    self.hidden += 1;
                }
                blip = n::next_blip_info_id(sprite);
            }
        }
        if self.hidden > 0 && now - self.last_report >= 10_000 {
            self.last_report = now;
            crate::log(&format!("no GTA missions: {} map blip(s) hidden", self.hidden));
            self.hidden = 0;
        }
    }
}
