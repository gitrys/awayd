//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use awayd_ipc::SchedulerClient;

pub struct Ipc {
    inner: SchedulerClient,
}

impl Ipc {
    pub fn new() -> Self {
        Self {
            inner: SchedulerClient::new(),
        }
    }

    pub async fn ping(&mut self) -> bool {
        self.inner.ping().await.unwrap_or(false)
    }

    pub async fn set_config_toml(&mut self, toml: &str) -> Result<bool> {
        self.inner.set_config_toml(toml).await
    }

    pub async fn has_credential(&mut self, id: &str) -> Result<bool> {
        self.inner.has_credential(id).await
    }

    pub async fn set_credential(&mut self, id: &str, value: &str) -> Result<bool> {
        self.inner.set_credential(id, value).await
    }

    pub async fn list_subdivisions(&mut self, country: &str) -> Result<Vec<String>> {
        self.inner.list_subdivisions(country).await
    }

    pub async fn list_countries(&mut self) -> Result<Vec<(String, String)>> {
        self.inner.list_available_countries().await
    }

    pub async fn list_holidays(
        &mut self,
        country: &str,
        subdiv: &str,
        year: i32,
    ) -> Result<awayd_ipc::ListHolidaysResponse> {
        self.inner.list_holidays(country, subdiv, year).await
    }

    pub async fn refresh_holiday_data(&mut self, country: &str) -> Result<String> {
        self.inner.refresh_holiday_data(country).await
    }

    pub async fn resource_use(&mut self) -> Result<awayd_ipc::ResourceUse> {
        self.inner.resource_use().await
    }

    pub async fn clear_credential(&mut self, id: &str) -> Result<bool> {
        self.inner.clear_credential(id).await
    }

    /// Ask the running awayd to quit (for "Restart awayd").
    pub async fn quit(&mut self) -> Result<()> {
        self.inner.quit().await
    }

    pub async fn holiday_data_status(
        &mut self,
        country: &str,
    ) -> Result<(bool, i32, i32, String, String)> {
        self.inner.holiday_data_status(country).await
    }

    pub async fn apply_now(&mut self) -> Result<bool> {
        self.inner.apply_now().await
    }

    pub async fn set_override(&mut self, mode: &str, ttl_seconds: u32) -> Result<bool> {
        self.inner.set_override(mode, ttl_seconds).await
    }

    pub async fn clear_override(&mut self) -> Result<bool> {
        self.inner.clear_override().await
    }

    pub async fn probe_host(
        &mut self,
        host: &str,
        probe: &str,
        port: u16,
    ) -> Result<awayd_ipc::ProbeHostResult> {
        self.inner.probe_host(host, probe, port).await
    }

    /// Daemon status JSON (`StatusSnapshot`).
    pub async fn status(&mut self) -> Result<String> {
        self.inner.status().await
    }
}
