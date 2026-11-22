//! SPDX-License-Identifier: GPL-3.0-or-later
//! Steam / Discord Status Scheduler — resident agent library.

pub mod apply;
pub mod holidays;
pub mod local_time;
pub mod network_probe;
pub mod override_state;
pub mod platform;
pub mod runtime_config;
pub mod schedule_next;
pub mod scheduler;
pub mod secret;
pub mod service;
pub mod tray;
pub mod watchers;
pub mod work_window;

pub use schedule_next::{next_schedule_flip_after, next_schedule_flip_after_default};

pub mod run;
