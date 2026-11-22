//! SPDX-License-Identifier: GPL-3.0-or-later
//! Search-first dropdown: open popup, autofocus search field, filter list.

use eframe::egui;

const EMPTY_LABEL: &str = "—";
const MAX_BUTTON_CHARS: usize = 40;

/// Searchable selector backed by `(code, label)` options.
#[derive(Clone, Default)]
pub struct SearchablePicker {
    pub search: String,
    was_open: bool,
}

impl SearchablePicker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dropdown button only (fixed width); returns `Some(code)` when user picks a new value.
    pub fn show_button(
        &mut self,
        ui: &mut egui::Ui,
        id_source: &str,
        selected_code: &str,
        options: &[(String, String)],
        width: f32,
    ) -> Option<String> {
        let raw = Self::display_label(selected_code, options);
        let button_text = truncate_label(raw, MAX_BUTTON_CHARS);
        let popup_id = ui.id().with(id_source).with("popup");

        let response = ui.add_sized(
            egui::vec2(width, ui.spacing().interact_size.y),
            egui::Button::new(button_text),
        );
        if response.clicked() {
            if ui.memory(|m| m.is_popup_open(popup_id)) {
                ui.memory_mut(|m| m.close_popup());
                self.was_open = false;
            } else {
                ui.memory_mut(|m| m.open_popup(popup_id));
                self.search.clear();
                self.was_open = false;
            }
        }

        let picked = self.popup_below(
            ui,
            popup_id,
            &response,
            width,
            selected_code,
            options,
        );

        if !ui.memory(|m| m.is_popup_open(popup_id)) {
            self.was_open = false;
        }

        picked
    }

    fn display_label<'a>(selected_code: &'a str, options: &'a [(String, String)]) -> &'a str {
        if selected_code.is_empty() {
            EMPTY_LABEL
        } else {
            options
                .iter()
                .find(|(c, _)| c == selected_code)
                .map(|(_, l)| l.as_str())
                .unwrap_or(selected_code)
        }
    }

    fn popup_below(
        &mut self,
        ui: &mut egui::Ui,
        popup_id: egui::Id,
        response: &egui::Response,
        width: f32,
        selected_code: &str,
        options: &[(String, String)],
    ) -> Option<String> {
        let search_id = popup_id.with("search");
        egui::popup::popup_below_widget(
            ui,
            popup_id,
            response,
            egui::popup::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(width.max(280.0));
                let just_opened = !self.was_open;
                self.was_open = true;

                if just_opened {
                    ui.memory_mut(|m| m.request_focus(search_id));
                }

                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .id(search_id)
                        .hint_text("Type to search…")
                        .desired_width((width - 8.0).max(260.0)),
                );

                if just_opened {
                    search.request_focus();
                }

                let query = self.search.trim().to_lowercase();
                let filtered: Vec<&(String, String)> = if query.is_empty() {
                    options.iter().collect()
                } else {
                    options
                        .iter()
                        .filter(|(code, label)| {
                            code.to_lowercase().contains(&query)
                                || label.to_lowercase().contains(&query)
                        })
                        .collect()
                };

                ui.separator();

                let select_first = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let mut local_pick: Option<String> = None;

                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        if filtered.is_empty() {
                            ui.label("No matches");
                            return;
                        }
                        for (code, label) in &filtered {
                            let selected = !selected_code.is_empty() && code == selected_code;
                            let row = ui.add(
                                egui::Button::new(label.as_str())
                                    .fill(if selected {
                                        ui.visuals().selection.bg_fill
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    })
                                    .stroke(egui::Stroke::NONE)
                                    .min_size(egui::vec2(ui.available_width(), 0.0)),
                            );
                            if row.clicked() {
                                local_pick = Some((*code).clone());
                                ui.memory_mut(|m| m.close_popup());
                                self.search.clear();
                                self.was_open = false;
                            }
                        }
                        if select_first {
                            local_pick = Some(filtered[0].0.clone());
                            ui.memory_mut(|m| m.close_popup());
                            self.search.clear();
                            self.was_open = false;
                        }
                    });

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    ui.memory_mut(|m| m.close_popup());
                    self.search.clear();
                    self.was_open = false;
                }

                local_pick
            },
        )
        .flatten()
    }
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let trimmed: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{trimmed}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_long_label() {
        let long = "A".repeat(50);
        let t = truncate_label(&long, 40);
        assert!(t.chars().count() <= 40);
        assert!(t.ends_with('…'));
    }
}
