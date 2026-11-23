//! SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::time::timeout;

use crate::protocol::{decode_line, encode_line, pipe_name, JsonRpcRequest, JsonRpcResponse};

/// `ERROR_PIPE_BUSY`: the daemon's one waiting pipe instance was just taken by another client
/// and the next one is not created yet.
const ERROR_PIPE_BUSY: i32 = 231;
/// How long a client keeps retrying a busy pipe before giving up.
const PIPE_BUSY_WAIT: Duration = Duration::from_secs(2);
const PIPE_BUSY_RETRY: Duration = Duration::from_millis(20);

/// Open the pipe `name`; while it is busy (two clients connecting at the same moment), retry
/// for up to `wait`. Any other error (e.g. the daemon is not running) is returned at once.
async fn open_pipe(name: &str, wait: Duration) -> std::io::Result<NamedPipeClient> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match ClientOptions::new().open(name) {
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY)
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(PIPE_BUSY_RETRY).await;
            }
            other => return other,
        }
    }
}

pub struct SchedulerClient {
    next_id: u64,
}

impl Default for SchedulerClient {
    fn default() -> Self {
        Self { next_id: 1 }
    }
}

impl SchedulerClient {
    pub fn new() -> Self {
        Self::default()
    }

    async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let req = JsonRpcRequest::new(method, params, id);
        let resp: JsonRpcResponse = self.roundtrip(&req).await?;
        if let Some(err) = resp.error {
            anyhow::bail!("rpc error {}: {}", err.code, err.message);
        }
        resp.result.context("missing rpc result")
    }

    async fn roundtrip(&self, req: &JsonRpcRequest) -> Result<JsonRpcResponse> {
        let client = open_pipe(&pipe_name(), PIPE_BUSY_WAIT).await?;
        timeout(Duration::from_secs(30), async {
            let (reader, mut writer) = tokio::io::split(client);
            writer.write_all(&encode_line(req)?).await?;
            writer.flush().await?;
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await?
                .context("pipe closed before response")?;
            let v = decode_line(line.as_bytes())?;
            let resp: JsonRpcResponse = serde_json::from_value(v)?;
            Ok(resp)
        })
        .await
        .context("ipc timeout")?
    }

    pub async fn status(&mut self) -> Result<String> {
        let v = self.call("status", json!({})).await?;
        Ok(v.as_str().context("status not string")?.to_string())
    }

    pub async fn get_config_toml(&mut self) -> Result<String> {
        let v = self.call("get_config_toml", json!({})).await?;
        Ok(v.as_str().context("config not string")?.to_string())
    }

    pub async fn has_credential(&mut self, id: &str) -> Result<bool> {
        let v = self.call("has_credential", json!({ "id": id })).await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn set_config_toml(&mut self, toml: &str) -> Result<bool> {
        let v = self
            .call("set_config_toml", json!({ "toml": toml }))
            .await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn set_credential(&mut self, id: &str, value: &str) -> Result<bool> {
        let v = self
            .call("set_credential", json!({ "id": id, "value": value }))
            .await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn clear_credential(&mut self, id: &str) -> Result<bool> {
        let v = self.call("clear_credential", json!({ "id": id })).await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn apply_now(&mut self) -> Result<bool> {
        let v = self.call("apply_now", json!({})).await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn set_override(&mut self, mode: &str, ttl_seconds: u32) -> Result<bool> {
        let v = self
            .call(
                "set_override",
                json!({ "mode": mode, "ttl_seconds": ttl_seconds }),
            )
            .await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn clear_override(&mut self) -> Result<bool> {
        let v = self.call("clear_override", json!({})).await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn quit(&mut self) -> Result<()> {
        let _ = self.call("quit", json!({})).await?;
        Ok(())
    }

    pub async fn list_subdivisions(&mut self, country: &str) -> Result<Vec<String>> {
        let v = self
            .call("list_subdivisions", json!({ "country": country }))
            .await?;
        serde_json::from_value(v).context("list_subdivisions parse")
    }

    pub async fn list_available_countries(&mut self) -> Result<Vec<(String, String)>> {
        let v = self.call("list_available_countries", json!({})).await?;
        serde_json::from_value(v).context("list_available_countries parse")
    }

    pub async fn holiday_data_status(
        &mut self,
        country: &str,
    ) -> Result<(bool, i32, i32, String, String)> {
        let v = self
            .call("holiday_data_status", json!({ "country": country }))
            .await?;
        let arr = v.as_array().context("holiday_data_status array")?;
        Ok((
            arr.first().and_then(|x| x.as_bool()).unwrap_or(false),
            arr.get(1).and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            arr.get(2).and_then(|x| x.as_i64()).unwrap_or(0) as i32,
            arr.get(3)
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            arr.get(4)
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        ))
    }

    pub async fn refresh_holiday_data(&mut self, country: &str) -> Result<String> {
        let v = self
            .call("refresh_holiday_data", json!({ "country": country }))
            .await?;
        Ok(v.as_str().unwrap_or("").to_string())
    }

    pub async fn list_holidays(
        &mut self,
        country: &str,
        subdiv: &str,
        year: i32,
    ) -> Result<ListHolidaysResponse> {
        let v = self
            .call(
                "list_holidays",
                json!({ "country": country, "subdiv": subdiv, "year": year }),
            )
            .await?;
        serde_json::from_value(v).context("list_holidays parse")
    }

    /// One check of `host` by the daemon ("ping", or "tcp" on `port`), e.g. for "Test now".
    pub async fn probe_host(
        &mut self,
        host: &str,
        probe: &str,
        port: u16,
    ) -> Result<ProbeHostResult> {
        let v = self
            .call(
                "probe_host",
                json!({ "host": host, "probe": probe, "port": port }),
            )
            .await?;
        serde_json::from_value(v).context("probe_host parse")
    }

    /// Memory and CPU time of the daemon process.
    pub async fn resource_use(&mut self) -> Result<ResourceUse> {
        let v = self.call("resource_use", json!({})).await?;
        serde_json::from_value(v).context("resource_use parse")
    }

    pub async fn ping(&mut self) -> Result<bool> {
        match self.call("ping", json!({})).await {
            Ok(v) => Ok(v.as_bool().unwrap_or(true)),
            Err(_) => Ok(false),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolidayRow {
    pub date: String,
    pub name: String,
    pub local_name: Option<String>,
}

impl HolidayRow {
    pub fn display_name(&self) -> &str {
        self.local_name.as_deref().unwrap_or(self.date.as_str())
    }

    pub fn has_proper_name(&self) -> bool {
        self.local_name.is_some()
    }
}

/// Result of `resource_use`: what the daemon process uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUse {
    pub working_set_bytes: u64,
    pub private_bytes: u64,
    /// Kernel plus user CPU time since start.
    pub cpu_ms: u64,
    pub uptime_s: u64,
}

/// Result of `probe_host`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeHostResult {
    pub answered: bool,
    /// Time for name lookup and check.
    pub millis: u64,
    /// Why there was no check (e.g. the name could not be resolved); `None` when the host
    /// was checked, answered or not.
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListHolidaysResponse {
    pub year_min: i32,
    pub year_max: i32,
    pub holidays: Vec<HolidayRow>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::windows::named_pipe::ServerOptions;

    fn test_pipe(tag: &str) -> String {
        format!(r"\\.\pipe\awayd-test-{}-{tag}", std::process::id())
    }

    #[tokio::test]
    async fn busy_pipe_is_retried_until_a_new_instance_exists() {
        let name = test_pipe("busy");
        let first = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&name)
            .unwrap();
        // Another client holds the only instance, as the UI thread does while the settings'
        // background request connects.
        let _taken = ClientOptions::new().open(&name).unwrap();
        let err = open_pipe(&name, Duration::ZERO).await.unwrap_err();
        assert_eq!(err.raw_os_error(), Some(ERROR_PIPE_BUSY));

        let later_name = name.clone();
        let later = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            ServerOptions::new().create(&later_name).unwrap()
        });
        let client = open_pipe(&name, Duration::from_secs(2)).await;
        assert!(client.is_ok(), "{:?}", client.err());
        drop(later.await.unwrap());
        drop(first);
    }

    #[tokio::test]
    async fn missing_pipe_fails_at_once() {
        let started = std::time::Instant::now();
        let err = open_pipe(&test_pipe("missing"), Duration::from_secs(2))
            .await
            .unwrap_err();
        // ERROR_FILE_NOT_FOUND: no daemon, no waiting.
        assert_eq!(err.raw_os_error(), Some(2));
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}
