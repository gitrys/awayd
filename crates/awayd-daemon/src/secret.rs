//! SPDX-License-Identifier: GPL-3.0-or-later
//! Credential storage per backend: Windows Credential Manager (`keyring`, service `awayd`,
//! user = backend id) + plain-text file fallback `state\credentials\<id>.secret`.

use anyhow::{bail, Result};
use awayd_core::atomic_fs;
use awayd_core::backends;
use awayd_core::config::paths;
use keyring::Entry;
use tracing::warn;

const SERVICE: &str = "awayd";

/// Only ids of backends with a credential are accepted (also keeps the fallback path inside
/// the credentials folder).
fn check_id(id: &str) -> Result<()> {
    if !backends::needs_credential(id) {
        bail!("unknown credential id {id:?}");
    }
    Ok(())
}

fn fallback_path(id: &str) -> std::path::PathBuf {
    paths::state_dir()
        .join("credentials")
        .join(format!("{id}.secret"))
}

fn read_fallback(id: &str) -> Result<Option<String>> {
    let p = fallback_path(id);
    if p.is_file() {
        let t = std::fs::read_to_string(&p)?;
        let t = t.trim();
        if !t.is_empty() {
            return Ok(Some(t.to_string()));
        }
    }
    Ok(None)
}

pub fn get_credential_sync(id: &str) -> Result<Option<String>> {
    check_id(id)?;
    let entry = Entry::new(SERVICE, id)?;
    match entry.get_password() {
        Ok(s) if !s.trim().is_empty() => Ok(Some(s)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => read_fallback(id),
        Err(e) => {
            warn!(
                message_id = "awayd_secret_fallback",
                credential = id,
                ?e,
                "keyring_get_failed_try_file"
            );
            read_fallback(id)
        }
    }
}

pub fn set_credential_sync(id: &str, value: &str) -> Result<()> {
    check_id(id)?;
    let entry = Entry::new(SERVICE, id)?;
    if let Err(e) = entry.set_password(value) {
        warn!(
            message_id = "awayd_secret_fallback",
            credential = id,
            ?e,
            "keyring_set_fallback_file"
        );
        let p = fallback_path(id);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        atomic_fs::atomic_write(&p, value.as_bytes())?;
    }
    Ok(())
}

pub fn clear_credential_sync(id: &str) -> Result<()> {
    check_id(id)?;
    let entry = Entry::new(SERVICE, id)?;
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => {
            warn!(
                message_id = "awayd_secret_fallback",
                credential = id,
                ?e,
                "keyring_delete_failed"
            );
        }
    }
    let p = fallback_path(id);
    if p.is_file() {
        std::fs::remove_file(&p)?;
    }
    Ok(())
}

/// Run blocking keyring I/O off the async caller when a Tokio runtime exists (scheduler);
/// otherwise call synchronously (callers outside a Tokio runtime).
async fn run_blocking<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| anyhow::anyhow!(e))?
    } else {
        f()
    }
}

pub async fn get_credential(id: &str) -> Result<Option<String>> {
    let id = id.to_string();
    run_blocking(move || get_credential_sync(&id)).await
}

pub async fn set_credential(id: &str, value: &str) -> Result<()> {
    let (id, v) = (id.to_string(), value.to_string());
    run_blocking(move || set_credential_sync(&id, &v)).await
}

pub async fn clear_credential(id: &str) -> Result<()> {
    let id = id.to_string();
    run_blocking(move || clear_credential_sync(&id)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ids_without_credential() {
        assert!(get_credential_sync("steam").is_err());
        assert!(set_credential_sync("..\\x", "v").is_err());
        assert!(clear_credential_sync("slack").is_err());
    }

    #[test]
    fn fallback_path_is_per_backend() {
        let p = fallback_path("discord");
        assert!(
            p.ends_with("credentials\\discord.secret"),
            "{}",
            p.display()
        );
    }
}
