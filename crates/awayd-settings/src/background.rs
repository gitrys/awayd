//! SPDX-License-Identifier: GPL-3.0-or-later
//! IPC calls that may take long (downloads by the daemon) run on the runtime's worker thread
//! instead of blocking the UI thread; the window repaints when the result is there.

use std::future::Future;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use anyhow::Result;
use eframe::egui;

/// One background request and its pending result.
pub struct Job<T> {
    rx: Receiver<Result<T>>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn<F>(rt: &tokio::runtime::Runtime, ctx: &egui::Context, request: F) -> Self
    where
        F: Future<Output = Result<T>> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        rt.spawn(async move {
            let _ = tx.send(request.await);
            ctx.request_repaint();
        });
        Self { rx }
    }

    /// The result once the request has finished; `None` while it is still running.
    pub fn poll(&self) -> Option<Result<T>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err(anyhow::anyhow!("request ended without a result")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    fn wait<T: Send + 'static>(job: &Job<T>) -> Result<T> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(r) = job.poll() {
                return r;
            }
            assert!(Instant::now() < deadline, "job did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn result_arrives_without_blocking_the_caller() {
        let rt = runtime();
        let ctx = egui::Context::default();
        let job = Job::spawn(&rt, &ctx, async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(42)
        });
        // Still running right after the start: the caller (UI thread) was not blocked.
        assert!(job.poll().is_none());
        assert_eq!(wait(&job).unwrap(), 42);
    }

    #[test]
    fn errors_and_lost_tasks_are_reported() {
        let rt = runtime();
        let ctx = egui::Context::default();
        let failed: Job<()> = Job::spawn(&rt, &ctx, async { Err(anyhow::anyhow!("offline")) });
        assert_eq!(wait(&failed).unwrap_err().to_string(), "offline");

        let lost: Job<()> = Job::spawn(&rt, &ctx, async { panic!("task died") });
        assert!(wait(&lost).is_err());
    }
}
