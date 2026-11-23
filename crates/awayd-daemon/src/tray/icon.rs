//! SPDX-License-Identifier: GPL-3.0-or-later
//! Tray icon: the app mark as a one-colour glyph whose shape carries the state (ring =
//! online, half-filled = invisible, dashed = status unknown), with a small badge for an
//! override (accent dot) or a warning (yellow triangle). White on a dark taskbar, black on a
//! light one. Each combination is rasterized once per taskbar theme and icon size and cached,
//! so a status change costs no drawing.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use awayd_core::status::{StatusSnapshot, SOURCE_ERROR};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use tray_icon::Icon;

/// What the icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Glyph {
    /// Invisible (at work): right half filled.
    pub half: bool,
    /// Status unknown (failed tick): dashed ring.
    pub dashed: bool,
    pub override_dot: bool,
    /// Warning or error; takes the badge place of the override dot.
    pub warning: bool,
}

impl Glyph {
    pub fn for_status(s: &StatusSnapshot) -> Self {
        if s.source == SOURCE_ERROR {
            return Self {
                half: false,
                dashed: true,
                override_dot: false,
                warning: true,
            };
        }
        let warning = !s.ok;
        Self {
            half: s.work_mode,
            dashed: false,
            override_dot: s.source == "manual_override" && !warning,
            warning,
        }
    }

    pub const ONLINE: Glyph = Glyph {
        half: false,
        dashed: false,
        override_dot: false,
        warning: false,
    };
}

struct Colors {
    fg: &'static str,
    accent: &'static str,
}

const ON_DARK: Colors = Colors {
    fg: "#FFFFFF",
    accent: "#4CC2FF",
};
const ON_LIGHT: Colors = Colors {
    fg: "#1A1A1A",
    accent: "#005FB8",
};

/// The glyph as SVG (16 × 16 design grid). Badges are cut out of the glyph with a mask, so
/// they stand free on any taskbar colour.
fn svg(g: Glyph, light_taskbar: bool, size: u32) -> String {
    let c = if light_taskbar { &ON_LIGHT } else { &ON_DARK };
    let dash = if g.dashed {
        r#" stroke-dasharray="2.3 1.6""#
    } else {
        ""
    };
    let half = if g.half {
        format!(
            r#"<path d="M8 2.4 A5.6 5.6 0 0 1 8 13.6 Z" fill="{}"/>"#,
            c.fg
        )
    } else {
        String::new()
    };
    let (cut, badge) = if g.warning {
        (
            r#"<path d="M12.5 7.2 L17.4 16.4 L7.6 16.4 Z" fill="black"/>"#.to_string(),
            r##"<path d="M12.5 8.6 L16 15 L9 15 Z" fill="#FFB900"/><path d="M12.5 10.6 V12.6 M12.5 13.6 V13.7" stroke="#1A1A1A" stroke-width="1.2" stroke-linecap="round"/>"##.to_string(),
        )
    } else if g.override_dot {
        (
            r#"<circle cx="12.8" cy="12.8" r="3.4" fill="black"/>"#.to_string(),
            format!(
                r#"<circle cx="12.8" cy="12.8" r="2.4" fill="{}"/>"#,
                c.accent
            ),
        )
    } else {
        (String::new(), String::new())
    };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 16 16"><defs><mask id="m"><rect width="16" height="16" fill="white"/>{cut}</mask></defs><g mask="url(#m)"><circle cx="8" cy="8" r="5.6" fill="none" stroke="{fg}" stroke-width="1.6"{dash}/>{half}</g>{badge}</svg>"#,
        fg = c.fg
    )
}

type Key = (Glyph, bool, u32);
static CACHE: OnceLock<Mutex<HashMap<Key, Vec<u8>>>> = OnceLock::new();

/// Icon for `glyph` on the current taskbar at the system's small-icon size.
pub fn icon(glyph: Glyph) -> Icon {
    let light = crate::platform::windows::theme::taskbar_uses_light_theme();
    let size = crate::platform::windows::theme::small_icon_size();
    let key = (glyph, light, size);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let rgba = {
        let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(key)
            .or_insert_with(|| rasterize(&svg(glyph, light, size), size))
            .clone()
    };
    Icon::from_rgba(rgba, size, size).expect("tray icon rgba")
}

pub fn icon_for_status(s: &StatusSnapshot) -> Icon {
    icon(Glyph::for_status(s))
}

