//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use sdss_core::config::paths;
use sdss_core::config::{self};
use sdss_core::config::schema::DayWindow;
use sdss_core::schedule::{DAY_LABELS, schedule_days};
use sdss_core::status::StatusSnapshot;
use sdss_ipc::SchedulerClient;

fn is_tty() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

fn c(code: &str, s: &str) -> String {
    if is_tty() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn format_day_line(day: &DayWindow, label: &str) -> String {
    if day.enabled {
        format!(
            "  {:<4} {}–{}",
            label,
            day.start.format("%H:%M"),
            day.end.format("%H:%M")
        )
    } else {
        format!("  {:<4} off", label)
    }
}

fn print_schedule_days(days: &[DayWindow; 7]) {
    println!("  schedule days (Mon..Sun):");
    for (i, day) in days.iter().enumerate() {
        println!("{}", format_day_line(day, DAY_LABELS[i]));
    }
}

fn print_pretty_table(s: &StatusSnapshot) {
    let mode = if s.work_mode {
        "invisible (work)"
    } else {
        "online (off)"
    };
    println!("{}", c("1", "SDSS doctor — daemon status"));
    println!(
        "  {:<16} {}",
        "work_mode:",
        c(if s.work_mode { "31" } else { "32" }, mode)
    );
    println!("  {:<16} {}", "source:", s.source);
    println!("  {:<16} {}", "mode_kind:", s.mode_kind);
    println!("  {:<16} {}", "ok:", if s.ok { "yes" } else { "no" });
    if let Some(ref nw) = s.next_wake {
        println!("  {:<16} {}", "next_wake:", nw);
    }
    if let Some(ref la) = s.last_apply {
        println!("  {:<16} {}", "last_apply.at:", la.at);
        println!(
            "  {:<16} {}",
            "last_apply.ok:",
            if la.ok { "yes" } else { "no" }
        );
        if let Some(ref e) = la.discord_error {
            println!("  {:<16} {}", "discord_error:", e);
        }
        if let Some(ref e) = la.steam_error {
            println!("  {:<16} {}", "steam_error:", e);
        }
    }
    if !s.errors.is_empty() {
        println!("  errors:");
        for e in &s.errors {
            println!("    - {e}");
        }
    }
}

pub async fn doctor(json: bool, pretty: bool) -> Result<()> {
    let cfg_loaded = config::load_config(&paths::config_file());

    println!("  {:<16} {}", "portable:", paths::is_portable());
    println!("  {:<16} {}", "config:", paths::config_file().display());
    println!("  {:<16} {}", "state:", paths::state_file().display());

    let mut client = SchedulerClient::new();
    if client.ping().await.unwrap_or(false) {
        match client.status().await {
            Ok(raw) => {
                if json {
                    println!("{raw}");
                    return Ok(());
                }
                if pretty {
                    if let Ok(s) = serde_json::from_str::<StatusSnapshot>(&raw) {
                        print_pretty_table(&s);
                        if let Ok(cfg) = &cfg_loaded {
                            let cc = cfg.schedule.country.to_uppercase();
                            if let Ok((ready, y0, y1, fetched, msg)) =
                                client.holiday_data_status(&cc).await
                            {
                                println!(
                                    "  {:<16} {}",
                                    "holidays:",
                                    if ready {
                                        format!("{cc} {y0}..{y1} (updated {fetched})")
                                    } else {
                                        msg
                                    }
                                );
                            }
                        }
                        return Ok(());
                    }
                }
                println!("daemon status:\n{raw}");
                return Ok(());
            }
            Err(e) => tracing::warn!(target: "sdss::doctor", "status failed: {e}"),
        }
    }

    let cfg = cfg_loaded?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "local": true,
                "country": cfg.schedule.country,
                "subdiv": cfg.schedule.subdiv,
            })
        );
    } else {
        println!("{}", c("33", "no daemon; local config:"));
        println!(
            "  country={} subdiv={}",
            cfg.schedule.country, cfg.schedule.subdiv
        );
        print_schedule_days(&schedule_days(&cfg.schedule));
    }
    Ok(())
}
