//! SPDX-License-Identifier: GPL-3.0-or-later
//! Coalesce watcher events; immediate poke for user actions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;

pub struct PokeGate {
    immediate: Arc<AtomicBool>,
    last_emit: Mutex<Option<Instant>>,
    debounce: Duration,
    tx: mpsc::Sender<()>,
}

impl PokeGate {
    pub fn new(debounce_ms: u64, tx: mpsc::Sender<()>) -> Self {
        Self {
            immediate: Arc::new(AtomicBool::new(false)),
            last_emit: Mutex::new(None),
            debounce: Duration::from_millis(debounce_ms.max(1)),
            tx,
        }
    }

    /// User clicked “Apply now” — bypass debounce.
    pub fn poke_immediate(&self) {
        self.immediate.store(true, Ordering::SeqCst);
        let _ = self.tx.try_send(());
    }

    /// Event from watcher — debounced.
    pub async fn poke_debounced(&self) {
        let mut last = self.last_emit.lock().await;
        let now = Instant::now();
        if let Some(t) = *last {
            if now.saturating_duration_since(t) < self.debounce {
                return;
            }
        }
        *last = Some(now);
        drop(last);
        let _ = self.tx.send(()).await;
    }

    pub fn take_immediate(&self) -> bool {
        self.immediate.swap(false, Ordering::SeqCst)
    }
}
