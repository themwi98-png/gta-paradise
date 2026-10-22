//! Reader for Burnout Paradise PC bundles (`bnd2`, version 2).
//!
//! Layout (all little-endian on PC):
//! - 0x28-byte header, then optional debug XML (resource names/types)
//! - an array of 64-byte resource entries
//! - three data regions ("memory types": main, graphics, unused); each
//!   resource stores one optional chunk per region, zlib-compressed when the
//!   bundle's compressed flag is set.

use anyhow::{bail, ensure, Context, Result};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

pub const FLAG_COMPRESSED: u32 = 0x1;
pub const FLAG_HAS_DEBUG_DATA: u32 = 0x8;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u64,
    pub import_hash: u64,
    pub uncompressed_size: [u32; 3],
    pub uncompressed_align: [u32; 3],
    pub disk_size: [u32; 3],
    pub disk_offset: [u32; 3],
    pub import_offset: u32,
    pub type_id: u32,
    pub import_count: u16,
    pub flags: u8,
    pub stream_index: u8,
}

#[derive(Debug, Clone)]
pub struct DebugInfo {
    pub name: String,
    pub type_name: String,
}

pub struct Bundle {
    pub platform: u32,
    pub flags: u32,
    pub data_offsets: [u32; 3],
    pub entries: Vec<Entry>,
    pub debug: HashMap<u64, DebugInfo>,
    raw: Vec<u8>,
}

/// Names for resources whose own bundle has none for them (a modded game
/// can ship bundles without debug data), from the game's other bundles.
static FALLBACK_NAMES: OnceLock<HashMap<u64, String>> = OnceLock::new();

/// Sets the fallback names (once per run; see `debug_names`).
pub fn set_fallback_names(names: HashMap<u64, String>) {
    let _ = FALLBACK_NAMES.set(names);
}

/// Every resource name in the debug data of the bundles in `dir` (not its
/// subfolders). Only each file's header and debug block are read.
pub fn debug_names(dir: &Path) -> HashMap<u64, String> {
    let mut out = HashMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let Ok(mut f) = std::fs::File::open(&p) else { continue };
        let mut head = [0u8; 0x28];
        if f.read_exact(&mut head).is_err() || &head[0..4] != b"bnd2" {
            continue;
        }
        let (debug_offset, flags) = (u32_at(&head, 0x0C) as u64, u32_at(&head, 0x24));
        if flags & FLAG_HAS_DEBUG_DATA == 0 || debug_offset == 0 || f.seek(SeekFrom::Start(debug_offset)).is_err() {
            continue;
        }
        let mut xml = Vec::new();
        let mut buf = vec![0u8; 1 << 16];
        while let Ok(n) = f.read(&mut buf) {
            if n == 0 {
                break;
            }
            match buf[..n].iter().position(|&c| c == 0) {
                Some(end) => {
                    xml.extend_from_slice(&buf[..end]);
                    break;
                }
                None => xml.extend_from_slice(&buf[..n]),
            }
        }
        for (id, d) in parse_debug_xml(&String::from_utf8_lossy(&xml)) {
            out.entry(id).or_insert(d.name);
        }
    }
    out
}

/// (id, type) of every resource in a bundle, reading only its header and
/// entry table.
pub fn entry_ids(path: &Path) -> Result<Vec<(u64, u32)>> {
    let mut f = std::fs::File::open(path)?;
    let mut head = [0u8; 0x28];
    f.read_exact(&mut head)?;
    ensure!(&head[0..4] == b"bnd2", "not a bnd2 bundle");
    let (count, at) = (u32_at(&head, 0x10) as usize, u32_at(&head, 0x14) as u64);
    f.seek(SeekFrom::Start(at))?;
    let mut t = vec![0u8; count * 0x40];
    f.read_exact(&mut t)?;
    Ok((0..count).map(|i| (u64_at(&t, i * 0x40), u32_at(&t, i * 0x40 + 0x38))).collect())
}

