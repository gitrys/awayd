//! SPDX-License-Identifier: GPL-3.0-or-later
//! JSON-RPC 2.0 over a Windows named pipe for SDSS scheduler control.

pub mod client;
pub mod protocol;
pub mod server;

pub use client::{HolidayRow, ListHolidaysResponse, SchedulerClient};
pub use protocol::{JsonRpcRequest, JsonRpcResponse, PIPE_NAME};
pub use server::{run_pipe_server, SchedulerApi};
