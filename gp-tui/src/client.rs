use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Official,
    Gpclient,
    None,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Official => "official globalprotect CLI",
            Backend::Gpclient => "gpclient (openconnect)",
            Backend::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendPreference {
    Auto,
    Official,
    Gpclient,
}

impl BackendPreference {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "official" => Self::Official,
            "gpclient" => Self::Gpclient,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Official => "official",
            Self::Gpclient => "gpclient",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Official => "official only",
            Self::Gpclient => "gpclient only",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Official,
            Self::Official => Self::Gpclient,
            Self::Gpclient => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Details {
    pub ip: String,
    pub gateway: String,
    pub gateway_ip: String,
    pub protocol: String,
    pub uptime: String,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub backend: Backend,
    pub installed: bool,
    pub status: String,
    pub details: Details,
    pub gpa_active: bool,
    pub error: Option<String>,
}

impl Snapshot {
    fn not_installed(preference: BackendPreference) -> Self {
        Self {
            backend: Backend::None,
            installed: false,
            status: "Not installed".to_string(),
            details: Details::default(),
            gpa_active: false,
            error: Some(format!(
                "no usable backend for '{}'; install the official client or globalprotect-openconnect",
                preference.as_str()
            )),
        }
    }
}

pub fn detect_backend(preference: BackendPreference) -> Backend {
    let official = in_path("globalprotect");
    let gpclient = in_path("gpclient") && in_path("gpauth");

    match preference {
        BackendPreference::Auto if official => Backend::Official,
        BackendPreference::Auto if gpclient => Backend::Gpclient,
        BackendPreference::Official if official => Backend::Official,
        BackendPreference::Gpclient if gpclient => Backend::Gpclient,
        _ => Backend::None,
    }
}

fn in_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
        .unwrap_or(false)
}

pub fn snapshot(preference: BackendPreference) -> Snapshot {
    match detect_backend(preference) {
        Backend::Official => official_snapshot(),
        Backend::Gpclient => gpclient_snapshot(),
        Backend::None => Snapshot::not_installed(preference),
    }
}

fn official_snapshot() -> Snapshot {
    let status_text = match run(&["show", "--status"]) {
        Ok(text) => text,
        Err(error) => {
            return Snapshot {
                backend: Backend::Official,
                installed: true,
                status: "Error".to_string(),
                details: Details::default(),
                gpa_active: gpa_active(),
                error: Some(error),
            };
        }
    };

    let details = run(&["show", "--details"])
        .map(|text| parse_details(&text))
        .unwrap_or_default();

    Snapshot {
        backend: Backend::Official,
        installed: true,
        status: parse_status(&status_text),
        details,
        gpa_active: gpa_active(),
        error: None,
    }
}

fn gpclient_snapshot() -> Snapshot {
    let lock = std::fs::read_to_string("/var/run/gpclient.lock").ok();
    let pid = lock.as_deref().and_then(lock_pid);
    let pid_alive = pid
        .map(|pid| Path::new(&format!("/proc/{pid}")).exists())
        .unwrap_or(false);
    let running = pid_alive || gpclient_running();
    let ip = tun_ip();

    let status = if running && ip.is_some() {
        "Connected"
    } else if running {
        "Connecting"
    } else if ip.is_some() {
        "Unknown"
    } else {
        "Disconnected"
    };

    // A leftover lock with a dead PID is harmless: gpclient rewrites it when
    // the tunnel starts and its own is_running() check ignores dead PIDs. Do
    // not present it as a problem; it clears itself on the next connect.
    let error = None;

    Snapshot {
        backend: Backend::Gpclient,
        installed: true,
        status: status.to_string(),
        details: Details {
            ip: ip.unwrap_or_default(),
            protocol: "SSL".to_string(),
            ..Details::default()
        },
        gpa_active: false,
        error,
    }
}

fn gpclient_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "gpclient"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn lock_pid(text: &str) -> Option<u32> {
    text.trim().split(':').next()?.trim().parse().ok()
}

