//! SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ClientOptions;
use tokio::time::timeout;

use crate::protocol::{decode_line, encode_line, JsonRpcRequest, JsonRpcResponse, PIPE_NAME};

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
        let client = ClientOptions::new().open(PIPE_NAME)?;
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

    pub async fn has_token(&mut self) -> Result<bool> {
        let v = self.call("has_token", json!({})).await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn set_config_toml(&mut self, toml: &str) -> Result<bool> {
        let v = self
            .call("set_config_toml", json!({ "toml": toml }))
            .await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn set_token(&mut self, token: &str) -> Result<bool> {
        let v = self
            .call("set_token", json!({ "token": token }))
            .await?;
        Ok(v.as_bool().unwrap_or(false))
    }

    pub async fn clear_token(&mut self) -> Result<bool> {
        let v = self.call("clear_token", json!({})).await?;
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
        self.local_name
            .as_deref()
            .unwrap_or(self.date.as_str())
    }

    pub fn has_proper_name(&self) -> bool {
        self.local_name.is_some()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListHolidaysResponse {
    pub year_min: i32,
    pub year_max: i32,
    pub holidays: Vec<HolidayRow>,
    pub error: Option<String>,
}
