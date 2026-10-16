//! Burnout Paradise traffic lanes (`B5TRAFFIC.BNDL`, resource type 0x10002
//! "TrafficData", format 45; 32-bit layout from burnout.wiki).
//!
//! The world is split into hulls; each hull holds sections, and a section is
//! one lane: a run of rungs (pairs of points across the lane, left and right
//! edge) that traffic drives along in rung order. Sections link to up to three
//! sections ahead and behind, possibly in other hulls, and to their left /
//! right neighbour lanes.

use bnd2::{f32_at, u16_at, u32_at};

pub const TRAFFIC_DATA: u32 = 0x10002;

/// A (hull, section) reference.
pub type SectionRef = (u16, u8);

#[derive(Debug, Clone)]
pub struct Section {
    /// Lane cross-sections in driving order: [left edge, right edge].
    pub rungs: Vec<[[f32; 3]; 2]>,
    /// Sections traffic may continue into / came from.
    pub forward: Vec<SectionRef>,
    pub backward: Vec<SectionRef>,
    /// Neighbouring lanes (same hull): section index, shared rung count, our
    /// and their first shared rung. Left neighbours come first.
    pub left: Vec<Neighbour>,
    pub right: Vec<Neighbour>,
    /// Speed limit (m/s, as stored) and length (m).
    pub speed: f32,
    pub length: f32,
    pub turn_left_prob: u8,
    pub turn_right_prob: u8,
    /// How busy the lane is: Burnout's traffic flow (vehicles per minute).
    pub vehicles_per_minute: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct Neighbour {
    pub section: u8,
    pub shared_length: u8,
    pub our_start_rung: u8,
    pub their_start_rung: u8,
}

/// A parked car: world transform (Burnout's row-major affine matrix; rows 0-2
/// are the car's axes, row 3 its position), flow type and spawn chance.
/// `model` is a GTA vehicle model set by hand (none in Burnout's data: GTA
/// picks a car).
#[derive(Debug, Clone)]
pub struct StaticVehicle {
    pub transform: [f32; 16],
    pub flow_type: u16,
    pub exists_chance: u8,
    pub flags: u8,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Hull {
    pub sections: Vec<Section>,
    pub parked: Vec<StaticVehicle>,
}

#[derive(Debug, Clone)]
pub struct Traffic {
    pub version: u8,
    pub hulls: Vec<Hull>,
}

const SECTION_SIZE: usize = 0x30;
const RUNG_SIZE: usize = 0x20;
const STATIC_VEHICLE_SIZE: usize = 0x50;
const NO_HULL: u16 = 0xFFFF;
const NO_SECTION: u8 = 0xFF;

fn vec3(b: &[u8], o: usize) -> [f32; 3] {
    [f32_at(b, o), f32_at(b, o + 4), f32_at(b, o + 8)]
}

/// Parses the TrafficData main chunk (pointers are offsets into it).
pub fn parse_traffic(b: &[u8]) -> Traffic {
    let version = b[0];
    let num_hulls = u16_at(b, 2) as usize;
    let hull_ptrs = u32_at(b, 0xC) as usize;
    let mut hulls = Vec::with_capacity(num_hulls);
    for h in 0..num_hulls {
        let ho = u32_at(b, hull_ptrs + h * 4) as usize;
        if ho == 0 || ho + 0x50 > b.len() {
            hulls.push(Hull::default());
            continue;
        }
        let num_sections = b[ho] as usize;
        let num_rungs = u16_at(b, ho + 8) as usize;
        let sections_at = u32_at(b, ho + 0x10) as usize;
        let rungs_at = u32_at(b, ho + 0x14) as usize;
        let neighbours_at = u32_at(b, ho + 0x1C) as usize;
        // One flow (flow type, vehicles per minute) per section.
        let flows_at = u32_at(b, ho + 0x28) as usize;
        let rung = |i: usize| -> [[f32; 3]; 2] {
            let o = rungs_at + i * RUNG_SIZE;
            [vec3(b, o), vec3(b, o + 0x10)]
        };
        let neighbour = |i: usize| -> Neighbour {
            let o = neighbours_at + i * 4;
            Neighbour { section: b[o], shared_length: b[o + 1], our_start_rung: b[o + 2], their_start_rung: b[o + 3] }
        };
        let mut sections = Vec::with_capacity(num_sections);
        for s in 0..num_sections {
            let so = sections_at + s * SECTION_SIZE;
            let rung_offset = u32_at(b, so) as usize;
            let n_rungs = b[so + 4] as usize;
            let links = |hull_at: usize, sec_at: usize| -> Vec<SectionRef> {
                (0..3)
                    .map(|k| (u16_at(b, so + hull_at + k * 2), b[so + sec_at + k]))
                    .filter(|&(h, s)| h != NO_HULL && s != NO_SECTION)
                    .collect()
            };
            let nb_offset = u16_at(b, so + 0x1C) as usize;
            let (n_left, n_right) = (b[so + 0x1E] as usize, b[so + 0x1F] as usize);
            sections.push(Section {
                rungs: (rung_offset..(rung_offset + n_rungs).min(num_rungs)).map(rung).collect(),
                forward: links(0x8, 0x14),
                backward: links(0xE, 0x17),
                left: (0..n_left).map(|k| neighbour(nb_offset + k)).collect(),
                right: (0..n_right).map(|k| neighbour(nb_offset + n_left + k)).collect(),
                speed: f32_at(b, so + 0x24),
                length: f32_at(b, so + 0x28),
                turn_left_prob: b[so + 0x1A],
                turn_right_prob: b[so + 0x1B],
                vehicles_per_minute: if flows_at == 0 { 0 } else { u16_at(b, flows_at + s * 4 + 2) },
            });
        }
        let num_static = b[ho + 5] as usize;
        let static_at = u32_at(b, ho + 0x24) as usize;
        let parked = (0..num_static)
            .map(|k| {
                let o = static_at + k * STATIC_VEHICLE_SIZE;
                let mut transform = [0.0; 16];
                for (i, v) in transform.iter_mut().enumerate() {
                    *v = f32_at(b, o + i * 4);
                }
                StaticVehicle { transform, flow_type: u16_at(b, o + 0x40), exists_chance: b[o + 0x42], flags: b[o + 0x43], model: None }
            })
            .collect();
        hulls.push(Hull { sections, parked });
    }
    Traffic { version, hulls }
}
