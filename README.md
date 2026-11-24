# gp-tui

An unofficial terminal UI for GlobalProtect on Arch Linux. It manages the
installed client; it is not a VPN implementation.

- Connect and disconnect using the official CLI or the free
  [`globalprotect-openconnect`](https://archlinux.org/packages/extra/x86_64/globalprotect-openconnect/)
  backend.
- Complete SSO/MFA in your browser; the TUI never stores passwords or session
  cookies.
- View connection status, profiles and client logs.

## Quick start: free backend

Requires Arch Linux (or an Arch-based distribution), x86_64, systemd, a desktop
browser, `/dev/net/tun` and `sudo` access for the tunnel. Install the backend
and prebuilt TUI:

```bash
sudo pacman -S --needed globalprotect-openconnect
mkdir -p ~/.local/bin
curl -fsSL https://github.com/MD2SA/gp-arch/releases/latest/download/gp-tui-x86_64-linux.tar.gz |
    tar -xz -C ~/.local/bin
gp-tui
```

On first launch, enter your VPN portal. Press `c`, finish SSO/MFA in the browser,
and enter your sudo password in the TUI prompt. Press `d` to disconnect.

To build from source instead of downloading the binary:

```bash
sudo pacman -S --needed git rust globalprotect-openconnect
git clone https://github.com/MD2SA/gp-arch.git
cd gp-arch
./install.sh
gp-tui
```

## Optional: official GlobalProtect CLI

If your organisation provides the vendor tarball, the Arch-specific installer is
in [`installers/arch/`](installers/arch/). It does not redistribute vendor
binaries:

```bash
sudo ./installers/arch/gp_install_arch.sh ~/Downloads/PanGPLinux-*.tgz
./install.sh
gp-tui
```

NetworkManager is optional. When installed, the script configures it to leave
GlobalProtect's `gpd0` tunnel interface alone. See
[`installers/arch/README.md`](installers/arch/README.md) for staging and
uninstall instructions.

## Safety and support

- Arch Linux, x86_64 and systemd are the supported platform. A graphical browser
  session is required for SSO/MFA.
- The TUI does not change routes, DNS or tunnel interfaces itself.
- For the free backend, closing the terminal triggers a best-effort disconnect.
  If the sudo timestamp has expired, run `sudo gpclient disconnect` manually.
- This is an unofficial project, not endorsed by Palo Alto Networks.

See [`docs/tui.md`](docs/tui.md) for configuration and the keymap, and
[`docs/TESTING.md`](docs/TESTING.md) for test commands and manual validation.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
bash installers/arch/tests/test_installer.sh
```

## License

MIT for this repository. The GlobalProtect client remains proprietary and is
provided under its own terms.
