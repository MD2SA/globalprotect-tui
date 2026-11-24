# gp-tui

Terminal UI for GlobalProtect on Arch Linux. It is part of this repository and
controls the local backend; it is not a VPN client.

## Requirements

Choose one backend:

- **Official CLI** — installed with
  [`installers/arch/gp_install_arch.sh`](../installers/arch/gp_install_arch.sh),
  with `gpd.service` and `gpa.service` working. Requires the vendor tarball.
- **Free backend** — `gpauth` and `gpclient` from
  [`globalprotect-openconnect`][gpoc]. No vendor tarball is needed.

Also required:

- Arch Linux or an Arch-based distribution, x86_64, systemd.
- A graphical desktop session with a default browser for SAML/SSO/MFA.
- `sudo` access for the free backend.
- Rust/Cargo to build the TUI.
- A terminal of at least **60×16**; 80×24 or larger is recommended.

[gpoc]: https://archlinux.org/packages/extra/x86_64/globalprotect-openconnect/

## Build and install

Prebuilt binary for x86_64 Linux (no Rust needed):

```bash
mkdir -p ~/.local/bin
curl -fsSL https://github.com/MD2SA/gp-arch/releases/latest/download/gp-tui-x86_64-linux.tar.gz |
    tar -xz -C ~/.local/bin
~/.local/bin/gp-tui
```

`~/.local/bin` is a per-user install location. If it is not on your `PATH`,
run the binary by its full path as above or add it to your shell's `PATH`. For a
system-wide install, use `/usr/local/bin`.

From the repository root:

```bash
./install.sh          # ~/.local/bin/gp-tui
~/.local/bin/gp-tui
# or
./install.sh --system # /usr/local/bin/gp-tui
gp-tui
```

For a manual build:

```bash
cargo build --release
./target/release/gp-tui
```

## Backends

The TUI auto-detects the official CLI first, then the free backend.

- **Official CLI** (`globalprotect`): no sudo is needed to connect, and status
  and details come from `globalprotect show`.
- **Free backend** (`gpauth` + `gpclient`): `gpauth` opens the browser for
  SSO/MFA; `gpclient` creates the tunnel with `sudo`. The paid GP Connect GUI is
  not used.

Press `b` to cycle the backend preference: `auto → official → gpclient`. The
choice is saved in the config.

## Free backend flow

```bash
sudo pacman -S globalprotect-openconnect
./install.sh
gp-tui
```

When connecting:

1. `gpauth <portal> --browser` runs as your user and opens the browser.
2. Complete SSO/MFA. Press `Esc` in the TUI to cancel; authentication also
   times out after 180 seconds.
3. If sudo is not cached, the TUI shows a masked password prompt without
   leaving the alternate screen. `Esc` cancels the connection.
4. `sudo -n gpclient connect ... --cookie-on-stdin` runs in the background.
5. The TUI checks `/var/run/gpclient.lock` and the `tun*` interface for status.
6. `gpclient` output is written to `~/.cache/gp-tui/gpclient.log`.

Disconnect runs `sudo -n gpclient disconnect`, using the same prompt when the
sudo timestamp has expired. The password is only held in memory while sudo
validates it; it is never written to the config or the log.

To skip the prompt entirely, allow the user to run only `gpclient` without a
password (replace `youruser`):

```bash
echo 'youruser ALL=(root) NOPASSWD: /usr/bin/gpclient connect *, /usr/bin/gpclient disconnect' |
    sudo tee /etc/sudoers.d/gp-tui
sudo chmod 440 /etc/sudoers.d/gp-tui
sudo visudo -cf /etc/sudoers.d/gp-tui
```

This is optional. Without it, the TUI prompts for your password once per sudo
timestamp (typically 15 minutes).

## Pages and keys

The TUI has three pages plus a Settings detail view and a help overlay:

- **Status** — connection state, next action, IP/gateway details and
  connect/disconnect.
- **Profiles** — saved configurations; choose the active profile and press `e`
  to edit it.