fn tun_ip() -> Option<String> {
    let output = Command::new("ip").args(["-brief", "addr"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(interface) = parts.next() else {
            continue;
        };
        if !interface.starts_with("tun") {
            continue;
        }
        for part in parts {
            let Some(address) = part.split('/').next() else {
                continue;
            };
            if part.contains('/')
                && !address.contains(':')
                && address.chars().next().is_some_and(|c| c.is_ascii_digit())
            {
                return Some(address.to_string());
            }
        }
    }
    None
}

pub fn gpa_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "gpa.service"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == "active")
        .unwrap_or(false)
}

pub fn connect(portal: &str, username: &str) -> Result<Vec<String>, String> {
    let mut command = Command::new("globalprotect");
    command.args(["connect", "--portal", portal.trim()]);
    if !username.trim().is_empty() {
        command.args(["--username", username.trim()]);
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = command
        .output()
        .map_err(|error| format!("failed to run globalprotect: {error}"))?;
    let text = combine_output(&output);

    if output.status.success() {
        Ok(lines(&text))
    } else {
        Err(error_message(&text, "connect"))
    }
}

pub fn disconnect() -> Result<Vec<String>, String> {
    let output = Command::new("globalprotect")
        .arg("disconnect")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run globalprotect: {error}"))?;
    let text = combine_output(&output);

    if output.status.success() {
        Ok(lines(&text))
    } else {
        Err(error_message(&text, "disconnect"))
    }
}

pub fn start_gpa() -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["--user", "start", "gpa.service"])
        .output()
        .map_err(|error| format!("failed to run systemctl: {error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(error_message(
            &combine_output(&output),
            "systemctl --user start gpa",
        ))
    }
}

pub fn collect_log() -> Result<String, String> {
    run(&["collect-log"]).map(|text| {
        let text = text.trim().to_string();
        if text.is_empty() {
            "log bundle written to ~/.GlobalProtect".to_string()
        } else {
            text
        }
    })
}

pub fn ssl_only_status() -> Result<bool, String> {
    let text = run(&["show", "--ssl-only"])?;
    parse_ssl_only(&text)
        .ok_or_else(|| format!("unexpected 'show --ssl-only' output: {}", text.trim()))
}

pub fn parse_ssl_only(text: &str) -> Option<bool> {
    let lower = text.to_lowercase();
    if !(lower.contains("ssl only") || lower.contains("ssl-only")) {
        return None;
    }
    if lower.contains("yes") || lower.contains("enabled") {
        Some(true)
    } else if lower.contains("no") || lower.contains("disabled") {
        Some(false)
    } else {
        None
    }
}

pub fn set_ssl_only(enabled: bool) -> Result<(), String> {
    if enabled {
        run(&["set-config", "--ssl-only"]).map(|_| ())
    } else {
        run(&["set-config"]).map(|_| ())
    }
}

pub fn toggle_ssl_only(current: bool) -> Result<bool, String> {
    set_ssl_only(!current)?;
    ssl_only_status()
}

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub cookie: String,
    pub logs: Vec<String>,
}