pub fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
pub fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
pub fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl Bundle {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(raw).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(raw: Vec<u8>) -> Result<Self> {
        ensure!(raw.len() >= 0x28, "file too small");
        ensure!(&raw[0..4] == b"bnd2", "not a bnd2 bundle");
        let version = u32_at(&raw, 4);
        ensure!(version == 2, "unsupported bundle version {version}");
        let platform = u32_at(&raw, 8);
        ensure!(platform == 1, "only PC bundles (platform 1) supported, got {platform}");
        let debug_offset = u32_at(&raw, 0x0C) as usize;
        let count = u32_at(&raw, 0x10) as usize;
        let entries_offset = u32_at(&raw, 0x14) as usize;
        let data_offsets = [u32_at(&raw, 0x18), u32_at(&raw, 0x1C), u32_at(&raw, 0x20)];
        let flags = u32_at(&raw, 0x24);

        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let o = entries_offset + i * 0x40;
            ensure!(o + 0x40 <= raw.len(), "entry table truncated");
            let mut usz = [0; 3];
            let mut ual = [0; 3];
            let mut dsz = [0; 3];
            let mut dof = [0; 3];
            for m in 0..3 {
                let v = u32_at(&raw, o + 0x10 + m * 4);
                usz[m] = v & 0x0FFF_FFFF;
                ual[m] = 1 << (v >> 28);
                dsz[m] = u32_at(&raw, o + 0x1C + m * 4) & 0x0FFF_FFFF;
                dof[m] = u32_at(&raw, o + 0x28 + m * 4);
            }
            entries.push(Entry {
                id: u64_at(&raw, o),
                import_hash: u64_at(&raw, o + 8),
                uncompressed_size: usz,
                uncompressed_align: ual,
                disk_size: dsz,
                disk_offset: dof,
                import_offset: u32_at(&raw, o + 0x34),
                type_id: u32_at(&raw, o + 0x38),
                import_count: u16_at(&raw, o + 0x3C),
                flags: raw[o + 0x3E],
                stream_index: raw[o + 0x3F],
            });
        }

        let mut debug = HashMap::new();
        if flags & FLAG_HAS_DEBUG_DATA != 0 && debug_offset != 0 {
            let end = raw[debug_offset..].iter().position(|&c| c == 0).unwrap_or(0) + debug_offset;
            let xml = String::from_utf8_lossy(&raw[debug_offset..end]);
            debug = parse_debug_xml(&xml);
        }

        Ok(Bundle { platform, flags, data_offsets, entries, debug, raw })
    }

    pub fn find(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// A resource's debug name: from this bundle, else from the fallback names.
    pub fn name_of(&self, id: u64) -> Option<&str> {
        self.debug.get(&id).map(|d| d.name.as_str()).or_else(|| FALLBACK_NAMES.get()?.get(&id).map(String::as_str))
    }

    /// Returns the (decompressed) bytes of one memory chunk of a resource.
    pub fn chunk(&self, e: &Entry, mem: usize) -> Result<Vec<u8>> {
        let size = e.disk_size[mem] as usize;
        if size == 0 {
            return Ok(Vec::new());
        }
        let start = self.data_offsets[mem] as usize + e.disk_offset[mem] as usize;
        let Some(src) = self.raw.get(start..start + size) else {
            bail!("resource {:08X} chunk {mem} out of bounds", e.id);
        };
        if self.flags & FLAG_COMPRESSED != 0 {
            let mut out = Vec::with_capacity(e.uncompressed_size[mem] as usize);
            flate2::read::ZlibDecoder::new(src)
                .read_to_end(&mut out)
                .with_context(|| format!("inflating resource {:08X} chunk {mem}", e.id))?;
            Ok(out)
        } else {
            Ok(src.to_vec())
        }
    }

    /// The import table lives inside the main chunk at `import_offset`:
    /// `import_count` records of (resource id: u64, pointer offset: u32, pad: u32).
    pub fn imports(&self, e: &Entry, main: &[u8]) -> Vec<(u64, u32)> {
        let base = e.import_offset as usize;
        (0..e.import_count as usize)
            .filter_map(|i| {
                let o = base + i * 16;
                (o + 12 <= main.len()).then(|| (u64_at(main, o), u32_at(main, o + 8)))
            })
            .collect()
    }
}

/// Minimal parser for the `<ResourceStringTable>` debug block; avoids an XML
/// dependency since the format is fixed and machine-written.
fn parse_debug_xml(xml: &str) -> HashMap<u64, DebugInfo> {
    let attr = |tag: &str, key: &str| -> Option<String> {
        let start = tag.find(&format!("{key}=\""))? + key.len() + 2;
        let len = tag[start..].find('"')?;
        Some(tag[start..start + len].to_string())
    };
    let mut map = HashMap::new();
    for tag in xml.split("<Resource ").skip(1) {
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        let (Some(id), Some(name), Some(ty)) = (attr(tag, "id"), attr(tag, "name"), attr(tag, "type")) else {
            continue;
        };
        if let Ok(id) = u64::from_str_radix(id.trim_start_matches("0x"), 16) {
            map.insert(id, DebugInfo { name, type_name: ty });
        }
    }
    map
}