- **Logs** — followed or paused TUI/gpclient logs with scrollbar, search and
  line wrapping.

| Key | Action |
|---|---|
| `1` / `2` / `3` | Status / Profiles / Logs |
| `Tab` / `Shift+Tab` | Next / previous page |
| `h` / `l` | Previous / next page (Vim style) |
| `j` / `k` | Select a profile/setting or scroll logs |
| `Enter` | Profiles: activate; Settings: edit/cycle |
| `e` | Profiles: open Settings |
| `n` / `R` / `D` | Profiles: new / rename / delete |
| `?` | Help overlay |
| `c` | Connect |
| `d` | Disconnect |
| `r` | Refresh status |
| `g` | Start `gpa.service` (official backend only) |
| `L` | Load logs: `collect-log` (official) or the `gpclient.log` tail |
| `b` | Cycle backend preference |
| `p` / `u` | Edit portal / user |
| `↑` / `↓` | Select a config field or scroll logs |
| `PgUp` / `PgDn`, `Home` / `End` | Scroll logs |
| `Ctrl+d` / `Ctrl+u` | Scroll logs by half a page |
| `G` | Jump to the end of the logs |
| `f` | Toggle log follow mode |
| `w` | Toggle log line wrapping |
| `/` | Search/filter logs (`Enter` apply, `Esc` cancel) |
| `y` | Write diagnostics to `~/.cache/gp-tui/diagnostics.txt` |
| `q` / `Ctrl+C` | Quit (confirmation when active) |
| `Esc` | Back, close a modal, cancel edit/search or browser auth |

In edit mode, `Enter` saves and `Esc` cancels. The footer shows the most useful
shortcuts for the current page; press `?` for the full inventory.

## CLI helpers

```bash
gp-tui --status    # JSON for Waybar/scripts
gp-tui --version
gp-tui --help
```

`--status` prints the current state, active profile, portal, backend, IP,
gateway, protocol and errors as JSON. It works without starting the TUI.

## Environment

- `GP_TUI_ASCII=1` — use ASCII symbols and plain borders (serial consoles,
  `TERM=linux`, broken fonts).
- `NO_COLOR=1` — disable colours; state remains visible through symbols and text.

## Settings

`e` on a profile opens its settings. `Esc` returns to the profile list. The
config lives at `~/.config/gp-tui/config.toml`:

```toml
active = "work"

[[profiles]]
name = "work"
portal = "vpn.example.com"
username = "user@example.com"
backend = "auto"

[[profiles]]
name = "home"
portal = "vpn.example.org"
username = ""
backend = "gpclient"
```

The username is optional. The official backend passes it as `--username` to
avoid a prompt, and the free backend passes it as `--user` to `gpclient`. With
`gpauth` cookies it is usually unnecessary. VPN passwords and MFA are handled by
the browser; the sudo password is only passed to `sudo -v` from memory and is
never persisted. Old single-profile configs are migrated automatically to a
profile named `default`.

Writes are atomic. If `config.toml` cannot be parsed, the TUI keeps a copy at
`config.toml.bak` and starts with defaults instead of silently overwriting it.

## Testing and status

The automated suite covers state handling, config parsing, backend arguments
and responsive TestBackend layouts. From the repository root:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
env NO_COLOR=1 GP_TUI_ASCII=1 cargo test
cargo build --release
```

Live VPN tests are not part of CI and must be repeated on a real Arch desktop
with a real portal. See [`TESTING.md`](TESTING.md) for the manual checklist and
current validation state.

## Limits

- Unofficial; not endorsed by Palo Alto Networks or any organisation.
- The official CLI is proprietary and is not bundled here.
- The free backend needs `sudo` for `gpclient` and infers status instead of
  using an official `show --status`.
- SAML/SSO/MFA uses the system browser; the TUI does not embed a browser.
- Arch Linux, x86_64 and systemd are the supported platform. Other
  distributions and platforms are not tested.
- If you quit while `gpclient` is running, the TUI asks for confirmation.
  Disconnect first with `d` (or `sudo gpclient disconnect`).
