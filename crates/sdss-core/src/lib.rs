//! SPDX-License-Identifier: GPL-3.0-or-later
//! Shared types and config for Steam / Discord Status Scheduler (Windows).

pub mod atomic_fs;
pub mod config;
pub mod launch;
pub mod log;
pub mod log_tail;
pub mod logging;
pub mod schedule;
pub mod schedule_util;
pub mod status;

pub use status::{LastApplyRecord, StatusSnapshot};
