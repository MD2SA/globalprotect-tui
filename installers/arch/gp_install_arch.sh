#!/usr/bin/env bash
#
# gp_install_arch.sh
#
# Install the official Palo Alto Networks GlobalProtect *CLI* on Arch Linux
# (and Arch-based distributions) from the vendor "PanGPLinux-*.tgz" tarball.
#
# Why this exists:
#   The vendor tarball ships only .deb and .rpm packages. Its gp_install.sh
#   detects pacman but has no "arch" branch, so on Arch it exits without
#   installing anything. The CLI binaries themselves are glibc/libstdc++ only
#   and work on Arch. This script installs those binaries and the systemd
#   units/handler that the vendor's Debian post-install script would install.
#
# Unofficial. The GlobalProtect client is proprietary software by Palo Alto
# Networks. This script does not redistribute it; it installs a tarball that
# the user has downloaded from their organisation.
#
set -euo pipefail

readonly PROG="${0##*/}"
# Do not call this VERSION: os-release files on several distributions define
# VERSION, which would collide when the file is sourced.
readonly SCRIPT_VERSION="0.1.6"
readonly GP_DIR="/opt/paloaltonetworks/globalprotect"
readonly SYSTEM_UNIT="/usr/lib/systemd/system/gpd.service"
readonly USER_UNIT="/usr/lib/systemd/user/gpa.service"
readonly DESKTOP_FILE="/usr/share/applications/gp.desktop"
readonly NM_UNMANAGED_CONF="/etc/NetworkManager/conf.d/99-globalprotect-unmanaged.conf"
readonly HANDLER="x-scheme-handler/globalprotectcallback"
# GP_OS_RELEASE is a test seam for the installer regression tests; production
# runs use the real file.
readonly OS_RELEASE="${GP_OS_RELEASE:-/etc/os-release}"

STAGE=""
TARBALL=""
DO_UNINSTALL=0
DO_PURGE=0

usage() {
    cat <<EOF
$PROG $SCRIPT_VERSION - install the official GlobalProtect CLI on Arch Linux

Usage:
  sudo $PROG PanGPLinux-*.tgz            # install CLI
        $PROG --stage DIR PanGPLinux-*.tgz   # lay out files under DIR, no root
  sudo $PROG --uninstall                 # stop services and remove files
  sudo $PROG --uninstall --purge         # also remove ~/.GlobalProtect state

Options:
  --stage DIR    Install into DIR instead of /. Does not touch systemd.
  --uninstall    Remove the installed CLI, units, desktop file and symlink.
  --purge        With --uninstall, also delete the invoking user's ~/.GlobalProtect.
  -h, --help     Show this help.

After install:
  globalprotect connect --portal <your-portal>
  # Your browser opens for SAML/SSO/MFA. Allow "Open GlobalProtect?" if asked.
  globalprotect show --status
  globalprotect show --details
  globalprotect disconnect

Notes:
  * Supported: Arch Linux or Arch-based, x86_64, systemd, desktop session.
    Other Linux distributions are best-effort only (use the vendor .deb/.rpm
    installer where possible). Windows is not supported.
  * The official client uses the system browser for SAML login. There is no
    embedded login window in CLI mode.
  * CLI-only. The official GUI (PanGPUI) needs Qt5 WebEngine, which is not in
    the Arch repositories; it is not supported by this installer.
  * This installer is unofficial and best-effort. It is not endorsed by
    Palo Alto Networks or by any organisation.
EOF
}

log() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die() {
    printf '\033[1;31merror:\033[0m %s\n' "$*" >&2
    exit 1
}

TMP_WORK=""
cleanup() {
    if [ -n "$TMP_WORK" ] && [ -d "$TMP_WORK" ]; then
        rm -rf "$TMP_WORK"
    fi
}
trap cleanup EXIT

require_root() {
    [ "$(id -u)" -eq 0 ] || die "run as root: sudo $PROG ..."
}

