//! SPDX-License-Identifier: GPL-3.0-or-later
//! Resident agent backend (tokio): scheduler, IPC, watchers.

use std::sync::Arc;

use anyhow::Result;
use awayd_core::config::{self, paths};
use awayd_ipc::{run_pipe_server, SchedulerApi};
use tokio::sync::{mpsc, watch, Notify, RwLock};

use crate::platform::windows::{power, process_watch};
use crate::service::{SchedulerService, StatusCache};
use crate::watchers::PokeGate;

/// Seconds to wait after suspend/resume before re-applying (network stack settle).
const RESUME_SETTLE_SECS: u64 = 5;

pub struct AgentHandle {
    pub service: Arc<SchedulerService>,
    pub status_rx: watch::Receiver<String>,
    pub status_tx: watch::Sender<String>,
    pub poke_rx: mpsc::Receiver<()>,
    pub shutdown: Arc<Notify>,
}

pub fn setup() -> Result<AgentHandle> {
    paths::ensure_dirs()?;
    let cfg_text = std::fs::read_to_string(paths::config_file())
        .unwrap_or_else(|_| String::from("# empty config\n"));
    let debounce_ms = config::load_config(&paths::config_file())
        .map(|c| c.advanced.poke_debounce_ms.max(1))
        .unwrap_or(250);

    let status = Arc::new(RwLock::new(StatusCache::default()));
    let config_toml = Arc::new(RwLock::new(cfg_text));
    let shutdown = Arc::new(Notify::new());
    let (status_tx, status_rx) = watch::channel(String::new());

    let (poke_tx, poke_rx) = mpsc::channel::<()>(64);
    let poke_gate = Arc::new(PokeGate::new(debounce_ms, poke_tx));

    let service = SchedulerService {
        status: status.clone(),
        config_toml: config_toml.clone(),
        poke: poke_gate.clone(),
        shutdown: shutdown.clone(),
    }
    .arc();

    Ok(AgentHandle {
        service,
        status_rx,
        status_tx,
        poke_rx,
        shutdown,
    })
}

pub async fn run_backend(handle: AgentHandle) -> Result<()> {
    let service = handle.service.clone();
    let status = service.status.clone();
    let config_toml = service.config_toml.clone();
    let poke_gate = service.poke.clone();
    let poke_rx = handle.poke_rx;
    let shutdown = handle.shutdown.clone();
    let status_tx = handle.status_tx;

    let api: Arc<dyn SchedulerApi> = service.clone();
    let shut_pipe = shutdown.clone();
    tokio::spawn(async move {
        run_pipe_server(api, shut_pipe).await;
    });

    let poke_holidays = poke_gate.clone();
    tokio::spawn(async move {
        crate::holidays::sync::background_task(poke_holidays).await;
    });

    let (power_tx, power_rx) = std::sync::mpsc::channel::<()>();
    power::spawn(power_tx);
    let poke_resume = poke_gate.clone();
    tokio::task::spawn_blocking(move || {
        while power_rx.recv().is_ok() {
            std::thread::sleep(std::time::Duration::from_secs(RESUME_SETTLE_SECS));
            poke_resume.poke_immediate();
        }
    });

    process_watch::spawn(poke_gate.clone());

    let sched_shutdown = shutdown.clone();
    let sched = tokio::spawn(async move {
        crate::scheduler::run(
            status,
            config_toml,
            poke_gate,
            poke_rx,
            sched_shutdown,
            status_tx,
        )
        .await
    });

    shutdown.notified().await;
    let _ = sched.await;
    Ok(())
}