fn rasterize(svg: &str, size: u32) -> Vec<u8> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).expect("built-in tray svg");
    let scale = size as f32 / tree.size().width();
    let mut pixmap = Pixmap::new(size, size).expect("pixmap");
    resvg::render(
        &tree,
        Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.data().to_vec();
    unpremultiply_alpha(&mut rgba);
    rgba
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
        px[0] = (px[0] as f32 / a).round().min(255.0) as u8;
        px[1] = (px[1] as f32 / a).round().min(255.0) as u8;
        px[2] = (px[2] as f32 / a).round().min(255.0) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u32 = 32;

    fn px(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * S + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    fn draw(g: Glyph, light: bool) -> Vec<u8> {
        rasterize(&svg(g, light, S), S)
    }

    fn snapshot(ok: bool, work_mode: bool, source: &str) -> StatusSnapshot {
        StatusSnapshot {
            ok,
            work_mode,
            source: source.into(),
            ..StatusSnapshot::default()
        }
    }

    #[test]
    fn state_mapping() {
        assert_eq!(
            Glyph::for_status(&snapshot(true, false, "schedule")),
            Glyph::ONLINE
        );
        let inv = Glyph::for_status(&snapshot(true, true, "schedule"));
        assert!(inv.half && !inv.override_dot && !inv.warning);
        let ovr = Glyph::for_status(&snapshot(true, false, "manual_override"));
        assert!(ovr.override_dot && !ovr.half);
        // A warning (e.g. no Discord token) keeps the state and shows the triangle.
        let warn = Glyph::for_status(&snapshot(false, true, "schedule"));
        assert!(warn.half && warn.warning);
        let err = Glyph::for_status(&snapshot(false, false, SOURCE_ERROR));
        assert!(err.dashed && err.warning);
    }

    #[test]
    fn online_ring_is_hollow_invisible_is_half_filled() {
        let online = draw(Glyph::ONLINE, false);
        let inv = draw(
            Glyph {
                half: true,
                ..Glyph::ONLINE
            },
            false,
        );
        // Centre right of the circle: empty for online, white for invisible.
        assert_eq!(px(&online, 20, 16)[3], 0);
        assert_eq!(px(&inv, 20, 16), [255, 255, 255, 255]);
        // Centre left stays empty in both.
        assert_eq!(px(&inv, 12, 16)[3], 0);
        // Ring itself (left edge at x = 16 - 11.2).
        assert!(px(&online, 5, 16)[3] > 200);
    }

    #[test]
    fn light_taskbar_draws_dark() {
        let g = draw(
            Glyph {
                half: true,
                ..Glyph::ONLINE
            },
            true,
        );
        assert_eq!(px(&g, 20, 16), [0x1A, 0x1A, 0x1A, 255]);
    }

    #[test]
    fn badges_have_their_colours() {
        let ovr = draw(
            Glyph {
                override_dot: true,
                ..Glyph::ONLINE
            },
            false,
        );
        assert_eq!(px(&ovr, 25, 25), [0x4C, 0xC2, 0xFF, 255]);
        let warn = draw(
            Glyph {
                warning: true,
                ..Glyph::ONLINE
            },
            false,
        );
        let p = px(&warn, 22, 28);
        assert_eq!((p[0], p[1], p[3]), (0xFF, 0xB9, 255), "{p:?}");
    }

    /// Regenerates `docs/images/tray-icons.png` for the README (all states on a dark and a
    /// light taskbar, drawn by the same code as the tray):
    /// `cargo test -p awayd-daemon write_tray_icons_png -- --ignored`.
    #[test]
    #[ignore = "writes docs/images/tray-icons.png; run on purpose"]
    fn write_tray_icons_png() {
        const ICON: u32 = 48;
        const GAP: u32 = 28;
        let states = [
            Glyph::ONLINE,
            Glyph {
                half: true,
                ..Glyph::ONLINE
            },
            Glyph {
                override_dot: true,
                ..Glyph::ONLINE
            },
            Glyph {
                half: true,
                override_dot: true,
                ..Glyph::ONLINE
            },
            Glyph {
                half: true,
                warning: true,
                ..Glyph::ONLINE
            },
            Glyph {
                dashed: true,
                warning: true,
                ..Glyph::ONLINE
            },
        ];
        let width = states.len() as u32 * (ICON + GAP) + GAP;
        let row = ICON + 2 * GAP;
        let mut sheet = Pixmap::new(width, 2 * row).unwrap();
        for (r, (light, bg)) in [(false, [0x1C, 0x1C, 0x1C]), (true, [0xEE, 0xEE, 0xEE])]
            .into_iter()
            .enumerate()
        {
            let rect = resvg::tiny_skia::Rect::from_xywh(
                0.0,
                (r as u32 * row) as f32,
                width as f32,
                row as f32,
            )
            .unwrap();
            let mut paint = resvg::tiny_skia::Paint::default();
            paint.set_color_rgba8(bg[0], bg[1], bg[2], 255);
            sheet.fill_rect(rect, &paint, Transform::identity(), None);
            for (i, g) in states.iter().enumerate() {
                let tree =
                    usvg::Tree::from_str(&svg(*g, light, ICON), &usvg::Options::default()).unwrap();
                let scale = ICON as f32 / tree.size().width();
                let x = (GAP + i as u32 * (ICON + GAP)) as f32;
                let y = (r as u32 * row + GAP) as f32;
                resvg::render(
                    &tree,
                    Transform::from_translate(x, y).pre_scale(scale, scale),
                    &mut sheet.as_mut(),
                );
            }
        }
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/images/tray-icons.png"
        );
        sheet.save_png(path).unwrap();
    }

    #[test]
    fn every_combination_rasterizes() {
        for bits in 0..16u8 {
            let g = Glyph {
                half: bits & 1 != 0,
                dashed: bits & 2 != 0,
                override_dot: bits & 4 != 0,
                warning: bits & 8 != 0,
            };
            for light in [false, true] {
                for size in [16, 20, 24, 32] {
                    assert_eq!(
                        rasterize(&svg(g, light, size), size).len(),
                        (size * size * 4) as usize
                    );
                }
            }
        }
    }
}
