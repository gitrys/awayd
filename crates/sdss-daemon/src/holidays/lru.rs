//! SPDX-License-Identifier: GPL-3.0-or-later
//! Holiday parse cache metadata.
//!
//! Parsed holiday sets are memoized in [`super::data::load_holiday_set`] using a bounded in-memory
//! map (see [`HOLIDAY_CACHE_CAP`]). The map stores [`super::data::HolidaySet`] values keyed by
//! country, subdivision, year, and category list.

/// Maximum entries kept in the holiday-set cache; beyond this the map is cleared (simple bound).
pub const HOLIDAY_CACHE_CAP: usize = 64;
