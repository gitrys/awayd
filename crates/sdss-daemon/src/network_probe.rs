//! SPDX-License-Identifier: GPL-3.0-or-later
//! Network-mode reachability probe (TCP connect with short cache).

use std::net::ToSocketAddrs;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sdss_core::config::schema::ModeSection;
use tokio::net::TcpStream;
use tokio::time::timeout;

struct ProbeCache {
    host: String,
    port: u16,
    result: bool,
    at: Instant,
}

static CACHE: Mutex<Option<ProbeCache>> = Mutex::new(None);

const PROBE_TTL: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// `true` when the configured work host is reachable (→ treat as work / invisible).
pub async fn is_work_host_reachable(mode: &ModeSection) -> bool {
    let host = mode.work_host.trim();
    if host.is_empty() {
        return false;
    }
    let port = mode.work_port;
  {
        if let Ok(guard) = CACHE.lock() {
            if let Some(c) = guard.as_ref() {
                if c.host == host && c.port == port && c.at.elapsed() < PROBE_TTL {
                    return c.result;
                }
            }
        }
    }
    let result = probe_tcp(host, port).await;
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(ProbeCache {
            host: host.to_string(),
            port,
            result,
            at: Instant::now(),
        });
    }
    result
}

async fn probe_tcp(host: &str, port: u16) -> bool {
    let addr = match (host, port).to_socket_addrs() {
        Ok(mut addrs) => addrs.next(),
        Err(_) => None,
    };
    let Some(addr) = addr else {
        return false;
    };
    timeout(PROBE_TIMEOUT, TcpStream::connect(addr))
        .await
        .ok()
        .and_then(|r| r.ok())
        .is_some()
}
