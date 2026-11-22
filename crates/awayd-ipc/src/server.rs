//! SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tracing::warn;

use crate::protocol::{decode_line, encode_line, JsonRpcRequest, JsonRpcResponse, PIPE_NAME};

/// Trait implemented by the resident scheduler service.
#[async_trait::async_trait]
pub trait SchedulerApi: Send + Sync {
    async fn status(&self) -> String;
    async fn get_config_toml(&self) -> String;
    /// A credential is stored for backend `id`.
    async fn has_credential(&self, id: &str) -> bool;
    async fn list_subdivisions(&self, country: &str) -> Vec<String>;
    fn list_available_countries(&self) -> Vec<(String, String)>;
    async fn holiday_data_status(&self, country: &str) -> (bool, i32, i32, String, String);
    fn refresh_holiday_data(&self, country: &str) -> String;
    async fn list_holidays(&self, country: &str, subdiv: &str, year: i32) -> Value;
    async fn set_config_toml(&self, toml: &str) -> bool;
    async fn reload_config(&self) -> bool;
    /// Store the credential of backend `id`; false for unknown ids or empty values.
    async fn set_credential(&self, id: &str, value: &str) -> bool;
    async fn clear_credential(&self, id: &str) -> bool;
    async fn apply_now(&self) -> bool;
    async fn set_override(&self, mode: &str, ttl_seconds: u32) -> bool;
    async fn clear_override(&self) -> bool;
    fn quit(&self);
}

pub async fn run_pipe_server(api: Arc<dyn SchedulerApi>, shutdown: Arc<tokio::sync::Notify>) {
    let mut server = create_pipe().expect("create initial pipe");
    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => break,
            res = server.connect() => {
                if let Err(e) = res {
                    warn!(target: "awayd::ipc", "pipe connect: {e}");
                    continue;
                }
                let api = api.clone();
                let next = create_pipe();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(server, api).await {
                        warn!(target: "awayd::ipc", "client session: {e:#}");
                    }
                });
                server = match next {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(target: "awayd::ipc", "recreate pipe: {e}");
                        break;
                    }
                };
            }
        }
    }
}

static FIRST_PIPE: AtomicBool = AtomicBool::new(true);

fn create_pipe() -> Result<NamedPipeServer> {
    let first = FIRST_PIPE.swap(false, Ordering::SeqCst);
    Ok(ServerOptions::new()
        .first_pipe_instance(first)
        .create(PIPE_NAME)?)
}

async fn handle_client(pipe: NamedPipeServer, api: Arc<dyn SchedulerApi>) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(pipe);
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let v = decode_line(line.as_bytes())?;
        let req: JsonRpcRequest = match serde_json::from_value(v) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::err(Value::Null, -32700, format!("parse error: {e}"));
                writer.write_all(&encode_line(&resp)?).await?;
                continue;
            }
        };
        let resp = dispatch(&api, &req).await;
        writer.write_all(&encode_line(&resp)?).await?;
        writer.flush().await?;
    }
    Ok(())
}

/// String parameter `name`, empty when missing or not a string.
fn str_param<'a>(req: &'a JsonRpcRequest, name: &str) -> &'a str {
    req.params.get(name).and_then(|v| v.as_str()).unwrap_or("")
}

async fn dispatch(api: &Arc<dyn SchedulerApi>, req: &JsonRpcRequest) -> JsonRpcResponse {
    let id = req.id.clone();
    let result = match req.method.as_str() {
        "ping" => Ok(json!(true)),
        "status" => Ok(json!(api.status().await)),
        "get_config_toml" => Ok(json!(api.get_config_toml().await)),
        "has_credential" => Ok(json!(api.has_credential(str_param(req, "id")).await)),
        "list_subdivisions" => {
            let country = req
                .params
                .get("country")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(json!(api.list_subdivisions(country).await))
        }
        "list_available_countries" => Ok(json!(api.list_available_countries())),
        "holiday_data_status" => {
            let country = req
                .params
                .get("country")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let s = api.holiday_data_status(country).await;
            Ok(json!([s.0, s.1, s.2, s.3, s.4]))
        }
        "refresh_holiday_data" => {
            let country = req
                .params
                .get("country")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(json!(api.refresh_holiday_data(country)))
        }
        "list_holidays" => {
            let country = req
                .params
                .get("country")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let subdiv = req
                .params
                .get("subdiv")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let year = req.params.get("year").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            Ok(api.list_holidays(country, subdiv, year).await)
        }
        "set_config_toml" => {
            let toml = req
                .params
                .get("toml")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(json!(api.set_config_toml(toml).await))
        }
        "reload_config" => Ok(json!(api.reload_config().await)),
        "set_credential" => Ok(json!(
            api.set_credential(str_param(req, "id"), str_param(req, "value"))
                .await
        )),
        "clear_credential" => Ok(json!(api.clear_credential(str_param(req, "id")).await)),
        "apply_now" => Ok(json!(api.apply_now().await)),
        "set_override" => {
            let mode = req
                .params
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let ttl = req
                .params
                .get("ttl_seconds")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            Ok(json!(api.set_override(mode, ttl).await))
        }
        "clear_override" => Ok(json!(api.clear_override().await)),
        "quit" => {
            api.quit();
            Ok(json!(true))
        }
        other => Err(format!("unknown method: {other}")),
    };

    match result {
        Ok(v) => JsonRpcResponse::ok(id, v),
        Err(e) => JsonRpcResponse::err(id, -32601, e),
    }
}
