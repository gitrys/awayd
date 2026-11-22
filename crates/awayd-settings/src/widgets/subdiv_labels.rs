//! SPDX-License-Identifier: GPL-3.0-or-later
//! ISO 3166-2 English subdivision names for picker labels.

use std::collections::HashMap;
use std::sync::OnceLock;

/// One subdivision row in the picker.
#[derive(Clone, Debug)]
pub struct SubdivOption {
    pub code: String,
    pub label: String,
}

/// English name if known.
pub fn subdiv_name(country: &str, code: &str) -> Option<&'static str> {
    let cc = country.trim().to_uppercase();
    let sc = code.trim().to_uppercase();
    if sc.is_empty() {
        return None;
    }
    labels_table().get(&cc)?.get(&sc).map(String::as_str)
}

/// Picker label: `Bavaria (BY)` or code only.
pub fn format_subdiv_label(country: &str, code: &str) -> String {
    let c = code.trim().to_uppercase();
    if c.is_empty() {
        return "—".to_string();
    }
    if let Some(name) = subdiv_name(country, &c) {
        format!("{name} ({c})")
    } else {
        c
    }
}

/// Build sorted picker options for codes from the holiday cache.
pub fn subdiv_options_for_country(country: &str, codes: &[String]) -> Vec<SubdivOption> {
    let mut opts: Vec<SubdivOption> = codes
        .iter()
        .map(|code| SubdivOption {
            code: code.clone(),
            label: format_subdiv_label(country, code),
        })
        .collect();
    opts.sort_by(|a, b| a.label.cmp(&b.label));
    opts
}

fn labels_table() -> &'static HashMap<String, HashMap<String, String>> {
    static TABLE: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/subdiv_labels_en.json"))
            .expect("subdiv_labels_en.json")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_by_bavaria() {
        assert_eq!(subdiv_name("DE", "BY"), Some("Bavaria"));
        assert_eq!(format_subdiv_label("DE", "BY"), "Bavaria (BY)");
    }
}
