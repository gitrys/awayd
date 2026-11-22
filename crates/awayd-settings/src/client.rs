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
        Ok(self.inner.set_config_toml(toml).await?)
    }

    pub async fn has_credential(&mut self, id: &str) -> Result<bool> {
        Ok(self.inner.has_credential(id).await?)
    }

    pub async fn set_credential(&mut self, id: &str, value: &str) -> Result<bool> {
        Ok(self.inner.set_credential(id, value).await?)
    }

    pub async fn list_subdivisions(&mut self, country: &str) -> Result<Vec<String>> {
        Ok(self.inner.list_subdivisions(country).await?)
    }

    pub async fn list_countries(&mut self) -> Result<Vec<(String, String)>> {
        Ok(self.inner.list_available_countries().await?)
    }

    pub async fn list_holidays(
        &mut self,
        country: &str,
        subdiv: &str,
        year: i32,
    ) -> Result<awayd_ipc::ListHolidaysResponse> {
        Ok(self.inner.list_holidays(country, subdiv, year).await?)
    }

    pub async fn refresh_holiday_data(&mut self, country: &str) -> Result<String> {
        Ok(self.inner.refresh_holiday_data(country).await?)
    }

    /// Daemon status JSON (`StatusSnapshot`).
    pub async fn status(&mut self) -> Result<String> {
        Ok(self.inner.status().await?)
    }
}
