//! Texture output: DDS (kept losslessly for the GTA side) and PNG (mip 0,
//! for glTF previews).

use anyhow::{bail, Result};
use bpworld::Texture;
use std::path::Path;

/// Bytes per 4x4 block for block-compressed formats, else None.
fn block_bytes(fmt: u32) -> Option<usize> {
    match fmt {
        70..=72 | 79..=81 => Some(8),
        73..=78 | 82..=84 | 94..=99 => Some(16),
        _ => None,
    }
}

fn bytes_per_pixel(fmt: u32) -> Option<usize> {
    match fmt {
        27..=32 | 87..=93 => Some(4),
        61 | 65 => Some(1),
        _ => None,
    }
}

pub fn has_alpha(fmt: u32) -> bool {
    matches!(fmt, 73..=78 | 97..=99 | 27..=32 | 87 | 91)
}

fn mip0_size(t: &Texture) -> Option<usize> {
    let (w, h) = (t.width as usize, t.height as usize);
    if let Some(bb) = block_bytes(t.dxgi_format) {
        Some(w.div_ceil(4) * h.div_ceil(4) * bb)
    } else {
        bytes_per_pixel(t.dxgi_format).map(|bpp| w * h * bpp)
    }
}

/// Writes a DDS with a DX10 extension header, which can describe any DXGI
/// format and is understood by CodeWalker, Blender and texconv.
pub fn write_dds(t: &Texture, path: &Path) -> Result<()> {
    let mut h = Vec::with_capacity(148 + t.data.len());
    let put = |h: &mut Vec<u8>, v: u32| h.extend_from_slice(&v.to_le_bytes());
    h.extend_from_slice(b"DDS ");
    put(&mut h, 124);
    // CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT | LINEARSIZE
    put(&mut h, 0x1 | 0x2 | 0x4 | 0x1000 | 0x20000 | 0x80000);
    put(&mut h, t.height);
    put(&mut h, t.width);
    put(&mut h, mip0_size(t).unwrap_or(0) as u32);
    put(&mut h, 0); // depth
    put(&mut h, t.mips);
    for _ in 0..11 {
        put(&mut h, 0);
    }
    // DDS_PIXELFORMAT: size, flags=FOURCC, 'DX10'
    put(&mut h, 32);
    put(&mut h, 0x4);
    h.extend_from_slice(b"DX10");
    for _ in 0..5 {
        put(&mut h, 0);
    }
    put(&mut h, 0x1000 | 0x8 | 0x400000); // TEXTURE | COMPLEX | MIPMAP
    for _ in 0..4 {
        put(&mut h, 0);
    }
    // DDS_HEADER_DXT10
    put(&mut h, t.dxgi_format);
    put(&mut h, 3); // TEXTURE2D
    put(&mut h, 0);
    put(&mut h, 1);
    put(&mut h, 0);
    h.extend_from_slice(&t.data);
    std::fs::write(path, h)?;
    Ok(())
}

/// Decodes mip 0 to RGBA8.
pub fn decode_rgba(t: &Texture) -> Result<Vec<u8>> {
    use texture2ddecoder as d;
    let (w, h) = (t.width as usize, t.height as usize);
    let Some(size) = mip0_size(t) else {
        bail!("unsupported texture format {}", t.dxgi_format);
    };
    if t.data.len() < size {
        bail!("texture data truncated");
    }
    let src = &t.data[..size];
    if let Some(bpp) = bytes_per_pixel(t.dxgi_format) {
        let mut out = Vec::with_capacity(w * h * 4);
        for px in src.chunks(bpp) {
            match (t.dxgi_format, bpp) {
                (87..=93, 4) => out.extend([px[2], px[1], px[0], px[3]]),
                (_, 4) => out.extend_from_slice(px),
                _ => out.extend([px[0], px[0], px[0], 255]),
            }
        }
        return Ok(out);
    }
    let mut img = vec![0u32; w.max(4) * h.max(4)];
    let r = match t.dxgi_format {
        70..=72 => d::decode_bc1a(src, w, h, &mut img),
        73..=75 => d::decode_bc2(src, w, h, &mut img),
        76..=78 => d::decode_bc3(src, w, h, &mut img),
        79..=81 => d::decode_bc4(src, w, h, &mut img),
        82..=84 => d::decode_bc5(src, w, h, &mut img),
        97..=99 => d::decode_bc7(src, w, h, &mut img),
        f => bail!("unsupported texture format {f}"),
    };
    if let Err(e) = r {
        bail!("decode failed: {e}");
    }
    // texture2ddecoder packs pixels as 0xAARRGGBB.
    Ok(img[..w * h]
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8])
        .collect())
}

pub fn write_png(t: &Texture, path: &Path) -> Result<()> {
    let rgba = decode_rgba(t)?;
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut enc = png::Encoder::new(file, t.width, t.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    Ok(())
}
