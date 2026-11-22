//! SPDX-License-Identifier: GPL-3.0-or-later
//! Nager.Date API v3 JSON types.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AvailableCountry {
    #[serde(rename = "countryCode")]
    pub country_code: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct PublicHoliday {
    pub date: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "localName")]
    pub local_name: Option<String>,
    #[serde(default)]
    pub global: bool,
    #[serde(default)]
    pub counties: Option<Vec<String>>,
    #[serde(default)]
    pub types: Vec<String>,
}
