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
    use awayd_core::schedule::default_week_days;
    use awayd_daemon::apply::{apply_presence, ApplyPlan, BackendOutcome};
    use awayd_daemon::work_window::{Config, ScheduleConfig};

    let _lock = discord_env_lock().lock().await;

    let mock = MockServer::start().await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&mock)
        .await;
    std::env::set_var("AWAYD_DISCORD_API_URL", mock.uri());

    let cfg = Config {
        discord_auth_token: "  ".into(),
        schedule: ScheduleConfig {
            country: "DE".into(),
            subdiv: "BW".into(),
            days: default_week_days(),
            vacation_ranges: vec![],
        },
    };
    let plan = ApplyPlan {
        discord: true,
        steam: false,
    };
    let r = apply_presence(&cfg, false, plan).await;
    std::env::remove_var("AWAYD_DISCORD_API_URL");

    assert!(
        matches!(r.discord, BackendOutcome::Skipped(_)),
        "{:?}",
        r.discord
    );
    assert_eq!(r.steam, BackendOutcome::NotAttempted);
    assert!(!r.needs_retry());
    mock.verify().await;
}
