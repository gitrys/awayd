//! SPDX-License-Identifier: GPL-3.0-or-later
//! Logs page: tail file, display-only filters, seamless older chunks.

use awayd_core::config::paths;
use awayd_core::log::{matches_display_filter, DisplayBucket, LogEntry};
use awayd_core::log_tail::TailReader;
use eframe::egui;

use crate::theme;

#[inline]
pub fn log_popout_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("awayd_logs_popout")
}

pub const LOG_POPOUT_DEFAULT_SIZE: [f32; 2] = [900.0, 520.0];

const TIME_W: f32 = 92.0;
const LEVEL_W: f32 = 50.0;
const TARGET_W: f32 = 150.0;

pub struct LogViewState {
    pub tail: TailReader,
    /// DEBUG and TRACE lines.
    pub show_verbose: bool,
    pub show_info: bool,
    pub show_warning: bool,
    pub show_errors: bool,
    pub search: String,
    /// Keep the view at the newest line while it is at the bottom.
    pub follow_tail: bool,
    /// Jump to the newest line once (first show, or "Follow new lines" switched on again).
    scroll_to_bottom: bool,
}

impl LogViewState {
    pub fn new() -> Self {
        Self {
            tail: TailReader::new(paths::logs_dir()),
            show_verbose: false,
            show_info: true,
            show_warning: true,
            show_errors: true,
            search: String::new(),
            follow_tail: true,
            scroll_to_bottom: true,
        }
    }

    pub fn poll(&mut self) {
        self.tail.poll_tail();
    }

    fn visible(&self, e: &LogEntry) -> bool {
        // INFO has its own switch here; the shared filter counts it as verbose.
        let level_ok = if e.level.eq_ignore_ascii_case("INFO") {
            self.show_info
        } else {
            match e.level_bucket() {
                DisplayBucket::Verbose => self.show_verbose,
                DisplayBucket::Warning => self.show_warning,
                DisplayBucket::Errors => self.show_errors,
            }
        };
        level_ok && matches_display_filter(e, true, true, true, &self.search)
    }
}

fn time_of(entry: &LogEntry) -> &str {
    // "HH:MM:SS.mmm" from "2026-11-22T17:59:21.311+01:00" (older lines have no offset).
    let time = entry.ts.split('T').nth(1).unwrap_or(&entry.ts);
    time.get(..12).unwrap_or(time)
}

fn message_of(entry: &LogEntry) -> String {
    let mut text = entry.message.clone();
    for (k, v) in &entry.fields {
        text.push_str(&format!("  {k}={v}"));
    }
    text
}

pub fn show_logs_controls(ui: &mut egui::Ui, state: &mut LogViewState, daemon_ok: bool) {
    let p = theme::palette(ui.ctx());
    egui::Frame::none()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0_f32, p.card_border))
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(14.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Show").color(p.text_secondary));
                ui.checkbox(&mut state.show_verbose, "Verbose")
                    .on_hover_text("DEBUG and TRACE lines");
                ui.checkbox(&mut state.show_info, "Info");
                ui.checkbox(&mut state.show_warning, "Warnings");
                ui.checkbox(&mut state.show_errors, "Errors");
                ui.separator();
                if ui
                    .checkbox(&mut state.follow_tail, "Follow new lines")
                    .changed()
                    && state.follow_tail
                {
                    state.scroll_to_bottom = true;
                }
                ui.separator();
                ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .hint_text("Search, e.g. sent, discord, steam")
                        .desired_width(220.0),
                );
            });
        });
    if !daemon_ok {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "Daemon offline — showing saved log files only.",
        );
    }
}

/// One fixed-width column of a log row.
fn cell(ui: &mut egui::Ui, width: f32, text: egui::RichText) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            ui.set_min_width(width);
            ui.add(egui::Label::new(text).truncate());
        },
    );
}

pub fn show_logs_list(ui: &mut egui::Ui, state: &mut LogViewState) {
    if state.tail.entries.is_empty() && !paths::current_log_file().is_file() {
        ui.label("Waiting for daemon logs…");
        return;
    }
    let p = theme::palette(ui.ctx());
    let shown = state
        .tail
        .entries
        .iter()
        .filter(|e| state.visible(e))
        .count();
    ui.label(
        egui::RichText::new(format!(
            "Showing {shown} of {} loaded · filters only hide lines here; the files in state\\logs\\ keep everything",
            state.tail.entries.len()
        ))
        .size(12.0)
        .color(p.text_secondary),
    );

    let has_older = state.tail.has_older();
    let mut load_older = false;
    let jump = std::mem::take(&mut state.scroll_to_bottom);

    egui::Frame::none()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0_f32, p.card_border))
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(state.follow_tail)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if has_older {
                        let at_top = ui.clip_rect().top() <= ui.min_rect().top() + 4.0;
                        if at_top || ui.button("Load older entries").clicked() {
                            load_older = true;
                        }
                    }
                    for e in state.tail.entries.iter().filter(|e| state.visible(e)) {
                        let (level_color, fill) = match e.level_bucket() {
                            DisplayBucket::Errors => (p.error_fg, p.error_bg),
                            DisplayBucket::Warning => (p.warn_fg, p.warn_bg),
                            DisplayBucket::Verbose => (p.text, egui::Color32::TRANSPARENT),
                        };
                        egui::Frame::none().fill(fill).rounding(3.0).show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                let mono = |t: &str| egui::RichText::new(t).monospace();
                                cell(ui, TIME_W, mono(time_of(e)).color(p.text_secondary));
                                cell(ui, LEVEL_W, mono(&e.level).color(level_color).strong());
                                cell(ui, TARGET_W, mono(&e.target).color(p.text_secondary));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(message_of(e)).monospace(),
                                    )
                                    .wrap(),
                                );
                            });
                        });
                    }
                    if jump {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    }
                });
        });

    if load_older {
        let n = state.tail.prepend_older_chunk();
        if n > 0 {
            state.follow_tail = false;
        }
    }
}

pub fn show_logs_full(ui: &mut egui::Ui, state: &mut LogViewState, daemon_ok: bool) {
    show_logs_controls(ui, state, daemon_ok);
    ui.add_space(6.0);
    show_logs_list(ui, state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: &str, message: &str) -> LogEntry {
        LogEntry {
            v: 1,
            ts: "2026-11-23T01:15:14.000+01:00".into(),
            level: level.into(),
            target: "awayd".into(),
            message: message.into(),
            fields: Default::default(),
        }
    }

    #[test]
    fn info_and_debug_have_separate_switches() {
        let mut s = LogViewState::new();
        assert!(s.visible(&entry("INFO", "presence sent")));
        assert!(!s.visible(&entry("DEBUG", "connecting")));
        s.show_info = false;
        s.show_verbose = true;
        assert!(!s.visible(&entry("INFO", "presence sent")));
        assert!(s.visible(&entry("DEBUG", "connecting")));
        assert!(s.visible(&entry("WARN", "slow")));
    }

    #[test]
    fn search_still_filters() {
        let mut s = LogViewState::new();
        s.search = "steam".into();
        assert!(s.visible(&entry("INFO", "steam sent")));
        assert!(!s.visible(&entry("INFO", "discord skipped")));
    }

    #[test]
    fn time_column_is_cut_from_the_timestamp() {
        assert_eq!(time_of(&entry("INFO", "x")), "01:15:14.000");
    }
}

pub fn show_logs_docked_placeholder(ui: &mut egui::Ui, dock: &mut bool) {
    crate::widgets::card::secondary_label(ui, "Logs are shown in a separate window.");
    if ui.button("Dock logs").clicked() {
        *dock = true;
    }
}
