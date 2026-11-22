//! SPDX-License-Identifier: GPL-3.0-or-later
//! Holiday helpers + `HolidayLookup` trait.

pub mod catalogue;
pub mod data;
pub mod lru;
pub mod nager;
pub mod sync;

pub use data::{
    clear_holiday_cache, holiday_year_summary, holiday_year_summary_on, list_holidays_for_subdiv,
    load_holiday_nearby_years, load_holiday_set, load_holidays_or_empty, resolve_country_bytes,
    year_range_in_file, HolidayListEntry, HolidaySet, HolidaySource, HolidayYearSummary,
};
pub use sync::{
    ensure_country, holiday_data_status, list_available_countries_cached, list_cached_bundles,
    refresh_country, HolidayDataStatus,
};

use chrono::NaiveDate;

/// Abstract holiday calendar (public holidays for schedule).
pub trait HolidayLookup: Send + Sync {
    fn is_holiday(&self, d: NaiveDate) -> bool;
}

impl HolidayLookup for HolidaySet {
    fn is_holiday(&self, d: NaiveDate) -> bool {
        HolidaySet::is_holiday(self, d)
    }
}
