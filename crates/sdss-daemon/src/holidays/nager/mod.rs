//! SPDX-License-Identifier: GPL-3.0-or-later
//! Nager.Date API integration.

pub mod client;
pub mod convert;
pub mod types;

pub use client::{
    fetch_available_countries, fetch_available_countries_blocking, fetch_public_holidays,
    fetch_public_holidays_blocking,
};
pub use convert::{build_holiday_file, normalize_subdiv};
