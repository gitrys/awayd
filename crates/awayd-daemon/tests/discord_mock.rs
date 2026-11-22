//! SPDX-License-Identifier: GPL-3.0-or-later
//! Discord PATCH against a local mock (`AWAYD_DISCORD_API_URL`).

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
    std::env::set_var("AWAYD_DISCORD_API_URL", &url);

    let res = awayd_daemon::apply::discord::patch_status("DummyDiscordToken", false).await;
    std::env::remove_var("AWAYD_DISCORD_API_URL");

    assert!(
        res.is_ok(),
        "mock PATCH should succeed: {:?}",
        res.as_ref().err()
    );
}

/// A rejected token is a permanent error and is not retried inside
/// `patch_status` (exactly one request).
#[tokio::test]
async fn unauthorized_is_permanent_and_sent_once() {
    let _lock = discord_env_lock().lock().await;

    let mock = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/users/@me/settings"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&mock)
        .await;

    let url = format!("{}/api/v10/users/@me/settings", mock.uri());
    std::env::set_var("AWAYD_DISCORD_API_URL", &url);
    let res = awayd_daemon::apply::discord::patch_status("RejectedToken", true).await;
    std::env::remove_var("AWAYD_DISCORD_API_URL");

    let err = res.expect_err("401 must fail");
    assert_eq!(
        err,
        awayd_daemon::apply::discord::DiscordError::Unauthorized
    );
    assert!(err.is_permanent());
    mock.verify().await;
}

/// Without a token Discord is skipped (no request, no retry).
#[tokio::test]
async fn missing_token_skips_discord_without_request() {
    use awayd_core::backends;
    use awayd_daemon::apply::{apply_presence, build_backend, ApplyPlan, BackendOutcome};

    let _lock = discord_env_lock().lock().await;

    let mock = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&mock)
        .await;
    std::env::set_var("AWAYD_DISCORD_API_URL", mock.uri());

    let discord = backends::find(backends::DISCORD).unwrap();
    let list = vec![build_backend(discord, "  ".into())];
    let plan = ApplyPlan {
        ids: vec![backends::DISCORD],
    };
    let r = apply_presence(&list, false, &plan).await;
    std::env::remove_var("AWAYD_DISCORD_API_URL");

    let discord_outcome = r.get(backends::DISCORD);
    assert!(
        matches!(discord_outcome, BackendOutcome::Skipped(_)),
        "{discord_outcome:?}"
    );
    assert_eq!(r.get(backends::STEAM), &BackendOutcome::NotAttempted);
    assert!(!r.needs_retry());
    mock.verify().await;
}