/// Runs `gpauth` as the user. It opens the system browser and returns the
/// authentication cookie. `on_start` receives the process id so the TUI can
/// cancel it. Safe to run in a worker while the TUI stays visible.
pub fn gpclient_auth(portal: &str, on_start: impl FnOnce(u32)) -> Result<AuthResult, String> {
    let mut command = Command::new("gpauth");
    command
        .args([portal.trim(), "--browser"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = command
        .spawn()
        .map_err(|error| format!("failed to run gpauth: {error}"))?;
    on_start(child.id());
    let output = child
        .wait_with_output()
        .map_err(|error| format!("waiting for gpauth: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = clean_output(&String::from_utf8_lossy(&output.stderr));
    let logs = lines(&stderr);

    if !output.status.success() {
        let message = stderr.trim();
        return Err(if message.is_empty() {
            format!("gpauth failed (exit {})", output.status)
        } else {
            message.to_string()
        });
    }

    let cookie = stdout.trim().to_string();
    if cookie.is_empty() {
        return Err("gpauth returned an empty cookie".to_string());
    }

    Ok(AuthResult { cookie, logs })
}

/// True when `sudo -n gpclient connect` can run without a prompt, either
/// because sudo has a cached timestamp or because the user has a NOPASSWD rule
/// for it.
///
/// Probing with `--help` avoids touching the tunnel and works with narrow
/// sudoers rules such as `NOPASSWD: /usr/bin/gpclient connect *`.
pub fn sudo_connect_ready() -> bool {
    Command::new("sudo")
        .args(["-n", "gpclient", "connect", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Validates a sudo password without giving sudo a terminal.
///
/// `sudo -S` reads the password from stdin and `-p ""` suppresses the prompt,
/// so nothing is written to the TUI's terminal. On success sudo's timestamp is
/// refreshed, which lets the later `sudo -n` calls run without a prompt.
pub fn sudo_authenticate(password: &[u8]) -> Result<(), String> {
    let mut child = Command::new("sudo")
        .args(["-S", "-p", "", "-v"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to run sudo: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(password)
            .and_then(|_| stdin.write_all(b"\n"))
            .map_err(|error| format!("failed to pass the password to sudo: {error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("waiting for sudo: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = clean_output(&String::from_utf8_lossy(&output.stderr));
    let message = stderr.trim();
    Err(if message.is_empty() {
        "sudo authentication failed".to_string()
    } else {
        message.to_string()
    })
}

/// True when sudo refused a command because its cached timestamp is gone.
pub fn sudo_needs_password(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("password is required")
}

/// Starts `sudo -n gpclient connect` in the background with its output in a log
/// file. Sudo credentials must already be cached (see [`sudo_authenticate`]).
/// `-n` is important: it turns an expired timestamp into an error instead of a
/// terminal password prompt that would corrupt the TUI.
pub fn gpclient_start(cookie: &str, portal: &str, username: &str) -> Result<Child, String> {
    let log_path = gpclient_log_path().ok_or("cannot determine cache directory")?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("creating {}: {error}", parent.display()))?;
    }
    let log = std::fs::File::create(&log_path)
        .map_err(|error| format!("creating {}: {error}", log_path.display()))?;
    let mut permissions = log
        .metadata()
        .map_err(|error| format!("reading {} metadata: {error}", log_path.display()))?
        .permissions();
    permissions.set_mode(0o600);
    std::fs::set_permissions(&log_path, permissions)
        .map_err(|error| format!("securing {}: {error}", log_path.display()))?;
    let log_err = log
        .try_clone()
        .map_err(|error| format!("cloning log handle: {error}"))?;

    let mut command = Command::new("sudo");
    command.arg("-n");
    command.args(gpclient_args(portal, username));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to run sudo gpclient: {error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(cookie.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("failed to pass the cookie to gpclient: {error}"));
        }
    }

    Ok(child)
}

pub fn gpclient_log_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("gp-tui").join("gpclient.log"))
}

pub fn gpclient_log_tail(max: usize) -> Result<Vec<String>, String> {
    let path = gpclient_log_path().ok_or("cannot determine cache directory")?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("reading {}: {error}", path.display()))?;
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let start = lines.len().saturating_sub(max);
    Ok(lines.split_off(start))
}

fn gpclient_args(portal: &str, username: &str) -> Vec<String> {
    let mut args = vec![
        "gpclient".to_string(),
        "connect".to_string(),
        "--cookie-on-stdin".to_string(),
    ];
    if !username.trim().is_empty() {
        args.push("--user".to_string());
        args.push(username.trim().to_string());
    }
    args.push(portal.trim().to_string());
    args
}

/// Runs `sudo -n gpclient disconnect` and waits for it. `-n` prevents a
/// terminal password prompt; callers must authenticate sudo first.
pub fn gpclient_disconnect() -> Result<(), String> {
    let output = Command::new("sudo")
        .args(["-n", "gpclient", "disconnect"])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run sudo gpclient: {error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let text = clean_output(&String::from_utf8_lossy(&output.stderr));
        let message = text.trim();
        Err(if message.is_empty() {
            format!("sudo gpclient disconnect failed ({})", output.status)
        } else {
            message.to_string()
        })
    }
}

fn run(args: &[&str]) -> Result<String, String> {
    let output = Command::new("globalprotect")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run globalprotect: {error}"))?;
    let text = combine_output(&output);

    if output.status.success() {
        Ok(text)
    } else {
        Err(error_message(
            &text,
            &format!("globalprotect {}", args.join(" ")),
        ))
    }
}

fn combine_output(output: &std::process::Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    clean_output(&text)
}

fn error_message(text: &str, action: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        format!("{action} failed")
    } else {
        text.to_string()
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn parse_status(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains("disconnected") {
        "Disconnected".to_string()
    } else if lower.contains("connected") {
        "Connected".to_string()
    } else if lower.contains("connecting") {
        "Connecting".to_string()
    } else {
        "Unknown".to_string()
    }
}

pub fn parse_details(text: &str) -> Details {
    let mut details = Details::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().to_string();
        match key {
            "Assigned IP Address" => details.ip = value,
            "Gateway Name" => details.gateway = value,
            "Gateway IP Address" => details.gateway_ip = value,
            "Protocol" => details.protocol = value,
            "Uptime(sec)" => details.uptime = format_uptime(&value),
            _ => {}
        }
    }
    details
}

fn format_uptime(value: &str) -> String {
    let Ok(seconds) = value.parse::<u64>() else {
        return value.to_string();
    };
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

pub fn clean_output(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\x1b' => {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for character in chars.by_ref() {
                        if ('@'..='~').contains(&character) {
                            break;
                        }
                    }
                }
            }
            '\r' => {}
            '\x08' => {
                output.pop();
            }
            _ => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_with_spinner() {
        assert_eq!(
            parse_status("\r                                                                        \rGlobalProtect status: Connected"),
            "Connected"
        );
        assert_eq!(
            parse_status("GlobalProtect status: Disconnected"),
            "Disconnected"
        );
        assert_eq!(
            parse_status("GlobalProtect status: Connecting"),
            "Connecting"
        );
    }

    #[test]
    fn parses_details() {
        let text = "Gateway Name: vpn.example.com\n\
                    Gateway Description: Example VPN\n\
                    Assigned IP Address: 10.22.32.92\n\
                    Gateway IP Address: 192.0.2.1\n\
                    Protocol: IPSec\n\
                    Uptime(sec): 77\n";
        let details = parse_details(text);
        assert_eq!(details.ip, "10.22.32.92");
        assert_eq!(details.gateway, "vpn.example.com");
        assert_eq!(details.gateway_ip, "192.0.2.1");
        assert_eq!(details.protocol, "IPSec");
        assert_eq!(details.uptime, "1m 17s");
    }

    #[test]
    fn cleans_control_sequences() {
        let text = "\r\x1b[1;34m==>\x1b[0m Connected\r\n";
        assert_eq!(clean_output(text), "==> Connected\n");
    }

    #[test]
    fn parses_gpclient_lock() {
        assert_eq!(lock_pid("1234\n"), Some(1234));
        assert_eq!(lock_pid("1234:8080"), Some(1234));
        assert_eq!(lock_pid("not-a-pid"), None);
    }

    #[test]
    fn builds_gpclient_args() {
        assert_eq!(
            gpclient_args("vpn.example.com", ""),
            vec![
                "gpclient",
                "connect",
                "--cookie-on-stdin",
                "vpn.example.com"
            ]
        );
        assert_eq!(
            gpclient_args("vpn.example.com", "user@example.com"),
            vec![
                "gpclient",
                "connect",
                "--cookie-on-stdin",
                "--user",
                "user@example.com",
                "vpn.example.com"
            ]
        );
    }

    #[test]
    fn backend_preference_cycles() {
        assert_eq!(
            BackendPreference::parse("OFFICIAL"),
            BackendPreference::Official
        );
        assert_eq!(
            BackendPreference::parse("nonsense"),
            BackendPreference::Auto
        );
        assert_eq!(
            BackendPreference::Official.next(),
            BackendPreference::Gpclient
        );
        assert_eq!(BackendPreference::Gpclient.next(), BackendPreference::Auto);
    }

    #[test]
    fn detects_expired_sudo_credentials() {
        assert!(sudo_needs_password("sudo: a password is required"));
        assert!(sudo_needs_password(
            "sudo: a password is required\nsudo: no tty present"
        ));
        assert!(!sudo_needs_password("sudo: a terminal is required"));
        assert!(!sudo_needs_password("connection refused"));
    }

    #[test]
    fn parses_ssl_only_status() {
        assert_eq!(parse_ssl_only("SSL only: yes"), Some(true));
        assert_eq!(parse_ssl_only("SSL-only: enabled"), Some(true));
        assert_eq!(parse_ssl_only("SSL only: no"), Some(false));
        assert_eq!(parse_ssl_only("something else"), None);
    }
}
