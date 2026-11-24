# Arch installer

`gp_install_arch.sh` installs the official Palo Alto Networks GlobalProtect CLI
from an organisation-provided `PanGPLinux-*.tgz` tarball. It adds the systemd
units, browser callback handler and NetworkManager `gpd0` workaround that the
vendor's Arch support does not provide.

Run it from the repository root through the compatibility wrapper, or invoke
the script directly:

```bash
sudo ./gp_install_arch.sh ~/Downloads/PanGPLinux-*.tgz
sudo ./installers/arch/gp_install_arch.sh ~/Downloads/PanGPLinux-*.tgz
```

Use `--stage DIR` to inspect the file layout without touching the system, and
`--uninstall` to remove the installed CLI and units.

The tests use a synthetic package and mocks; they never install on the host:

```bash
bash installers/arch/tests/test_installer.sh
```

See `../../docs/TESTING.md` for the manual validation checklist.
