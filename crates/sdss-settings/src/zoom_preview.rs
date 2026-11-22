//! SPDX-License-Identifier: GPL-3.0-or-later
//! Inline typography sample for zoom preview while dragging.

use std::sync::Arc;

use eframe::egui;

/// Clone `base` and scale typography/spacing to match `pending_zoom`.
pub fn scaled_preview_style(base: &egui::Style, pending_zoom: f32) -> egui::Style {
    let mut style = base.clone();
    for font_id in style.text_styles.values_mut() {
        font_id.size *= pending_zoom;
    }
    style.spacing.interact_size *= pending_zoom;
    style.spacing.button_padding *= pending_zoom;
    style.spacing.slider_width *= pending_zoom;
    style.spacing.indent *= pending_zoom;
    style
}

pub fn show_typography_sample(ui: &mut egui::Ui, pending_zoom: f32) {
    let pending_pct = (pending_zoom * 100.0).round() as i32;
    ui.label(
        egui::RichText::new(format!("Preview at {pending_pct}%"))
            .weak()
            .small(),
    );
    ui.scope(|ui| {
        ui.set_style(Arc::new(scaled_preview_style(ui.style(), pending_zoom)));
        ui.heading("Scheduler");
        ui.label("Country · 09:00 – 17:00 · Schedule · Mode");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_preview_style_scales_body_font() {
        let base = egui::Style::default();
        let base_size = egui::TextStyle::Body.resolve(&base).size;
        let scaled = scaled_preview_style(&base, 1.25);
        let scaled_size = egui::TextStyle::Body.resolve(&scaled).size;
        assert!((scaled_size - base_size * 1.25).abs() < 0.01);
    }

    #[test]
    fn scaled_preview_style_scales_interact_size() {
        let base = egui::Style::default();
        let scaled = scaled_preview_style(&base, 0.75);
        assert!(
            (scaled.spacing.interact_size.y - base.spacing.interact_size.y * 0.75).abs() < 0.01
        );
    }
}
