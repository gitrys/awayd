//! SPDX-License-Identifier: GPL-3.0-or-later
//! Discord presence via HTTP PATCH (API v10).

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::{Client, StatusCode};
use serde_json::json;
use tracing::warn;

use crate::error_chain;

fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .use_rustls_tls()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client")
    })
}

const API_URL: &str = "https://discord.com/api/v10/users/@me/settings";
const DISCORD_PATCH_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Full URL for Discord “custom status” PATCH. Override with `AWAYD_DISCORD_API_URL` (integration tests, proxies).
fn settings_url() -> String {
    match std::env::var("AWAYD_DISCORD_API_URL") {
        Ok(u) if !u.trim().is_empty() => u,
        _ => API_URL.to_string(),
    }
}

pub fn normalize_token(raw: &str) -> String {
    raw.trim().trim_matches('"').trim_matches('\'').to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscordError {
    Transport(String),
    Unauthorized,
    RateLimited { retry_after_secs: u64 },
    Server { code: u16, body: String },
    Other(String),
}

impl DiscordError {
    /// Errors a retry cannot fix (rejected token, invalid token header, other 4xx except 429).
    /// The scheduler does not retry these automatically.
    pub fn is_permanent(&self) -> bool {
        match self {
            Self::Unauthorized | Self::Other(_) => true,
            Self::Server { code, .. } => (400..500).contains(code) && *code != 429,
            Self::Transport(_) | Self::RateLimited { .. } => false,
        }
    }
}

impl std::fmt::Display for DiscordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport: {e}"),
            Self::Unauthorized => write!(f, "unauthorized (check token)"),
            Self::RateLimited { retry_after_secs } => {
                write!(f, "rate limited (retry after {retry_after_secs}s)")
            }
            Self::Server { code, body } => write!(f, "HTTP {code}: {body}"),
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> u64 {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f.ceil() as u64)
        .unwrap_or(1)
        .max(1)
}

async fn patch_status_once(auth_token: &str, online: bool) -> Result<String, DiscordError> {
    let status = if online { "online" } else { "invisible" };
    let client = http_client();

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(auth_token).map_err(|e| DiscordError::Other(e.to_string()))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(DISCORD_PATCH_USER_AGENT),
    );

    let body = json!({ "status": status }).to_string();
    let url = settings_url();
    let resp = client
        .patch(&url)
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|e| DiscordError::Transport(error_chain(e)))?;

    let headers = resp.headers().clone();
    let code = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| DiscordError::Transport(error_chain(e)))?;
    if code.is_success() {
        return Ok(format!("HTTP {}", code));
    }
    let body_snip: String = text.chars().take(500).collect();
    match code {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(DiscordError::Unauthorized),
        StatusCode::TOO_MANY_REQUESTS => Err(DiscordError::RateLimited {
            retry_after_secs: parse_retry_after(&headers),
        }),
        c if c.is_server_error() => Err(DiscordError::Server {
            code: c.as_u16(),
            body: body_snip,
        }),
        _ => Err(DiscordError::Server {
            code: code.as_u16(),
            body: body_snip,
        }),
    }
}

/// Longest `Retry-After` waited for inside the apply. The tick blocks the scheduler (pokes,
/// override, quit); longer waits are returned as [`DiscordError::RateLimited`] and the
/// scheduler holds Discord back until then without blocking.
pub const MAX_INLINE_RETRY_AFTER_SECS: u64 = 5;

pub async fn patch_status(auth_token: &str, online: bool) -> Result<String, DiscordError> {
    let mut backoff_secs = 1u64;
    for attempt in 0..4 {
        match patch_status_once(auth_token, online).await {
            Ok(msg) => return Ok(msg),
            Err(DiscordError::RateLimited { retry_after_secs }) => {
                if attempt >= 3 || retry_after_secs > MAX_INLINE_RETRY_AFTER_SECS {
                    return Err(DiscordError::RateLimited { retry_after_secs });
                }
                warn!(
                    target: "awayd::apply",
                    message_id = "awayd_discord_retry",
                    retry_after_secs,
                    attempt,
                    "discord rate limited"
                );
                tokio::time::sleep(Duration::from_secs(retry_after_secs)).await;
            }
            Err(DiscordError::Transport(e)) if attempt < 3 => {
                warn!(
                    target: "awayd::apply",
                    message_id = "awayd_discord_retry",
                    attempt,
                    backoff_secs,
                    error = %e,
                    "discord transport error, retrying"
                );
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(8);
            }
            Err(DiscordError::Server { code, .. }) if code >= 500 && attempt < 3 => {
                warn!(
                    target: "awayd::apply",
                    message_id = "awayd_discord_retry",
                    code,
                    attempt,
                    backoff_secs,
                    "discord server error, retrying"
                );
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(8);
            }
            Err(e) => return Err(e),
        }
    }
    Err(DiscordError::Transport("max retries exceeded".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_errors_are_retryable_variant() {
        let e = DiscordError::Transport("connection reset".into());
        assert!(matches!(e, DiscordError::Transport(_)));
    }

    #[test]
    fn permanent_errors_are_not_retried() {
        assert!(DiscordError::Unauthorized.is_permanent());
        assert!(DiscordError::Other("invalid header".into()).is_permanent());
        let bad_request = DiscordError::Server {
            code: 400,
            body: String::new(),
        };
        assert!(bad_request.is_permanent());

        assert!(!DiscordError::Transport("reset".into()).is_permanent());
        assert!(!DiscordError::RateLimited {
            retry_after_secs: 5
        }
        .is_permanent());
        let server = DiscordError::Server {
            code: 502,
            body: String::new(),
        };
        assert!(!server.is_permanent());
    }
}
