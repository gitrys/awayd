//! SPDX-License-Identifier: GPL-3.0-or-later
//! Look of the settings window: light and dark palettes in the style of the Windows 11
//! settings app, egui style per theme, Segoe UI when Windows has it.

use eframe::egui;
use egui::{Color32, FontFamily, FontId, Rounding, Stroke, TextStyle, Theme};
use serde::{Deserialize, Serialize};

/// Colours of one theme. Text colours are checked against their backgrounds in the tests.
pub struct Palette {
    /// Content area behind the cards.
    pub window: Color32,
    /// Sidebar and status bar.
    pub sidebar: Color32,
    pub card: Color32,
    pub card_border: Color32,
    pub divider: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub accent: Color32,
    /// Text and marks drawn on `accent`.
    pub on_accent: Color32,
    pub link: Color32,
    /// Buttons and text fields at rest.
    pub control: Color32,
    pub control_hover: Color32,
    pub control_pressed: Color32,
    pub control_border: Color32,
    pub nav_selected: Color32,
    pub nav_hover: Color32,
    /// Text selection and selected list rows.
    pub selection: Color32,
    pub success_fg: Color32,
    pub success_bg: Color32,
    pub warn_fg: Color32,
    pub warn_bg: Color32,
    pub error_fg: Color32,
    pub error_bg: Color32,
    pub info_fg: Color32,
    pub info_bg: Color32,
}

pub static LIGHT: Palette = Palette {
    window: Color32::from_rgb(0xF3, 0xF3, 0xF3),
    sidebar: Color32::from_rgb(0xEE, 0xEE, 0xEE),
    card: Color32::from_rgb(0xFB, 0xFB, 0xFB),
    card_border: Color32::from_rgb(0xE5, 0xE5, 0xE5),
    divider: Color32::from_rgb(0xE8, 0xE8, 0xE8),
    text: Color32::from_rgb(0x1A, 0x1A, 0x1A),
    text_secondary: Color32::from_rgb(0x5D, 0x5D, 0x5D),
    accent: Color32::from_rgb(0x00, 0x5F, 0xB8),
    on_accent: Color32::WHITE,
    link: Color32::from_rgb(0x00, 0x5F, 0xB8),
    control: Color32::WHITE,
    control_hover: Color32::from_rgb(0xF5, 0xF5, 0xF5),
    control_pressed: Color32::from_rgb(0xEB, 0xEB, 0xEB),
    control_border: Color32::from_rgb(0xD1, 0xD1, 0xD1),
    nav_selected: Color32::from_rgb(0xE0, 0xE0, 0xE0),
    nav_hover: Color32::from_rgb(0xE6, 0xE6, 0xE6),
    selection: Color32::from_rgb(0xCC, 0xE4, 0xF7),
    success_fg: Color32::from_rgb(0x0E, 0x5C, 0x0E),
    success_bg: Color32::from_rgb(0xDF, 0xF6, 0xDD),
    warn_fg: Color32::from_rgb(0x5C, 0x39, 0x00),
    warn_bg: Color32::from_rgb(0xFF, 0xF4, 0xCE),
    error_fg: Color32::from_rgb(0xA4, 0x26, 0x2C),
    error_bg: Color32::from_rgb(0xFD, 0xE7, 0xE9),
    info_fg: Color32::from_rgb(0x12, 0x3A, 0x5E),
    info_bg: Color32::from_rgb(0xEA, 0xF3, 0xFC),
};

pub static DARK: Palette = Palette {
    window: Color32::from_rgb(0x20, 0x20, 0x20),
    sidebar: Color32::from_rgb(0x1C, 0x1C, 0x1C),
    card: Color32::from_rgb(0x2B, 0x2B, 0x2B),
    card_border: Color32::from_rgb(0x38, 0x38, 0x38),
    divider: Color32::from_rgb(0x36, 0x36, 0x36),
    text: Color32::WHITE,
    text_secondary: Color32::from_rgb(0xAB, 0xAB, 0xAB),
    accent: Color32::from_rgb(0x4C, 0xC2, 0xFF),
    on_accent: Color32::BLACK,
    link: Color32::from_rgb(0x99, 0xD9, 0xFF),
    control: Color32::from_rgb(0x2D, 0x2D, 0x2D),
    control_hover: Color32::from_rgb(0x32, 0x32, 0x32),
    control_pressed: Color32::from_rgb(0x3A, 0x3A, 0x3A),
    control_border: Color32::from_rgb(0x45, 0x45, 0x45),
    nav_selected: Color32::from_rgb(0x2D, 0x2D, 0x2D),
    nav_hover: Color32::from_rgb(0x28, 0x28, 0x28),
    selection: Color32::from_rgb(0x1A, 0x3A, 0x52),
    success_fg: Color32::from_rgb(0x8F, 0xE0, 0x8A),
    success_bg: Color32::from_rgb(0x1F, 0x3A, 0x1F),
    warn_fg: Color32::from_rgb(0xF7, 0xD7, 0x74),
    warn_bg: Color32::from_rgb(0x3D, 0x31, 0x15),
    error_fg: Color32::from_rgb(0xFF, 0x99, 0xA4),
    error_bg: Color32::from_rgb(0x44, 0x27, 0x26),
    info_fg: Color32::from_rgb(0xCF, 0xE8, 0xFA),
    info_bg: Color32::from_rgb(0x1A, 0x2D, 0x3B),
};

/// Palette of the theme the window shows right now.
pub fn palette(ctx: &egui::Context) -> &'static Palette {
    match ctx.theme() {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// Theme chosen in Settings → General (stored in `settings_ui.json`, not in `config.toml`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    /// Follow the Windows app theme.
    #[default]
    System,
    Light,
    Dark,
}

