//! SPDX-License-Identifier: GPL-3.0-or-later
//! Discord PATCH against a local mock (`SDSS_DISCORD_API_URL`).

use std::sync::OnceLock;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn discord_env_lock() -> &'static tokio::sync::Mutex<()> {
    static M: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    M.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[tokio::test]
async fn patch_status_hits_mock_server() {
    let _lock = discord_env_lock().lock().await;

    let mock = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/users/@me/settings"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    let url = format!("{}/api/v10/users/@me/settings", mock.uri());
    std::env::set_var("SDSS_DISCORD_API_URL", &url);

    let res = sdss_daemon::apply::discord::patch_status("DummyDiscordToken", false).await;
    std::env::remove_var("SDSS_DISCORD_API_URL");

    assert!(
        res.is_ok(),
        "mock PATCH should succeed: {:?}",
        res.as_ref().err()
    );
}
