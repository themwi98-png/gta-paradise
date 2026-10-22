//! Frame cost of each part of the plugin, written to paradise.log every
//! REPORT_MS (`@perf on`): average milliseconds per frame for each part, and
//! the game's own frame rate over the same time, to find what slows the game.

use std::time::Instant;

const REPORT_MS: i32 = 15_000;

pub const PARTS: [&str; 10] = ["spawn guard", "prop sweep", "prop shield", "wreck", "boost", "physics", "police", "junctions", "no missions", "shops"];

pub struct Perf {
    total: [f64; PARTS.len()],
    worst: [f64; PARTS.len()],
    frames: u32,
    game_time: f64,
    since: i32,
    mark: Instant,
}

impl Perf {
    pub fn new(now: i32) -> Self {
        Self { total: [0.0; PARTS.len()], worst: [0.0; PARTS.len()], frames: 0, game_time: 0.0, since: now, mark: Instant::now() }
    }

    /// Starts timing the next part.
    pub fn start(&mut self) {
        self.mark = Instant::now();
    }

    /// The part `i` just finished.
    pub fn done(&mut self, i: usize) {
        let ms = self.mark.elapsed().as_secs_f64() * 1000.0;
        self.total[i] += ms;
        self.worst[i] = self.worst[i].max(ms);
        self.mark = Instant::now();
    }

    /// Once per frame; writes the report when it's due.
    pub fn frame(&mut self, now: i32, frame_time: f32) {
        self.frames += 1;
        self.game_time += frame_time as f64;
        if now - self.since < REPORT_MS || self.frames == 0 {
            return;
        }
        let f = self.frames as f64;
        let sum: f64 = self.total.iter().sum::<f64>() / f;
        let parts: Vec<String> = PARTS
            .iter()
            .enumerate()
            .map(|(i, name)| format!("{name} {:.2} (worst {:.1})", self.total[i] / f, self.worst[i]))
            .collect();
        crate::log(&format!(
            "perf: {:.0} fps; plugin {sum:.2} ms/frame: {}",
            f / self.game_time.max(1e-3),
            parts.join(", ")
        ));
        *self = Self::new(now);
    }
}
