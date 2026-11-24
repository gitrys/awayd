//! SPDX-License-Identifier: GPL-3.0-or-later
//! awayd — resident agent library.

pub mod apply;
pub mod holidays;
pub mod local_time;
pub mod network_probe;
pub mod override_state;
pub mod platform;
pub mod runtime_config;
pub mod schedule_next;
pub mod scheduler;
pub mod secret;
pub mod service;
pub mod state_store;
pub mod tray;
pub mod watchers;
pub mod work_window;

pub use schedule_next::{next_schedule_flip_after, next_schedule_flip_after_default};

/// `err` with all its causes, joined by `: `. A bare `to_string()` on a `reqwest::Error`
/// only says "error sending request for url (…)" and drops whether DNS, connect or TLS failed.
pub(crate) fn error_chain(err: impl std::error::Error + Send + Sync + 'static) -> String {
    format!("{:#}", anyhow::Error::new(err))
}

#[cfg(test)]
mod tests {
    use super::error_chain;

    #[tokio::test]
    async fn error_chain_keeps_the_cause_of_a_reqwest_error() {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        // Port 1 on loopback: nothing listens, the connect is refused.
        let err = client.get("http://127.0.0.1:1/").send().await.unwrap_err();
        let top = err.to_string();
        let chain = error_chain(err);
        assert!(chain.starts_with(&top), "chain: {chain}");
        assert!(chain.len() > top.len(), "no cause appended: {chain}");
    }
}

pub mod run;
