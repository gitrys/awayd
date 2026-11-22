//! SPDX-License-Identifier: GPL-3.0-or-later
//! Tray icons rasterized from `sdss/data/icons/*.svg` (embedded, overridable on disk).

use std::path::Path;
use std::sync::OnceLock;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use sdss_core::config::paths;
use sdss_core::status::StatusSnapshot;
use tray_icon::Icon;

const TRAY_SIZE: u32 = 30;

const SVG_ONLINE: &str = include_str!("../../../../data/icons/sdss-tray-grid.svg");
const SVG_INVISIBLE: &str = include_str!("../../../../data/icons/sdss-tray-grid-invisible.svg");
const SVG_OVERRIDE_ONLINE: &str =
    include_str!("../../../../data/icons/sdss-tray-grid-override-online.svg");
const SVG_OVERRIDE_INVISIBLE: &str =
    include_str!("../../../../data/icons/sdss-tray-grid-override-invisible.svg");

const ICON_FILES: [&str; 4] = [
    "sdss-tray-grid.svg",
    "sdss-tray-grid-invisible.svg",
    "sdss-tray-grid-override-online.svg",
    "sdss-tray-grid-override-invisible.svg",
];

const ICON_EMBED: [&str; 4] = [
    SVG_ONLINE,
    SVG_INVISIBLE,
    SVG_OVERRIDE_ONLINE,
    SVG_OVERRIDE_INVISIBLE,
];

static RGBA_CACHE: OnceLock<[Vec<u8>; 4]> = OnceLock::new();

pub fn icon_off() -> Icon {
    icon_from_index(0)
}

pub fn icon_for_status(s: &StatusSnapshot) -> Icon {
    let override_active = s.source == "manual_override";
    let idx = icon_index(s.work_mode, override_active);
    icon_from_index(idx)
}

fn icon_index(work_mode: bool, override_active: bool) -> usize {
    match (work_mode, override_active) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    }
}

fn icon_from_index(idx: usize) -> Icon {
    let cache = RGBA_CACHE.get_or_init(init_rgba_cache);
    let rgba = cache[idx].clone();
    Icon::from_rgba(rgba, TRAY_SIZE, TRAY_SIZE).expect("tray icon rgba")
}

fn init_rgba_cache() -> [Vec<u8>; 4] {
    std::array::from_fn(|i| load_icon_rgba(i))
}

fn load_icon_rgba(index: usize) -> Vec<u8> {
    let disk = paths::data_dir().join("icons").join(ICON_FILES[index]);
    let svg = read_svg(&disk).unwrap_or_else(|| ICON_EMBED[index].to_string());
    rasterize_svg(&svg).unwrap_or_else(|e| {
        tracing::warn!(
            target: "sdss::tray",
            file = ICON_FILES[index],
            "icon rasterize failed ({e}); using embedded fallback"
        );
        rasterize_svg(ICON_EMBED[index]).expect("embedded tray icon svg")
    })
}

fn read_svg(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn rasterize_svg(svg: &str) -> Result<Vec<u8>, String> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return Err("svg has zero size".into());
    }
    // SVG width/height should match TRAY_SIZE (viewBox 15 -> 30 via width="30").
    let scale_x = TRAY_SIZE as f32 / size.width();
    let scale_y = TRAY_SIZE as f32 / size.height();
    let mut pixmap = Pixmap::new(TRAY_SIZE, TRAY_SIZE).ok_or("pixmap alloc")?;
    resvg::render(
        &tree,
        Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.data().to_vec();
    unpremultiply_alpha(&mut rgba);
    Ok(rgba)
}

fn unpremultiply_alpha(pixels: &mut [u8]) {
    for px in pixels.chunks_mut(4) {
        let a = px[3] as f32 / 255.0;
        if a <= 0.0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            continue;
        }
        if a >= 0.999 {
            continue;
        }
        px[0] = ((px[0] as f32 / a).round() as u8).min(255);
        px[1] = ((px[1] as f32 / a).round() as u8).min(255);
        px[2] = ((px[2] as f32 / a).round() as u8).min(255);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_at(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * TRAY_SIZE + x) * 4) as usize;
        [rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]]
    }

    /// Center of top-right status cell (viewBox 15: cell 8,1 size 6 -> center 11,4).
    const STATUS_X: u32 = 22;
    const STATUS_Y: u32 = 8;
    const OVERRIDE_X: u32 = 8;
    const OVERRIDE_Y: u32 = 8;

    #[test]
    fn tray_is_2x_viewbox() {
        assert_eq!(TRAY_SIZE, 30);
    }

    #[test]
    fn embedded_svgs_rasterize_all_states() {
        for (i, svg) in ICON_EMBED.iter().enumerate() {
            rasterize_svg(svg).unwrap_or_else(|e| panic!("icon {i}: {e}"));
        }
    }

    #[test]
    fn status_cell_green_when_online() {
        let off = rasterize_svg(SVG_ONLINE).expect("online svg");
        let px = rgba_at(&off, STATUS_X, STATUS_Y);
        assert!(
            px[1] > px[0],
            "expected green-ish at status cell, got {px:?}"
        );
    }

    #[test]
    fn status_cell_red_when_invisible() {
        let work = rasterize_svg(SVG_INVISIBLE).expect("invisible svg");
        let px = rgba_at(&work, STATUS_X, STATUS_Y);
        assert!(px[0] > px[1], "expected red-ish at status cell, got {px:?}");
    }

    #[test]
    fn override_fills_other_cells_blue() {
        let ovr = rasterize_svg(SVG_OVERRIDE_INVISIBLE).expect("override svg");
        let status = rgba_at(&ovr, STATUS_X, STATUS_Y);
        let other = rgba_at(&ovr, OVERRIDE_X, OVERRIDE_Y);
        assert!(status[0] > status[1], "status cell red, got {status:?}");
        assert!(other[2] > other[0], "override cell blue, got {other:?}");
    }
}
