# Testing

## Supported test environment

- Arch Linux or an Arch-based distribution.
- x86_64, systemd, graphical desktop session.
- A real GlobalProtect portal for end-to-end tests.
- `sudo` for the free backend and for real installer tests.

Automated tests do not need root, VPN credentials or a real vendor tarball.
Never include cookies, passwords, tokens or private portal data in test output.

## Automated tests

Run the TUI checks from `gp-tui/`:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
env NO_COLOR=1 GP_TUI_ASCII=1 cargo test
cargo build --release
```

Run the installer tests from the repository root:

```bash
bash -n gp_install_arch.sh
bash -n tests/installer/test_installer.sh
bash tests/installer/test_installer.sh
```

The installer suite builds a synthetic package and uses mocks for system
commands. It never installs on the host.

To check a real vendor tarball without changing the system:

```bash
./gp_install_arch.sh --stage /tmp/gp-stage PanGPLinux-*.tgz
```

The CI workflow runs formatting, Clippy, TUI tests, a release build and the
installer regression suite.

## Manual validation

These checks need a real Arch desktop and a real portal. They are not run in CI.

### Free backend

- [ ] Connect with **User empty**.
- [ ] Disconnect successfully.
- [ ] Connect again with **User filled**.
- [ ] Disconnect successfully.
- [ ] Confirm `gpclient.log` shows no argument errors.
- [ ] Confirm the TUI reports `Connected` only after the tunnel exists.

### Official backend

- [ ] Install the vendor CLI with `gp_install_arch.sh`.
- [ ] Connect through the browser/SSO flow.
- [ ] Confirm `globalprotect show --status` and `show --details`.
- [ ] Disconnect and confirm `gpd0` and routes are gone.

### Lifecycle and system integration

- [ ] Install the TUI and connect/disconnect from a clean shell.
- [ ] Confirm failed connections keep their error visible while status polls.
- [ ] Stop `gpd.service` with the VPN idle and connected; confirm it stops
  without `Result=timeout` or SIGKILL.
- [ ] Run `gp_install_arch.sh --uninstall` and check for leftover services,
  routes, DNS entries, `gpd0` or processes.
- [ ] Suspend and resume with an active tunnel.
- [ ] Switch between Wi-Fi and Ethernet and verify reconnection.

Record the environment, commands and observed result when completing these
checks. Do not record secrets or raw authentication logs.
