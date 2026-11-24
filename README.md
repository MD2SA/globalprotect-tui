# GlobalProtect for Arch Linux (unofficial)

An unofficial installer and terminal UI for Palo Alto Networks GlobalProtect on
Arch Linux.

The project has two parts:

- **`gp_install_arch.sh`** installs the official GlobalProtect CLI from the
  vendor `PanGPLinux-*.tgz` tarball, together with the systemd units and browser
  callback handler that the vendor's Arch support is missing.
- **`gp-tui`** is a small terminal frontend for connecting, disconnecting and
  inspecting status or logs. It works with either the official CLI or the free
  [`globalprotect-openconnect`][gpoc] backend.

It is not a VPN implementation and does not store passwords or session cookies.
SAML/SSO/MFA stays in the system browser.

[gpoc]: https://archlinux.org/packages/extra/x86_64/globalprotect-openconnect/

## Quick start

### Free backend (no vendor tarball)

Requires an Arch desktop session and `sudo` access for the tunnel. Install the
TUI from a release binary (no Rust needed):

```bash
sudo pacman -S --needed globalprotect-openconnect
mkdir -p ~/.local/bin
curl -fsSL https://github.com/MD2SA/gp-arch/releases/latest/download/gp-tui-x86_64-linux.tar.gz |
    tar -xz -C ~/.local/bin
gp-tui
```

Or build it from source:

```bash
sudo pacman -S --needed git rust globalprotect-openconnect
git clone https://github.com/MD2SA/gp-arch.git
cd gp-arch/gp-tui
./install.sh
gp-tui
```

`install.sh` builds the release binary once and copies it to `~/.local/bin`.
After that, run `gp-tui` directly; Rust is only needed to build it. Make sure
`~/.local/bin` is in your `PATH`.

On first start, enter your portal in the TUI. Press `c` to connect; the browser
opens for SSO/MFA, then the TUI asks for your sudo password in a masked prompt.

Closing the terminal sends `SIGHUP`; the TUI then tries
`sudo -n gpclient disconnect` before exiting. If the sudo timestamp has already
expired, that cleanup cannot prompt and the tunnel may stay up; run
`sudo gpclient disconnect` in that case.

### Official CLI (requires the vendor tarball)

Download `PanGPLinux-*.tgz` from your organisation's VPN page, then:

```bash
cd gp-arch
chmod +x gp_install_arch.sh
sudo ./gp_install_arch.sh ~/Downloads/PanGPLinux-*.tgz

cd gp-tui
./install.sh
gp-tui
```

To inspect the staged files without changing the system:

```bash
./gp_install_arch.sh --stage /tmp/gp-stage ~/Downloads/PanGPLinux-*.tgz
```

## What the TUI does

`gp-tui` auto-detects an installed backend:

| Backend | How it connects | Needs vendor tarball |
|---|---|---|
| Official CLI | `globalprotect` + `gpd`/`gpa` services | Yes |
| Free backend | `gpauth` + `sudo gpclient` | No |

Use `b` to cycle the preferred backend (`auto`, `official`, `gpclient`).
Credentials and MFA stay in the browser; the TUI only displays status and
controls the local client. For the free backend, the sudo password is requested
in a masked in-app prompt, so the TUI stays on screen.

The TUI has three pages:

- **Status** — state, portal, connection details and the next action.
- **Profiles** — saved VPN configurations and per-profile settings.
- **Logs** — followed or paused client logs, search, wrapping and diagnostics.

The supported terminal minimum is **60×16**; 80×24 or larger is recommended.
`GP_TUI_ASCII=1` and `NO_COLOR=1` are supported.

See [`gp-tui/README.md`](gp-tui/README.md) for the full keymap, configuration
and backend details.

## Requirements and scope

Supported:

- Arch Linux or an Arch-based distribution.
- x86_64.
- systemd.
- A graphical desktop session with a browser for SAML/SSO/MFA.
- Rust/Cargo to build the TUI.
- `/dev/net/tun`.
- NetworkManager is recommended for the official backend; the installer
  configures `gpd0` so NetworkManager leaves the tunnel alone.

Not supported:

- Windows, macOS or non-systemd Linux.
- ARM, unless the guard and vendor package layout are tested.
- Headless/SSH-only sessions.
- The official GUI. It requires Qt5 WebEngine, which is not available in the
  current Arch repositories.

Other Linux distributions are best-effort only. This repository targets Arch
first.

## Testing

Build and test the TUI:

```bash
cd gp-tui
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Run the installer regression suite from the repository root:

```bash
bash -n gp_install_arch.sh
bash -n tests/installer/test_installer.sh
bash tests/installer/test_installer.sh
```

Live VPN connections, real install/uninstall and suspend/resume behaviour are
manual tests because they need a real portal, credentials, root access and a
desktop session. See [`docs/TESTING.md`](docs/TESTING.md) for the checklist and
current validation state.

## Safety

- The TUI does not modify routes, DNS or tunnel interfaces itself.
- No passwords, cookies or tokens are stored in this repository or on disk by
  the TUI. The sudo password is held in memory only while sudo validates it.
- The installer never redistributes Palo Alto Networks binaries. It installs the
  tarball that the user downloaded from their organisation.
- Quitting while a tunnel or operation is active asks for confirmation.

## Uninstall

Remove the TUI installed by `install.sh`:

```bash
rm -f ~/.local/bin/gp-tui
```

Remove the official CLI and its services:

```bash
sudo ./gp_install_arch.sh --uninstall
```

Add `--purge` only if you also want to remove the invoking user's
`~/.GlobalProtect` state.

## License

MIT for this repository. The GlobalProtect client is proprietary software by
Palo Alto Networks and is distributed under its own terms. This project is
unofficial and is not endorsed by Palo Alto Networks or any organisation.