impl From<ThemeChoice> for egui::ThemePreference {
    fn from(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        }
    }
}

/// Install fonts and both styles once at startup, then select `choice`.
pub fn install(ctx: &egui::Context, choice: ThemeChoice) {
    install_fonts(ctx);
    ctx.set_style_of(Theme::Light, style(&LIGHT, false));
    ctx.set_style_of(Theme::Dark, style(&DARK, true));
    ctx.set_theme(choice);
}

/// Segoe UI from the Windows font folder in front of egui's fonts (which still provide the
/// symbols Segoe lacks). Without the file egui's own font is used.
fn install_fonts(ctx: &egui::Context) {
    let Some(dir) = std::env::var_os("WINDIR") else {
        return;
    };
    let path = std::path::Path::new(&dir).join("Fonts").join("segoeui.ttf");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            tracing::info!(path = %path.display(), "Segoe UI not loaded, using egui font: {e}");
            return;
        }
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("segoe-ui".into(), egui::FontData::from_owned(bytes));
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "segoe-ui".into());
    }
    ctx.set_fonts(fonts);
}

fn style(p: &Palette, dark: bool) -> egui::Style {
    let mut style = egui::Style::default();

    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(20.0)),
        (TextStyle::Body, FontId::proportional(13.0)),
        (TextStyle::Button, FontId::proportional(13.0)),
        (TextStyle::Small, FontId::proportional(11.5)),
        (TextStyle::Monospace, FontId::monospace(12.0)),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(10.0, 4.0);
    s.interact_size = egui::vec2(40.0, 28.0);
    // Always-visible scroll bars: the floating default only appears on hover, so a page
    // taller than the window looked cut off.
    s.scroll = egui::style::ScrollStyle::solid();
    s.icon_width = 18.0;
    s.icon_width_inner = 10.0;
    s.icon_spacing = 8.0;
    s.combo_height = 320.0;
    s.menu_margin = egui::Margin::same(6.0);
    s.window_margin = egui::Margin::same(12.0);

    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.link;
    v.panel_fill = p.window;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0_f32, p.card_border);
    v.window_rounding = Rounding::same(8.0);
    v.menu_rounding = Rounding::same(6.0);
    v.extreme_bg_color = p.control;
    v.faint_bg_color = p.divider;
    v.code_bg_color = p.control_pressed;
    v.warn_fg_color = p.warn_fg;
    v.error_fg_color = p.error_fg;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.5_f32, p.accent);
    v.slider_trailing_fill = true;

    let control = |fill: Color32| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0_f32, p.control_border),
        rounding: Rounding::same(4.0),
        fg_stroke: Stroke::new(1.0_f32, p.text),
        expansion: 0.0,
    };
    v.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: p.card,
        weak_bg_fill: p.card,
        bg_stroke: Stroke::new(1.0_f32, p.divider),
        rounding: Rounding::same(4.0),
        fg_stroke: Stroke::new(1.0_f32, p.text),
        expansion: 0.0,
    };
    v.widgets.inactive = control(p.control);
    v.widgets.hovered = control(p.control_hover);
    v.widgets.active = control(p.control_pressed);
    v.widgets.open = control(p.control_hover);

    style.visuals = v;
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(c: u8) -> f64 {
        let c = c as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(c: Color32) -> f64 {
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    /// WCAG contrast ratio.
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    fn assert_readable(name: &str, fg: Color32, bg: Color32) {
        let c = contrast(fg, bg);
        assert!(c >= 4.5, "{name}: contrast {c:.2} < 4.5");
    }

    #[test]
    fn text_is_readable_in_both_themes() {
        for (theme, p) in [("light", &LIGHT), ("dark", &DARK)] {
            for (bg_name, bg) in [
                ("window", p.window),
                ("sidebar", p.sidebar),
                ("card", p.card),
                ("control", p.control),
                ("nav_selected", p.nav_selected),
            ] {
                assert_readable(&format!("{theme} text on {bg_name}"), p.text, bg);
                assert_readable(
                    &format!("{theme} secondary on {bg_name}"),
                    p.text_secondary,
                    bg,
                );
            }
            assert_readable(&format!("{theme} link on card"), p.link, p.card);
            assert_readable(&format!("{theme} on_accent"), p.on_accent, p.accent);
            assert_readable(&format!("{theme} text on selection"), p.text, p.selection);
            assert_readable(&format!("{theme} success"), p.success_fg, p.success_bg);
            assert_readable(&format!("{theme} warning"), p.warn_fg, p.warn_bg);
            assert_readable(&format!("{theme} error"), p.error_fg, p.error_bg);
            assert_readable(&format!("{theme} info"), p.info_fg, p.info_bg);
            // Status texts are also drawn without their background (status bar, log view).
            assert_readable(&format!("{theme} error on card"), p.error_fg, p.card);
            assert_readable(&format!("{theme} warning on card"), p.warn_fg, p.card);
            assert_readable(
                &format!("{theme} success on sidebar"),
                p.success_fg,
                p.sidebar,
            );
        }
    }

    #[test]
    fn theme_choice_maps_to_egui_preference() {
        assert_eq!(
            egui::ThemePreference::from(ThemeChoice::System),
            egui::ThemePreference::System
        );
        assert_eq!(
            egui::ThemePreference::from(ThemeChoice::Dark),
            egui::ThemePreference::Dark
        );
    }

    #[test]
    fn installed_choice_selects_the_palette() {
        let ctx = egui::Context::default();
        install(&ctx, ThemeChoice::Light);
        assert!(std::ptr::eq(palette(&ctx), &LIGHT));
        ctx.set_theme(ThemeChoice::Dark);
        assert!(std::ptr::eq(palette(&ctx), &DARK));
        assert_eq!(ctx.style().visuals.panel_fill, DARK.window);
    }
}