check_platform() {
    local strict="${1:-0}" id="" id_like="" fields=""
    if [ -r "$OS_RELEASE" ]; then
        # Read os-release in a subshell. Sourcing it in this shell would let
        # any field it defines (VERSION, NAME, HOME_URL, ...) overwrite the
        # installer's own variables.
        fields="$(
            # shellcheck disable=SC1090
            . "$OS_RELEASE" >/dev/null 2>&1 || true
            printf '%s\n%s\n' "${ID:-}" "${ID_LIKE:-}"
        )"
        id="${fields%%$'\n'*}"
        id_like="${fields#*$'\n'}"
    fi
    case " $id $id_like " in
    *" arch "*) ;;
    *)
        if [ "$strict" -eq 1 ]; then
            die "unsupported distribution (detected ID=${id:-?}, ID_LIKE=${id_like:-?}). This installer targets Arch Linux and Arch-based distributions; use the vendor .deb/.rpm installer on other distributions."
        fi
        warn "this script targets Arch Linux (detected ID=${id:-?}, ID_LIKE=${id_like:-?})"
        ;;
    esac
    if ! command -v pacman >/dev/null 2>&1; then
        if [ "$strict" -eq 1 ]; then
            die "pacman not found; this installer targets Arch Linux."
        fi
        warn "pacman not found; continuing anyway"
    fi
    if [ "$strict" -eq 1 ] && [ ! -d /run/systemd/system ]; then
        die "systemd is not running; gpd.service and gpa.service require systemd."
    fi
}

require_x86_64() {
    local arch
    arch="$(uname -m)"
    [ "$arch" = "x86_64" ] || die "unsupported architecture: $arch. This installer supports x86_64 only; the vendor tarball's ARM packages are not supported by this script yet."
}

check_tools() {
    local t
    for t in bsdtar tar install find; do
        command -v "$t" >/dev/null 2>&1 || die "missing required tool: $t"
    done
}

# Extract a .deb (an ar archive containing data.tar.*) into DEST/root.
extract_deb() {
    local deb="$1" dest="$2" data
    mkdir -p "$dest"
    bsdtar -xf "$deb" -C "$dest"
    data="$(find "$dest" -maxdepth 1 -type f -name 'data.tar.*' | head -n1)"
    [ -n "$data" ] || die "cannot find data.tar.* inside $deb"
    mkdir -p "$dest/root"
    bsdtar -xf "$data" -C "$dest/root"
}

user_uid() {
    getent passwd "$1" | cut -d: -f3
}

user_home() {
    getent passwd "$1" | cut -d: -f6
}

run_user_systemctl() {
    local user="$1"
    shift
    local uid
    uid="$(user_uid "$user")"
    [ -n "$uid" ] || return 1
    runuser -u "$user" -- env \
        XDG_RUNTIME_DIR="/run/user/$uid" \
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
        systemctl --user "$@"
}

# GlobalProtect configures the gpd0 tunnel itself. On NetworkManager systems
# PanGPS writes this config too, but only while connecting: it assigns the
# tunnel address and then deletes the NM connection for gpd0, which removes
# the address and leaves the tunnel down (vendor bug GPC-25169). Install the
# config before the first connection and drop any stale NM profile.
configure_networkmanager() {
    local dest="${1:-}"
    local conf="$dest$NM_UNMANAGED_CONF"

    # NetworkManager is optional. Without it there is no gpd0 conflict, so do
    # not create /etc/NetworkManager/conf.d on hosts that use another network
    # manager (systemd-networkd, iwd, ...).
    if ! command -v nmcli >/dev/null 2>&1; then
        log "nmcli not found; skipping the gpd0 unmanaged config"
        return 0
    fi

    install -d "$(dirname "$conf")"
    cat >"$conf" <<'EOF'
# Installed by gp_install_arch.sh: keep NetworkManager away from the gpd0
# tunnel. GlobalProtect configures this interface itself. Without this file,
# PanGPS deletes the NM connection after assigning the tunnel address, which
# removes the address and leaves gpd0 down (vendor bug GPC-25169).
[keyfile]
unmanaged-devices=interface-name:gpd0
EOF
    chmod 644 "$conf"

    # In --stage mode the tree is inert; do not touch the running system.
    [ -z "$dest" ] || return 0

    if command -v nmcli >/dev/null 2>&1; then
        nmcli connection delete gpd0 >/dev/null 2>&1 || true
        if systemctl is-active --quiet NetworkManager; then
            systemctl reload NetworkManager >/dev/null 2>&1 \
                || nmcli general reload >/dev/null 2>&1 || true
        fi
    fi
}

