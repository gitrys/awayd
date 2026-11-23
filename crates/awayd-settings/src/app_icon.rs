//! SPDX-License-Identifier: GPL-3.0-or-later
//! The app's mark (light-blue tile with a half-filled circle, as in the sidebar) as pixels:
//! the window icon at runtime and the source of `data/icons/awayd.ico`, which is regenerated
//! with `cargo test -p awayd-settings write_app_icon -- --ignored`.

use eframe::egui;

const TILE: [u8; 3] = [0x4C, 0xC2, 0xFF];
const MARK: [u8; 3] = [0x0B, 0x0B, 0x0B];
/// Samples per pixel and axis (smooth edges without a graphics library).
const SUPERSAMPLE: u32 = 4;

/// Straight (not premultiplied) RGBA pixels of the mark, `size` × `size`, row by row.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let radius = s * 0.22;
    let ring_r = s * 0.29;
    let ring_w = (s * 0.07).max(1.2);
    let c = s / 2.0;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut tile, mut mark) = (0u32, 0u32);
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let x = px as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32;
                    if !in_rounded_square(x, y, s, radius) {
                        continue;
                    }
                    tile += 1;
                    let d = ((x - c).powi(2) + (y - c).powi(2)).sqrt();
                    let on_ring = (d - ring_r).abs() <= ring_w / 2.0;
                    let in_half = d <= ring_r && x >= c;
                    if on_ring || in_half {
                        mark += 1;
                    }
                }
            }
            let n = (SUPERSAMPLE * SUPERSAMPLE) as f32;
            let alpha = tile as f32 / n;
            // Share of the mark within the covered part of the pixel.
            let m = if tile == 0 {
                0.0
            } else {
                mark as f32 / tile as f32
            };
            for ch in 0..3 {
                let v = TILE[ch] as f32 * (1.0 - m) + MARK[ch] as f32 * m;
                out.push(v.round() as u8);
            }
            out.push((alpha * 255.0).round() as u8);
        }
    }
    out
}

fn in_rounded_square(x: f32, y: f32, size: f32, radius: f32) -> bool {
    let nearest_x = x.clamp(radius, size - radius);
    let nearest_y = y.clamp(radius, size - radius);
    (x - nearest_x).powi(2) + (y - nearest_y).powi(2) <= radius * radius
}

/// Window icon (title bar, taskbar, Alt+Tab).
pub fn icon_data() -> egui::IconData {
    const SIZE: u32 = 64;
    egui::IconData {
        rgba: rgba(SIZE),
        width: SIZE,
        height: SIZE,
    }
}

/// Windows .ico with one 32-bit BMP image per size (alpha channel, empty AND mask).
#[cfg(test)]
fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    fn u16le(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn u32le(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&size| {
            let px = rgba(size);
            let mask_row = size.div_ceil(32) * 4;
            let mut img = Vec::new();
            // BITMAPINFOHEADER; the height counts colour and AND mask.
            u32le(&mut img, 40);
            u32le(&mut img, size);
            u32le(&mut img, size * 2);
            u16le(&mut img, 1);
            u16le(&mut img, 32);
            u32le(&mut img, 0);
            u32le(&mut img, size * size * 4 + mask_row * size);
            for _ in 0..4 {
                u32le(&mut img, 0);
            }
            // Bottom-up BGRA rows.
            for y in (0..size).rev() {
                for x in 0..size {
                    let i = ((y * size + x) * 4) as usize;
                    img.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
                }
            }
            img.resize(img.len() + (mask_row * size) as usize, 0);
            img
        })
        .collect();

    let mut out = Vec::new();
    u16le(&mut out, 0);
    u16le(&mut out, 1);
    u16le(&mut out, sizes.len() as u16);
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, img) in sizes.iter().zip(&images) {
        let dim = if size >= 256 { 0 } else { size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        u16le(&mut out, 1);
        u16le(&mut out, 32);
        u32le(&mut out, img.len() as u32);
        u32le(&mut out, offset);
        offset += img.len() as u32;
    }
    for img in &images {
        out.extend_from_slice(img);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

    fn pixel(px: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn mark_layout() {
        let size = 64;
        let px = rgba(size);
        assert_eq!(px.len(), (size * size * 4) as usize);
        // Rounded corner is transparent, the tile edge is light blue.
        assert_eq!(pixel(&px, size, 0, 0)[3], 0);
        assert_eq!(pixel(&px, size, 32, 4), [0x4C, 0xC2, 0xFF, 255]);
        // Right half of the circle is filled, the left half shows the tile.
        assert_eq!(pixel(&px, size, 38, 32), [0x0B, 0x0B, 0x0B, 255]);
        assert_eq!(pixel(&px, size, 26, 32), [0x4C, 0xC2, 0xFF, 255]);
    }

    #[test]
    fn ico_directory() {
        let ico = ico_bytes(&ICO_SIZES);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 6, 0]);
        // Entry of the 256 px image stores 0 for its width and height.
        let last = 6 + 16 * 5;
        assert_eq!(&ico[last..last + 2], &[0, 0]);
        let size = u32::from_le_bytes(ico[last + 8..last + 12].try_into().unwrap());
        let offset = u32::from_le_bytes(ico[last + 12..last + 16].try_into().unwrap());
        assert_eq!((offset + size) as usize, ico.len());
    }

    /// Regenerates `data/icons/awayd.ico` (embedded into all three programs by build.rs).
    #[test]
    #[ignore = "writes data/icons/awayd.ico; run on purpose"]
    fn write_app_icon() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/icons/awayd.ico");
        std::fs::write(path, ico_bytes(&ICO_SIZES)).unwrap();
    }
}
