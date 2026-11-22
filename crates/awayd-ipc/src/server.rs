//! SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tracing::warn;

use crate::protocol::{decode_line, encode_line, pipe_name, JsonRpcRequest, JsonRpcResponse};

/// Trait implemented by the resident scheduler service.
#[async_trait::async_trait]
pub trait SchedulerApi: Send + Sync {
    async fn status(&self) -> String;
    async fn get_config_toml(&self) -> String;
    /// A credential is stored for backend `id`.
    async fn has_credential(&self, id: &str) -> bool;
    async fn list_subdivisions(&self, country: &str) -> Vec<String>;
    /// May download from the internet: implementations must not block the async caller.
    async fn list_available_countries(&self) -> Vec<(String, String)>;
    async fn holiday_data_status(&self, country: &str) -> (bool, i32, i32, String, String);
    /// Downloads from the internet: implementations must not block the async caller.
    async fn refresh_holiday_data(&self, country: &str) -> String;
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

pub async fn run_pipe_server(
    api: Arc<dyn SchedulerApi>,
    shutdown: tokio_util::sync::CancellationToken,
) {
    let mut server = create_pipe().expect("create initial pipe");
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => break,
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
        .create(pipe_name())?)
}

/// Longest accepted request line: a local process must not make the daemon buffer unbounded
/// data (largest real request: `set_config_toml` with a config of a few KB).
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

async fn handle_client(pipe: NamedPipeServer, api: Arc<dyn SchedulerApi>) -> Result<()> {
    let (reader, writer) = tokio::io::split(pipe);
    serve_lines(reader, writer, api.as_ref()).await
}

/// One JSON-RPC request per line. Invalid JSON gets a `-32700` reply and the session goes on;
/// a line over [`MAX_REQUEST_BYTES`] gets `-32600` and ends the session (the next request
/// boundary cannot be found reliably).
async fn serve_lines<R, W>(reader: R, mut writer: W, api: &dyn SchedulerApi) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = (&mut reader)
            .take(MAX_REQUEST_BYTES as u64 + 1)
            .read_until(b'\n', &mut buf)
            .await?;
        if n == 0 {
            return Ok(());
        }
        if buf.len() > MAX_REQUEST_BYTES {
            let resp = JsonRpcResponse::err(
                Value::Null,
                -32600,
                format!("request larger than {MAX_REQUEST_BYTES} bytes"),
            );
            writer.write_all(&encode_line(&resp)?).await?;
            writer.flush().await?;
            return Ok(());
        }
        let line = buf.trim_ascii();
        if line.is_empty() {
            continue;
        }
        let parsed = decode_line(line)
            .and_then(|v| serde_json::from_value::<JsonRpcRequest>(v).map_err(Into::into));
        let resp = match parsed {
            Ok(req) => dispatch(api, &req).await,
            Err(e) => JsonRpcResponse::err(Value::Null, -32700, format!("parse error: {e}")),
        };
        writer.write_all(&encode_line(&resp)?).await?;
        writer.flush().await?;
    }
}

/// String parameter `name`, empty when missing or not a string.
fn str_param<'a>(req: &'a JsonRpcRequest, name: &str) -> &'a str {
    req.params.get(name).and_then(|v| v.as_str()).unwrap_or("")
}

async fn dispatch(api: &dyn SchedulerApi, req: &JsonRpcRequest) -> JsonRpcResponse {
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
        "list_available_countries" => Ok(json!(api.list_available_countries().await)),
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
            Ok(json!(api.refresh_holiday_data(country).await))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncBufReadExt;

    /// Answers `status` with "ok"; nothing else is called in these tests.
    struct Fake;

    #[async_trait::async_trait]
    impl SchedulerApi for Fake {
        async fn status(&self) -> String {
            "ok".into()
        }
        async fn get_config_toml(&self) -> String {
            unreachable!()
        }
        async fn has_credential(&self, _: &str) -> bool {
            unreachable!()
        }
        async fn list_subdivisions(&self, _: &str) -> Vec<String> {
            unreachable!()
        }
        async fn list_available_countries(&self) -> Vec<(String, String)> {
            unreachable!()
        }
        async fn holiday_data_status(&self, _: &str) -> (bool, i32, i32, String, String) {
            unreachable!()
        }
        async fn refresh_holiday_data(&self, _: &str) -> String {
            unreachable!()
        }
        async fn list_holidays(&self, _: &str, _: &str, _: i32) -> Value {
            unreachable!()
        }
        async fn set_config_toml(&self, _: &str) -> bool {
            unreachable!()
        }
        async fn reload_config(&self) -> bool {
            unreachable!()
        }
        async fn set_credential(&self, _: &str, _: &str) -> bool {
            unreachable!()
        }
        async fn clear_credential(&self, _: &str) -> bool {
            unreachable!()
        }
        async fn apply_now(&self) -> bool {
            unreachable!()
        }
        async fn set_override(&self, _: &str, _: u32) -> bool {
            unreachable!()
        }
        async fn clear_override(&self) -> bool {
            unreachable!()
        }
        fn quit(&self) {}
    }

    /// Send `input`, close the write side, return the reply lines.
    async fn session(input: Vec<u8>) -> Vec<Value> {
        let (client, server) = tokio::io::duplex(4 * MAX_REQUEST_BYTES);
        let (server_r, server_w) = tokio::io::split(server);
        let serve = tokio::spawn(async move { serve_lines(server_r, server_w, &Fake).await });
        let (client_r, mut client_w) = tokio::io::split(client);
        client_w.write_all(&input).await.unwrap();
        client_w.shutdown().await.unwrap();
        serve.await.unwrap().unwrap();
        let mut lines = BufReader::new(client_r).lines();
        let mut out = Vec::new();
        while let Some(l) = lines.next_line().await.unwrap() {
            out.push(serde_json::from_str(&l).unwrap());
        }
        out
    }

    #[tokio::test]
    async fn invalid_json_is_answered_and_the_session_goes_on() {
        let input = b"this is not json\n{\"jsonrpc\":\"2.0\",\"method\":\"status\",\"params\":{},\"id\":7}\n";
        let replies = session(input.to_vec()).await;
        assert_eq!(replies.len(), 2, "{replies:?}");
        assert_eq!(replies[0]["error"]["code"], -32700);
        assert_eq!(replies[1]["result"], "ok");
        assert_eq!(replies[1]["id"], 7);
    }

    #[tokio::test]
    async fn oversized_request_is_rejected_and_ends_the_session() {
        let mut input = vec![b'x'; MAX_REQUEST_BYTES + 10];
        input.extend_from_slice(
            b"\n{\"jsonrpc\":\"2.0\",\"method\":\"status\",\"params\":{},\"id\":1}\n",
        );
        let replies = session(input).await;
        assert_eq!(replies.len(), 1, "{replies:?}");
        assert_eq!(replies[0]["error"]["code"], -32600);
    }
}
