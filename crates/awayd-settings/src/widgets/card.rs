//! SPDX-License-Identifier: GPL-3.0-or-later
//! Building blocks of a settings page: page header, cards, setting rows and notices.

use eframe::egui;
use egui::{Align, Layout, RichText, Stroke};

use crate::theme;

/// Page title with a one-line description below it.
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(22.0));
    secondary_label(ui, subtitle);
    ui.add_space(6.0);
}

/// Secondary (grey) text. Uses the palette colour, which is checked for contrast; egui's
/// `weak()` blends towards the background and ends up too light on the cards.
pub fn secondary_label(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    let color = theme::palette(ui.ctx()).text_secondary;
    ui.label(RichText::new(text).size(12.0).color(color))
}

/// A card spanning the available width.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::palette(ui.ctx());
    egui::Frame::none()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.card_border))
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(18.0, 14.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Bold title at the top of a card.
pub fn card_title(ui: &mut egui::Ui, title: &str) {
    ui.label(RichText::new(title).size(14.0).strong());
}

/// Thin line between the rows of a card.
pub fn divider(ui: &mut egui::Ui) {
    let p = theme::palette(ui.ctx());
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, p.divider),
    );
}

/// One setting: name and optional description on the left, the control on the right.
pub fn row(
    ui: &mut egui::Ui,
    label: &str,
    description: Option<&str>,
    control: impl FnOnce(&mut egui::Ui),
) {
    divider(ui);
    // Control first (from the right), then the text in the space that is left, wrapped:
    // laid out the other way round a long description ran under the control.
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            control(ui);
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(label);
                if let Some(d) = description {
                    secondary_label(ui, d);
                }
            });
        });
    });
}

/// Selectable card with a radio mark, title and description (one of several choices).
/// Returns the response of the whole card; `clicked()` selects it.
pub fn choice_card(
    ui: &mut egui::Ui,
    selected: bool,
    title: &str,
    description: &str,
) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let (fill, stroke) = if selected {
        (p.info_bg, Stroke::new(2.0_f32, p.accent))
    } else {
        (p.card, Stroke::new(1.0_f32, p.control_border))
    };
    let inner = egui::Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (mark, _) =
                    ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                let c = mark.center();
                if selected {
                    ui.painter()
                        .circle_stroke(c, 8.0, Stroke::new(2.0_f32, p.accent));
                    ui.painter().circle_filled(c, 4.0, p.accent);
                } else {
                    ui.painter()
                        .circle_stroke(c, 8.0, Stroke::new(1.5_f32, p.text_secondary));
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).strong());
                    secondary_label(ui, description);
                });
            });
        });
    let response = ui.interact(
        inner.response.rect,
        ui.id().with(("choice_card", title)),
        egui::Sense::click(),
    );
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, title)
    });
    if response.has_focus() {
        ui.painter().rect_stroke(
            inner.response.rect.expand(2.0),
            9.0,
            Stroke::new(2.0_f32, p.text),
        );
    }
    response
}

#[derive(Clone, Copy)]
pub enum NoticeKind {
    Info,
    Warning,
    Error,
}

/// Coloured box for hints, warnings and errors; the text inside takes the box's colour.
pub fn notice<R>(ui: &mut egui::Ui, kind: NoticeKind, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = theme::palette(ui.ctx());
    let (fg, bg) = match kind {
        NoticeKind::Info => (p.info_fg, p.info_bg),
        NoticeKind::Warning => (p.warn_fg, p.warn_bg),
        NoticeKind::Error => (p.error_fg, p.error_bg),
    };
    egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, fg.gamma_multiply(0.35)))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.visuals_mut().override_text_color = Some(fg);
            add(ui)
        })
        .inner
}

#[derive(Clone, Copy)]
pub enum PillKind {
    Success,
    Warning,
    Error,
    Neutral,
}

/// Small rounded status label ("Sent", "Skipped", …); the word carries the meaning, the colour
/// only repeats it.
pub fn pill(ui: &mut egui::Ui, kind: PillKind, text: &str) -> egui::Response {
    let p = theme::palette(ui.ctx());
    let (fg, bg) = match kind {
        PillKind::Success => (p.success_fg, p.success_bg),
        PillKind::Warning => (p.warn_fg, p.warn_bg),
        PillKind::Error => (p.error_fg, p.error_bg),
        PillKind::Neutral => (p.text, p.nav_selected),
    };
    egui::Frame::none()
        .fill(bg)
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(9.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).strong().color(fg));
        })
        .response
}

/// A notice that is just one text.
pub fn notice_text(ui: &mut egui::Ui, kind: NoticeKind, text: impl Into<String>) {
    let text = text.into();
    notice(ui, kind, |ui| {
        ui.label(text);
    });
}
