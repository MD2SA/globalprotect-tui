mod app;
mod client;
mod config;
mod ui;

use anyhow::Result;
use app::{App, VpnState};
use client::BackendPreference;
use config::Config;

fn print_help() {
    println!(
        "gp-tui {}\n\nUsage:\n  gp-tui              start the TUI\n  gp-tui --status     print status as JSON (for Waybar/scripts)\n  gp-tui --version    print version\n  gp-tui --help       show this help\n\nEnvironment:\n  GP_TUI_ASCII=1      ASCII-only symbols/borders\n  NO_COLOR=1          disable colours\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_status() -> Result<()> {
    let config = Config::load();
    let preference = BackendPreference::parse(config.backend());
    let snapshot = client::snapshot(preference);
    let state = VpnState::from_snapshot(&snapshot.status).label();
    let profile = config.active_profile();
    let value = serde_json::json!({
        "state": state,
        "installed": snapshot.installed,
        "backend": snapshot.backend.label(),
        "profile": config.active,
        "portal": profile.map(|p| p.portal.clone()).unwrap_or_default(),
        "username": profile.map(|p| p.username.clone()).unwrap_or_default(),
        "ip": snapshot.details.ip,
        "gateway": snapshot.details.gateway,
        "gateway_ip": snapshot.details.gateway_ip,
        "protocol": snapshot.details.protocol,
        "uptime": snapshot.details.uptime,
        "gpa_active": snapshot.gpa_active,
        "error": snapshot.error,
    });
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("gp-tui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--status") {
        return print_status();
    }
    if let Some(arg) = args.first() {
        anyhow::bail!("unknown argument: {arg} (try --help)");
    }

    let config = Config::load();
    let mut terminal = ratatui::init();
    let result = App::new(config).run(&mut terminal);
    ratatui::restore();
    result
}
