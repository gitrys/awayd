//! SPDX-License-Identifier: GPL-3.0-or-later
//! JSON-RPC 2.0 over a Windows named pipe for awayd scheduler control.

pub mod client;
pub mod protocol;
pub mod server;

pub use client::{HolidayRow, ListHolidaysResponse, ProbeHostResult, ResourceUse, SchedulerClient};
pub use protocol::{pipe_name, JsonRpcRequest, JsonRpcResponse};
pub use server::{run_pipe_server, SchedulerApi};