# Best-effort graceful disconnect before stopping gpd. Some hosts tune
# DefaultTimeoutStopSec down to 5s; PanGPS handles SIGTERM but may need longer
# to tear down gpd0 and its threads. Letting the CLI disconnect first avoids
# starting that teardown only after SIGTERM. The call is bounded so a wedged
# daemon cannot hang uninstall; the regular stop below still runs afterwards.
stop_gpd_gracefully() {
    command -v systemctl >/dev/null 2>&1 || return 0
    systemctl is-active --quiet gpd.service 2>/dev/null || return 0
    [ -x "$GP_DIR/globalprotect" ] || return 0
    command -v timeout >/dev/null 2>&1 || return 0

    local run_as="${SUDO_USER:-}" home="" uid=""
    if [ -n "$run_as" ] && [ "$run_as" != "root" ]; then
        home="$(user_home "$run_as")"
        uid="$(user_uid "$run_as")"
        [ -n "$home" ] && [ -n "$uid" ] || return 0
        command -v runuser >/dev/null 2>&1 || return 0
        log "disconnecting any active GlobalProtect session for $run_as"
        timeout 5 runuser -u "$run_as" -- env HOME="$home" \
            XDG_RUNTIME_DIR="/run/user/$uid" \
            DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$uid/bus" \
            "$GP_DIR/globalprotect" disconnect >/dev/null 2>&1 || true
        return 0
    fi

    # Direct root login: the CLI may refuse system users, but try anyway.
    log "disconnecting any active GlobalProtect session"
    timeout 5 "$GP_DIR/globalprotect" disconnect >/dev/null 2>&1 || true
}

# Surface a stop that systemd had to finish with SIGKILL. This is usually a
# host-wide DefaultTimeoutStopSec policy (some setups use 5s), not a broken
# vendor unit; the installer does not override it.
warn_if_gpd_stop_timed_out() {
    command -v systemctl >/dev/null 2>&1 || return 0
    local state result
    state="$(systemctl show gpd.service -p ActiveState --value 2>/dev/null || true)"
    [ "$state" = "failed" ] || return 0
    result="$(systemctl show gpd.service -p Result --value 2>/dev/null || true)"
    [ "$result" = "timeout" ] || return 0
    warn "gpd.service hit systemd's stop timeout and was killed (Result=timeout)."
    warn "Check: systemctl show gpd.service -p TimeoutStopUSec; journalctl -u gpd -b"
}

