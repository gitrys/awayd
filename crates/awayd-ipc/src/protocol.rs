//! SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Named pipe of the awayd daemon in this Windows session: `\\.\pipe\awayd-<session id>`.
///
/// Pipe names are machine-wide; the session id keeps the daemons of several logged-in users
/// (fast user switching, Remote Desktop) apart. Daemon, settings and `awayctl` of one session
/// compute the same name.
pub fn pipe_name() -> String {
    pipe_name_for(current_session_id())
}

fn pipe_name_for(session: u32) -> String {
    format!(r"\\.\pipe\awayd-{session}")
}

fn current_session_id() -> u32 {
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    let mut session = 0u32;
    // SAFETY: plain Win32 call with a valid out pointer. On failure session 0 is used (the
    // services session), which still gives one consistent name for all awayd programs.
    unsafe {
        let _ = ProcessIdToSessionId(GetCurrentProcessId(), &mut session);
    }
    session
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub id: Value,
}

impl JsonRpcRequest {
    pub fn new(method: &str, params: Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
            id: Value::from(id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
            id,
        }
    }
}

/// One JSON-RPC message per line (newline-delimited JSON).
pub fn encode_line(value: &impl Serialize) -> anyhow::Result<Vec<u8>> {
    let mut buf = serde_json::to_vec(value)?;
    buf.push(b'\n');
    Ok(buf)
}

pub fn decode_line(line: &[u8]) -> anyhow::Result<Value> {
    let v: Value = serde_json::from_slice(line)?;
    Ok(v)
}

#[cfg(test)]
mod pipe_tests {
    use super::*;

    #[test]
    fn pipe_name_contains_the_session() {
        assert_eq!(pipe_name_for(3), r"\\.\pipe\awayd-3");
        // Same name for every caller in this process/session.
        assert_eq!(pipe_name(), pipe_name());
        assert!(pipe_name().starts_with(r"\\.\pipe\awayd-"));
    }
}
