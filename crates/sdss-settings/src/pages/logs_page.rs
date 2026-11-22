//! SPDX-License-Identifier: GPL-3.0-or-later
//! Logs tab: tail file, display-only filters, seamless older chunks.

use eframe::egui;
use sdss_core::config::paths;
use sdss_core::log::{matches_display_filter, LogEntry};
use sdss_core::log_tail::TailReader;

#[inline]
pub fn log_popout_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("sdss_logs_popout")
}

pub const LOG_POPOUT_DEFAULT_SIZE: [f32; 2] = [900.0, 520.0];

pub struct LogViewState {
    pub tail: TailReader,
    pub show_verbose: bool,
    pub show_warning: bool,
    pub show_errors: bool,
    pub search: String,
    pub follow_tail: bool,
    scroll_to_bottom: bool,
}

impl LogViewState {
    pub fn new() -> Self {
        Self {
            tail: TailReader::new(paths::logs_dir()),
            show_verbose: true,
            show_warning: true,
            show_errors: true,
            search: String::new(),
            follow_tail: true,
            scroll_to_bottom: true,
        }
    }

    pub fn poll(&mut self) {
        if self.tail.poll_tail() > 0 && self.follow_tail {
            self.scroll_to_bottom = true;
        }
    }
}

fn level_color(level: &str, visuals: &egui::Visuals) -> egui::Color32 {
    match level.to_ascii_uppercase().as_str() {
        "ERROR" => egui::Color32::from_rgb(220, 80, 80),
        "WARN" | "WARNING" => egui::Color32::from_rgb(220, 180, 60),
        _ => visuals.text_color(),
    }
}

fn format_row(entry: &LogEntry) -> String {
    let ts_short = entry.ts.split('T').nth(1).unwrap_or(&entry.ts);
    let mut fields = String::new();
    for (k, v) in &entry.fields {
        if !fields.is_empty() {
            fields.push(' ');
        }
        fields.push_str(&format!("{k}={v}"));
    }
    if fields.is_empty() {
        format!(
            "{ts_short}  {:5}  {:<18} {}",
            entry.level, entry.target, entry.message
        )
    } else {
        format!(
            "{ts_short}  {:5}  {:<18} {}  {fields}",
            entry.level, entry.target, entry.message
        )
    }
}

fn filtered_rows(state: &LogViewState, visuals: &egui::Visuals) -> Vec<(String, egui::Color32)> {
    state
        .tail
        .entries
        .iter()
        .filter(|e| {
            matches_display_filter(
                e,
                state.show_verbose,
                state.show_warning,
                state.show_errors,
                &state.search,
            )
        })
        .map(|e| (format_row(e), level_color(&e.level, visuals)))
        .collect()
}

pub fn show_logs_controls(ui: &mut egui::Ui, state: &mut LogViewState, daemon_ok: bool) {
    ui.label(
        egui::RichText::new("Filters hide lines in the viewer only; full history stays in state/logs/.")
            .weak()
            .small(),
    );
    if !daemon_ok {
        ui.label(
            egui::RichText::new("Daemon offline — showing saved log files only.")
                .color(egui::Color32::YELLOW),
        );
    }

    ui.horizontal(|ui| {
        ui.checkbox(&mut state.show_verbose, "Verbose");
        ui.checkbox(&mut state.show_warning, "Warning");
        ui.checkbox(&mut state.show_errors, "Errors");
        ui.separator();
        ui.checkbox(&mut state.follow_tail, "Follow tail");
    });

    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text("e.g. sent, discord, steam")
                .desired_width(ui.available_width() - 60.0),
        );
    });

    let rows = filtered_rows(state, ui.visuals());
    ui.label(format!(
        "Showing {} of {} loaded",
        rows.len(),
        state.tail.entries.len()
    ));
}

pub fn show_logs_list(ui: &mut egui::Ui, state: &mut LogViewState) {
    if state.tail.entries.is_empty() && !paths::current_log_file().is_file() {
        ui.label("Waiting for daemon logs…");
        return;
    }

    let rows = filtered_rows(state, ui.visuals());
    let has_older = state.tail.has_older();
    let mut load_older = false;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(state.follow_tail && state.scroll_to_bottom)
        .show(ui, |ui| {
            if has_older {
                let at_top = ui.clip_rect().top() <= ui.min_rect().top() + 4.0;
                if at_top || ui.button("↑ Load older entries").clicked() {
                    load_older = true;
                }
            }

            for (line, color) in &rows {
                ui.label(egui::RichText::new(line).monospace().color(*color));
            }
        });

    if load_older {
        let n = state.tail.prepend_older_chunk();
        if n > 0 {
            state.follow_tail = false;
        }
    }

    if state.scroll_to_bottom {
        state.scroll_to_bottom = false;
    }
}

pub fn show_logs_full(ui: &mut egui::Ui, state: &mut LogViewState, daemon_ok: bool) {
    show_logs_controls(ui, state, daemon_ok);
    show_logs_list(ui, state);
}

pub fn show_logs_embedded_tab(
    ui: &mut egui::Ui,
    state: &mut LogViewState,
    daemon_ok: bool,
    popout_open: &mut bool,
) {
    ui.horizontal(|ui| {
        if ui.button("Pop out logs").clicked() && !*popout_open {
            *popout_open = true;
        }
    });
    ui.separator();
    show_logs_full(ui, state, daemon_ok);
}

pub fn show_logs_docked_placeholder(ui: &mut egui::Ui, dock: &mut bool) {
    ui.label(
        egui::RichText::new("Logs are shown in a separate window.")
            .weak(),
    );
    if ui.button("Dock logs").clicked() {
        *dock = true;
    }
}
