//! SPDX-License-Identifier: GPL-3.0-or-later

mod commands;
mod ttl;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sdss",
    version,
    about = "Steam / Discord status scheduler",
    after_help = "Examples:\n  sdss doctor --pretty\n  sdss logs --tail 50 --grep sent\n  sdss install-autostart\n  sdss override set invisible --ttl 1h30m\n  sdss validate"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Environment and daemon diagnostics
    Doctor {
        #[arg(long)]
        json: bool,
        #[arg(long, help = "Colored table when the daemon is reachable")]
        pretty: bool,
    },
    /// Validate local config.toml
    Validate,
    /// Register sdssd.exe in HKCU Run (portable path)
    InstallAutostart,
    /// Remove sdssd from HKCU Run
    UninstallAutostart,
    Holidays {
        #[command(subcommand)]
        sub: HolidaysCommands,
    },
    Override {
        #[command(subcommand)]
        sub: OverrideCmd,
    },
    /// Tail daemon log file (display filter only)
    Logs {
        #[arg(
            long,
            default_value = "0",
            help = "Show last N lines (0 = entire active file)"
        )]
        tail: usize,
        #[arg(long, help = "Substring filter (case-insensitive)")]
        grep: Option<String>,
        #[arg(long, help = "verbose | warning | error")]
        level: Option<String>,
    },
}

#[derive(Subcommand)]
enum OverrideCmd {
    Set {
        mode: String,
        #[arg(
            short,
            long,
            default_value = "0",
            help = "TTL: 0 or boundary = next schedule change (max 8h); or seconds / 1h30m / 2h"
        )]
        ttl: String,
    },
    Clear,
}

#[derive(Subcommand)]
enum HolidaysCommands {
    List,
    Refresh {
        #[arg(help = "ISO country code (e.g. DE); defaults to schedule.country")]
        country: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    sdss_core::logging::init_tracing();
    if let Err(e) = run(Cli::parse()).await {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command.unwrap_or(Commands::Doctor {
        json: false,
        pretty: false,
    }) {
        Commands::Doctor { json, pretty } => commands::doctor::doctor(json, pretty).await,
        Commands::Validate => commands::validate::validate(),
        Commands::InstallAutostart => commands::autostart::install(),
        Commands::UninstallAutostart => commands::autostart::uninstall(),
        Commands::Holidays { sub } => match sub {
            HolidaysCommands::List => commands::holidays::list().await,
            HolidaysCommands::Refresh { country } => commands::holidays::refresh(country).await,
        },
        Commands::Override { sub } => commands::override_::run(sub).await,
        Commands::Logs { tail, grep, level } => commands::logs::logs(tail, grep, level),
    }
}
