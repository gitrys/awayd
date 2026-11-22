//! SPDX-License-Identifier: GPL-3.0-or-later
//! Credential storage: system keychain (`keyring` → Secret Service / kwallet) + file fallback.

use sdss_core::atomic_fs;
use sdss_core::config::paths;
use anyhow::Result;
use keyring::Entry;
use tracing::warn;

const SERVICE: &str = "org.steamdiscord.Scheduler";
const USER: &str = "discord-token";

pub fn get_token_sync() -> Result<Option<String>> {
    let entry = Entry::new(SERVICE, USER)?;
    match entry.get_password() {
        Ok(s) if !s.trim().is_empty() => Ok(Some(s)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => {
            let fallback = token_fallback_path();
            if fallback.is_file() {
                let t = std::fs::read_to_string(&fallback)?;
                let t = t.trim();
                if !t.is_empty() {
                    return Ok(Some(t.to_string()));
                }
            }
            Ok(None)
        }
        Err(e) => {
            warn!(
                message_id = "sdss_secret_fallback",
                ?e,
                "keyring_get_failed_try_file"
            );
            let fallback = token_fallback_path();
            if fallback.is_file() {
                let t = std::fs::read_to_string(&fallback)?;
                let t = t.trim();
                if !t.is_empty() {
                    return Ok(Some(t.to_string()));
                }
            }
            Ok(None)
        }
    }
}

pub fn set_token_sync(token: &str) -> Result<()> {
    let entry = Entry::new(SERVICE, USER)?;
    if let Err(e) = entry.set_password(token) {
        warn!(
            message_id = "sdss_secret_fallback",
            ?e,
            "keyring_set_fallback_file"
        );
        let p = token_fallback_path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        atomic_fs::atomic_write(&p, token.as_bytes())?;
        return Ok(());
    }
    Ok(())
}

fn token_fallback_path() -> std::path::PathBuf {
    paths::state_dir().join("discord_token.secret")
}

/// Run blocking keyring I/O off the async caller when a Tokio runtime exists (scheduler);
/// otherwise call synchronously (D-Bus/zbus executor threads have no Tokio reactor).
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

pub async fn get_token() -> Result<Option<String>> {
    run_blocking(get_token_sync).await
}

pub async fn set_token(token: &str) -> Result<()> {
    let t = token.to_string();
    run_blocking(move || set_token_sync(&t)).await
}

pub fn clear_token_sync() -> Result<()> {
    let entry = Entry::new(SERVICE, USER)?;
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => {
            warn!(
                message_id = "sdss_secret_fallback",
                ?e,
                "keyring_delete_failed"
            );
        }
    }
    let p = token_fallback_path();
    if p.is_file() {
        std::fs::remove_file(&p)?;
    }
    Ok(())
}

pub async fn clear_token() -> Result<()> {
    run_blocking(clear_token_sync).await
}
