//! SPDX-License-Identifier: GPL-3.0-or-later
//! Network mode: is `work_host` present? Checked by TCP connect or ping (ICMP echo), with a
//! short cache and a miss counter so a single lost packet does not switch to "away".

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use awayd_core::config::schema::{ModeSection, ProbeKind};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Consecutive failed checks before the host counts as gone; one answer counts as present.
const MISSES_FOR_GONE: u32 = 2;
/// Reuse a check result for pokes arriving in quick succession.
const PROBE_TTL: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

struct ProbeState {
    key: (String, u16, ProbeKind),
    present: bool,
    misses: u32,
    at: Instant,
}

static STATE: Mutex<Option<ProbeState>> = Mutex::new(None);

/// Presence after one more check: an answer means present at once, only
/// [`MISSES_FOR_GONE`] failures in a row mean gone. Returns (present, misses).
fn next_presence(present: bool, misses: u32, answered: bool) -> (bool, u32) {
    if answered {
        return (true, 0);
    }
    let misses = misses.saturating_add(1);
    (present && misses < MISSES_FOR_GONE, misses)
}

/// `true` when the configured work host is present (→ treat as work / invisible).
pub async fn is_work_host_reachable(mode: &ModeSection) -> bool {
    let host = mode.work_host.trim();
    if host.is_empty() {
        return false;
    }
    let key = (host.to_string(), mode.work_port, mode.probe);
    let (present, misses) = {
        let guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(s) if s.key == key && s.at.elapsed() < PROBE_TTL => return s.present,
            // Same host: continue counting; another host or probe: start over (absent).
            Some(s) if s.key == key => (s.present, s.misses),
            _ => (false, 0),
        }
    };
    let answered = match mode.probe {
        ProbeKind::Tcp => probe_tcp(host, mode.work_port).await,
        ProbeKind::Ping => probe_ping(host).await,
    };
    let (present, misses) = next_presence(present, misses, answered);
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(ProbeState {
        key,
        present,
        misses,
        at: Instant::now(),
    });
    present
}

/// First address of `host` (asynchronous lookup; the blocking `to_socket_addrs` used to stall
/// a Tokio worker for the whole DNS timeout).
async fn resolve(host: &str, port: u16) -> Option<SocketAddr> {
    tokio::net::lookup_host((host, port)).await.ok()?.next()
}

async fn probe_tcp(host: &str, port: u16) -> bool {
    let Some(addr) = resolve(host, port).await else {
        return false;
    };
    timeout(PROBE_TIMEOUT, TcpStream::connect(addr))
        .await
        .ok()
        .and_then(|r| r.ok())
        .is_some()
}

async fn probe_ping(host: &str) -> bool {
    let Some(IpAddr::V4(ip)) = resolve(host, 0).await.map(|a| a.ip()) else {
        // IPv6 hosts are not supported by the ping probe.
        return false;
    };
    tokio::task::spawn_blocking(move || ping_v4(ip, PROBE_TIMEOUT))
        .await
        .unwrap_or(false)
}

/// One ICMP echo via the Windows IP Helper API (no administrator rights needed). Blocks up to
/// `wait`.
fn ping_v4(ip: Ipv4Addr, wait: Duration) -> bool {
    use windows::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
    };

    const PAYLOAD: &[u8] = b"awayd";
    // Room for one reply, the echoed payload and an ICMP error message.
    let mut reply = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + PAYLOAD.len() + 8];
    // SAFETY: the handle is closed before returning; buffers outlive the call and their sizes
    // are passed with them. The address is an IPv4 address in network byte order.
    unsafe {
        let Ok(handle) = IcmpCreateFile() else {
            return false;
        };
        let replies = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(ip.octets()),
            PAYLOAD.as_ptr().cast(),
            PAYLOAD.len() as u16,
            None,
            reply.as_mut_ptr().cast(),
            reply.len() as u32,
            wait.as_millis() as u32,
        );
        let _ = IcmpCloseHandle(handle);
        // Status 0 = IP_SUCCESS (an "unreachable" reply from a router also counts as a reply).
        replies > 0
            && std::ptr::read_unaligned(reply.as_ptr().cast::<ICMP_ECHO_REPLY>()).Status == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_answer_is_present_two_misses_are_gone() {
        // Absent and no answer: stays absent.
        assert_eq!(next_presence(false, 0, false), (false, 1));
        // Present, one lost packet: still present.
        assert_eq!(next_presence(true, 0, false), (true, 1));
        // Second miss in a row: gone.
        assert_eq!(next_presence(true, 1, false), (false, 2));
        // An answer resets the counter at once.
        assert_eq!(next_presence(false, 5, true), (true, 0));
        assert_eq!(next_presence(true, 1, true), (true, 0));
    }

    #[test]
    fn ping_loopback_answers() {
        assert!(ping_v4(Ipv4Addr::LOCALHOST, PROBE_TIMEOUT));
    }

    #[test]
    fn ping_unused_address_does_not_answer() {
        // TEST-NET-1 (RFC 5737) is never routed.
        assert!(!ping_v4(
            Ipv4Addr::new(192, 0, 2, 1),
            Duration::from_millis(500)
        ));
    }

    #[tokio::test]
    async fn ping_probe_resolves_names() {
        assert!(probe_ping("localhost").await || probe_ping("127.0.0.1").await);
        assert!(!probe_ping("host.invalid").await);
    }
}