uninstall_all() {
    require_root
    stop_gpd_gracefully
    log "stopping GlobalProtect services"
    systemctl disable --now gpd.service 2>/dev/null || true
    warn_if_gpd_stop_timed_out
    if command -v systemctl >/dev/null 2>&1 &&
        systemctl is-active --quiet gpd.service 2>/dev/null; then
        warn "gpd.service is still active after stop; check: systemctl status gpd.service"
    fi
    systemctl --global disable gpa.service 2>/dev/null || true
    if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
        run_user_systemctl "$SUDO_USER" disable --now gpa.service 2>/dev/null || true
    fi

    if command -v nmcli >/dev/null 2>&1; then
        nmcli connection delete gpd0 >/dev/null 2>&1 || true
    fi
    if command -v ip >/dev/null 2>&1; then
        ip link delete gpd0 >/dev/null 2>&1 || true
    fi

    log "removing installed files"
    rm -f "$SYSTEM_UNIT" "$USER_UNIT" /etc/systemd/user/gpa.service \
        "$DESKTOP_FILE" /usr/share/applications/globalprotect.desktop \
        /usr/share/icons/hicolor/48x48/apps/globalprotect.png \
        /usr/share/man/man1/globalprotect.1.gz \
        /usr/bin/globalprotect /etc/profile.d/PanMSInit.sh
    rm -rf "$GP_DIR"
    rm -f "$NM_UNMANAGED_CONF" \
        /etc/NetworkManager/conf.d/90-gp-dns-none.conf

    systemctl daemon-reload 2>/dev/null || true
    systemctl reload NetworkManager 2>/dev/null || true
    if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
        run_user_systemctl "$SUDO_USER" daemon-reload 2>/dev/null || true
    fi
    update-desktop-database /usr/share/applications 2>/dev/null || true

    if [ "$DO_PURGE" -eq 1 ] && [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
        local home
        home="$(user_home "$SUDO_USER")"
        if [ -n "$home" ] && [ -d "$home/.GlobalProtect" ]; then
            warn "removing $home/.GlobalProtect (logs/session state)"
            rm -rf "$home/.GlobalProtect"
        fi
    fi

    log "uninstalled"
    warn "check for leftovers: ip link show gpd0; ip route show"
}

install_all() {
    if [ -n "$STAGE" ]; then
        check_platform 0
    else
        check_platform 1
    fi
    require_x86_64
    check_tools
    [ -n "$TARBALL" ] || die "missing tarball. Try: $PROG --help"
    [ -f "$TARBALL" ] || die "tarball not found: $TARBALL"
    [ -n "$STAGE" ] || require_root

    TMP_WORK="$(mktemp -d)"
    log "extracting $(basename "$TARBALL")"
    tar -xzf "$TARBALL" -C "$TMP_WORK"

    local deb src
    deb="$(find "$TMP_WORK" -maxdepth 1 -type f -name 'GlobalProtect_deb-*.deb' \
        ! -name '*aarch64*' ! -name '*_arm*' | head -n1)"
    [ -n "$deb" ] || die "CLI .deb not found in tarball"

    log "extracting $(basename "$deb")"
    extract_deb "$deb" "$TMP_WORK/deb"
    src="$TMP_WORK/deb/root$GP_DIR"
    [ -d "$src" ] || die "unexpected package layout: $src not found"
    [ -x "$src/globalprotect" ] || die "globalprotect binary not found in package"

    local dest="$STAGE"
    log "installing files${dest:+ into $dest}"
    if [ -z "$STAGE" ]; then
        stop_gpd_gracefully
        log "stopping any existing GlobalProtect services"
        systemctl stop gpd.service 2>/dev/null || true
        warn_if_gpd_stop_timed_out
        if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
            run_user_systemctl "$SUDO_USER" stop gpa.service 2>/dev/null || true
        fi
    fi
    rm -rf "$dest$GP_DIR"
    install -d "$dest$GP_DIR"
    cp -a "$src/." "$dest$GP_DIR/"
    install -d "$dest/usr/bin"
    ln -sfn "$GP_DIR/globalprotect" "$dest/usr/bin/globalprotect"
    install -Dm644 "$src/gpd.service" "$dest$SYSTEM_UNIT"
    install -Dm644 "$src/gpa.service" "$dest$USER_UNIT"
    install -Dm644 "$src/gp.desktop" "$dest$DESKTOP_FILE"
    if [ -f "$TMP_WORK/deb/root/usr/share/man/man1/globalprotect.1.gz" ]; then
        install -Dm644 "$TMP_WORK/deb/root/usr/share/man/man1/globalprotect.1.gz" \
            "$dest/usr/share/man/man1/globalprotect.1.gz"
    fi

    configure_networkmanager "$dest"

    if [ -n "$STAGE" ]; then
        log "staged successfully"
        find "$STAGE" -maxdepth 6 -type f -printf '  %p\n' | head -25
        log "to install for real: sudo $PROG $TARBALL"
        return 0
    fi

    log "enabling services"
    systemctl daemon-reload
    if ! systemctl enable --now gpd.service; then
        warn "gpd.service did not start. Check: journalctl -u gpd -b"
    fi
    systemctl --global enable gpa.service || warn "could not enable gpa.service globally"

    if [ -n "${SUDO_USER:-}" ] && [ "$SUDO_USER" != "root" ]; then
        local home
        home="$(user_home "$SUDO_USER")"
        run_user_systemctl "$SUDO_USER" daemon-reload 2>/dev/null || true
        run_user_systemctl "$SUDO_USER" enable --now gpa.service 2>/dev/null ||
            warn "could not start gpa.service for $SUDO_USER; run: systemctl --user start gpa"
        if [ -n "$home" ]; then
            runuser -u "$SUDO_USER" -- env HOME="$home" \
                xdg-mime default gp.desktop "$HANDLER" 2>/dev/null ||
                warn "could not set the browser handler for $SUDO_USER"
        fi
    fi
    update-desktop-database /usr/share/applications 2>/dev/null || true
    xdg-mime default gp.desktop "$HANDLER" 2>/dev/null || true

    log "checking dependencies"
    local pkg
    for pkg in glibc libstdc++ systemd xdg-utils iptables; do
        pacman -Q "$pkg" >/dev/null 2>&1 || warn "package not installed: $pkg"
    done
    if ! systemctl is-active --quiet systemd-resolved && ! command -v resolvconf >/dev/null 2>&1; then
        warn "neither systemd-resolved nor resolvconf is available; DNS may not be configured. Install openresolv or enable systemd-resolved."
    fi
    [ -e /dev/net/tun ] || warn "/dev/net/tun not found; the tunnel cannot be created. Enable TUN/TAP in the kernel or load the tun module."
    if ! systemctl is-active --quiet NetworkManager && ! command -v nmcli >/dev/null 2>&1; then
        warn "NetworkManager is not active. The gpd0 workaround assumes it; another network manager may work, but that path is untested."
    fi

    log "installed"
    cat <<EOF

Next steps:
  globalprotect connect --portal <your-portal>
  # The browser opens for SAML/SSO/MFA. Allow "Open GlobalProtect?" if prompted.
  globalprotect show --status
  globalprotect show --details
  globalprotect disconnect

Logs:
  journalctl -u gpd -b
  journalctl --user -u gpa -b
  ~/.GlobalProtect/PanGPI.log
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
    --stage)
        [ $# -ge 2 ] || die "--stage needs a directory"
        [ -n "$2" ] || die "--stage needs a non-empty directory"
        case "$2" in
        -*) die "--stage needs a directory, got option: $2" ;;
        esac
        STAGE="$2"
        shift 2
        ;;
    --uninstall)
        DO_UNINSTALL=1
        shift
        ;;
    --purge)
        DO_PURGE=1
        shift
        ;;
    -h | --help)
        usage
        exit 0
        ;;
    --)
        shift
        while [ $# -gt 0 ]; do
            [ -n "$TARBALL" ] && die "only one tarball may be given"
            TARBALL="$1"
            shift
        done
        ;;
    -*)
        die "unknown option: $1 (try --help)"
        ;;
    *)
        [ -n "$TARBALL" ] && die "only one tarball may be given"
        TARBALL="$1"
        shift
        ;;
    esac
done

if [ "$DO_UNINSTALL" -eq 1 ]; then
    [ -z "$STAGE" ] || die "--stage cannot be combined with --uninstall"
    [ -z "$TARBALL" ] || warn "ignoring tarball argument with --uninstall"
    uninstall_all
else
    [ "$DO_PURGE" -eq 0 ] || die "--purge requires --uninstall"
    install_all
fi
