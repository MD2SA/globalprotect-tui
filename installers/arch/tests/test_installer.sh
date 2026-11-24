#!/usr/bin/env bash
#
# Regression tests for gp_install_arch.sh.
#
# These tests never install on the host, never need root, and never need a
# real vendor tarball or VPN credentials. They build a synthetic .deb inside a
# throwaway tarball and exercise the --stage path plus argument/platform guards.
# systemctl, nmcli and runuser are mocked so any attempt to touch the running
# system during staging is caught.
#
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)"
INSTALLER="$REPO_ROOT/installers/arch/gp_install_arch.sh"
WRAPPER="$REPO_ROOT/gp_install_arch.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/gp-installer-tests.XXXXXX")"

cleanup() {
    rm -rf "$WORK"
}
trap cleanup EXIT

PASS=0
FAIL=0
SKIP=0

ok() {
    PASS=$((PASS + 1))
    printf 'ok   - %s\n' "$1"
}

not_ok() {
    FAIL=$((FAIL + 1))
    printf 'FAIL - %s\n' "$1"
    if [ -n "${LAST_RC:-}" ]; then
        printf '       exit: %s\n' "$LAST_RC"
    fi
    if [ -n "${LAST_OUT:-}" ]; then
        printf '       stdout: %s\n' "$(printf '%s' "$LAST_OUT" | head -c 1200)"
    fi
    if [ -n "${LAST_ERR:-}" ]; then
        printf '       stderr: %s\n' "$(printf '%s' "$LAST_ERR" | head -c 1200)"
    fi
}

skip() {
    SKIP=$((SKIP + 1))
    printf 'skip - %s\n' "$1"
}

run_installer() {
    : >"$WORK/stdout"
    : >"$WORK/stderr"
    set +e
    "$@" >"$WORK/stdout" 2>"$WORK/stderr"
    LAST_RC=$?
    set -e
    LAST_OUT="$(<"$WORK/stdout")"
    LAST_ERR="$(<"$WORK/stderr")"
}

assert_rc() {
    local desc="$1" expected="$2"
    if [ "${LAST_RC:-}" -eq "$expected" ]; then
        ok "$desc (exit $expected)"
    else
        not_ok "$desc (wanted exit $expected)"
    fi
}

assert_output_contains() {
    local desc="$1" haystack="$2" needle="$3"
    case "$haystack" in
    *"$needle"*)
        ok "$desc"
        ;;
    *)
        not_ok "$desc (missing: $needle)"
        ;;
    esac
}

assert_output_not_contains() {
    local desc="$1" haystack="$2" needle="$3"
    case "$haystack" in
    *"$needle"*)
        not_ok "$desc (unexpected: $needle)"
        ;;
    *)
        ok "$desc"
        ;;
    esac
}

assert_file() {
    local desc="$1" path="$2"
    if [ -f "$path" ] || [ -L "$path" ]; then
        ok "$desc"
    else
        not_ok "$desc (missing: $path)"
    fi
}

assert_missing() {
    local desc="$1" path="$2"
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
        ok "$desc"
    else
        not_ok "$desc (still exists: $path)"
    fi
}

assert_executable() {
    local desc="$1" path="$2"
    if [ -x "$path" ]; then
        ok "$desc"
    else
        not_ok "$desc (not executable: $path)"
    fi
}

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

MOCK_BIN="$WORK/mock-bin"
CALL_LOG="$WORK/command-calls.log"
mkdir -p "$MOCK_BIN"

cat >"$MOCK_BIN/uname" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "${GP_TEST_ARCH:-x86_64}"
MOCK
chmod +x "$MOCK_BIN/uname"

cat >"$MOCK_BIN/systemctl" <<'MOCK'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >>"${GP_TEST_CALL_LOG:?}"
exit 0
MOCK
chmod +x "$MOCK_BIN/systemctl"

cat >"$MOCK_BIN/nmcli" <<'MOCK'
#!/usr/bin/env bash
printf 'nmcli %s\n' "$*" >>"${GP_TEST_CALL_LOG:?}"
exit 0
MOCK
chmod +x "$MOCK_BIN/nmcli"

cat >"$MOCK_BIN/runuser" <<'MOCK'
#!/usr/bin/env bash
printf 'runuser %s\n' "$*" >>"${GP_TEST_CALL_LOG:?}"
exit 0
MOCK
chmod +x "$MOCK_BIN/runuser"

cat >"$MOCK_BIN/pacman" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
chmod +x "$MOCK_BIN/pacman"

printf 'ID=arch\nID_LIKE=\n' >"$WORK/arch-release"
printf 'ID=debian\nID_LIKE=\n' >"$WORK/debian-release"
# Ubuntu's os-release defines VERSION; sourcing it into the installer used to
# fail with "VERSION: readonly variable". Keep this fixture close to real files.
printf 'ID=ubuntu\nID_LIKE=debian\nVERSION="24.04 LTS (Noble Numbat)"\nVERSION_ID="24.04"\n' \
    >"$WORK/versioned-release"

# Build a deb (ar archive with control.tar.gz + data.tar.gz) from a root tree.
make_deb() {
    local data_root="$1" deb="$2"
    local debdir="$WORK/deb-build"
    rm -rf "$debdir"
    mkdir -p "$debdir/control"
    printf 'Package: fake-globalprotect\nVersion: 1.0\nArchitecture: all\n' \
        >"$debdir/control/control"
    tar -C "$debdir/control" -czf "$debdir/control.tar.gz" .
    tar -C "$data_root" -czf "$debdir/data.tar.gz" .
    printf '2.0\n' >"$debdir/debian-binary"
    (
        cd "$debdir"
        ar rcs "$deb" debian-binary control.tar.gz data.tar.gz
    )
}

# Build the normal fixture tarball used by the staging tests.
build_fixture_tarball() {
    local out="$1"
    local root="$WORK/fixture-root"
    local tarroot="$WORK/fixture-tar"

    rm -rf "$root" "$tarroot"
    mkdir -p "$root/opt/paloaltonetworks/globalprotect"
    mkdir -p "$root/usr/share/man/man1"

    cat >"$root/opt/paloaltonetworks/globalprotect/globalprotect" <<'BIN'
#!/bin/sh
exit 0
BIN
    chmod 755 "$root/opt/paloaltonetworks/globalprotect/globalprotect"
    printf '[Unit]\nDescription=fake gpd\n\n[Service]\nType=simple\nExecStart=/fake/PanGPS\n' \
        >"$root/opt/paloaltonetworks/globalprotect/gpd.service"
    printf '[Unit]\nDescription=fake gpa\n\n[Service]\nType=simple\nExecStart=/fake/PanGPA start\n' \
        >"$root/opt/paloaltonetworks/globalprotect/gpa.service"
    printf '[Desktop Entry]\nName=GlobalProtect\nExec=/fake/globalprotect\n' \
        >"$root/opt/paloaltonetworks/globalprotect/gp.desktop"
    printf 'fake man page\n' | gzip -n >"$root/usr/share/man/man1/globalprotect.1.gz"

    mkdir -p "$tarroot"
    make_deb "$root" "$tarroot/GlobalProtect_deb-1.0-1.deb"
    tar -C "$tarroot" -czf "$out" GlobalProtect_deb-1.0-1.deb
}

FIXTURE_TGZ="$WORK/PanGPLinux-fixture.tgz"
build_fixture_tarball "$FIXTURE_TGZ"

# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

printf '# gp_install_arch.sh regression tests\n'

# Help and argument parsing -------------------------------------------------

run_installer bash "$INSTALLER" --help
assert_rc "help exits successfully" 0
assert_output_contains "help documents --stage" "$LAST_OUT" "--stage DIR"
assert_output_contains "help states the supported scope" "$LAST_OUT" \
    "Supported: Arch Linux or Arch-based, x86_64, systemd, desktop session."

run_installer bash "$WRAPPER" --help
assert_rc "root wrapper help exits successfully" 0
assert_output_contains "root wrapper delegates to the Arch installer" "$LAST_OUT" \
    "--stage DIR"

run_installer bash "$INSTALLER" --definitely-not-an-option
assert_rc "unknown option is rejected" 1
assert_output_contains "unknown option has a useful error" "$LAST_ERR" \
    "unknown option"

run_installer bash "$INSTALLER" --stage
assert_rc "missing --stage value is rejected" 1
assert_output_contains "missing --stage value error" "$LAST_ERR" \
    "--stage needs a directory"

run_installer bash "$INSTALLER" --stage ""
assert_rc "empty --stage value is rejected" 1
assert_output_contains "empty --stage value error" "$LAST_ERR" \
    "--stage needs a non-empty directory"

run_installer bash "$INSTALLER" --stage "$WORK/stage" --uninstall
assert_rc "--stage with --uninstall is rejected" 1
assert_output_contains "--stage/--uninstall conflict error" "$LAST_ERR" \
    "--stage cannot be combined with --uninstall"

run_installer bash "$INSTALLER" --purge
assert_rc "--purge without --uninstall is rejected" 1
assert_output_contains "--purge without --uninstall error" "$LAST_ERR" \
    "--purge requires --uninstall"

run_installer bash "$INSTALLER" --stage "$WORK/stage"
assert_rc "missing tarball is rejected" 1
assert_output_contains "missing tarball error" "$LAST_ERR" "missing tarball"

run_installer bash "$INSTALLER" --stage "$WORK/stage" "$WORK/no-such.tgz"
assert_rc "missing tarball file is rejected" 1
assert_output_contains "missing tarball file error" "$LAST_ERR" \
    "tarball not found"

run_installer bash "$INSTALLER" "$FIXTURE_TGZ" "$FIXTURE_TGZ"
assert_rc "more than one tarball is rejected" 1
assert_output_contains "multiple tarballs error" "$LAST_ERR" \
    "only one tarball may be given"

# Platform and architecture guards -----------------------------------------

run_installer env \
    PATH="$MOCK_BIN:$PATH" \
    GP_OS_RELEASE="$WORK/debian-release" \
    GP_TEST_CALL_LOG="$CALL_LOG" \
    bash "$INSTALLER" --stage "$WORK/stage-debian" "$FIXTURE_TGZ"
assert_rc "staging on a non-Arch os-release warns but succeeds" 0
assert_output_contains "non-Arch distribution warning" "$LAST_ERR" \
    "targets Arch Linux"

# Regression: os-release files on Ubuntu define VERSION. The installer must
# not source them into its own namespace and die on a readonly variable.
run_installer env \
    PATH="$MOCK_BIN:$PATH" \
    GP_OS_RELEASE="$WORK/versioned-release" \
    GP_TEST_CALL_LOG="$CALL_LOG" \
    bash "$INSTALLER" --stage "$WORK/stage-versioned" "$FIXTURE_TGZ"
assert_rc "os-release VERSION does not break staging" 0
assert_output_not_contains "sourcing os-release does not leak shell errors" \
    "$LAST_ERR" "readonly variable"
assert_output_contains "versioned os-release is still detected" "$LAST_ERR" \
    "ID=ubuntu"

run_installer env \
    PATH="$MOCK_BIN:$PATH" \
    GP_OS_RELEASE="$WORK/arch-release" \
    GP_TEST_CALL_LOG="$CALL_LOG" \
    bash "$INSTALLER" --stage "$WORK/stage-arch" "$FIXTURE_TGZ"
assert_rc "staging on Arch succeeds" 0
assert_output_not_contains "Arch staging has no platform warning" "$LAST_ERR" \
    "targets Arch Linux"

run_installer env \
    PATH="$MOCK_BIN:$PATH" \
    GP_OS_RELEASE="$WORK/arch-release" \
    GP_TEST_ARCH=aarch64 \
    GP_TEST_CALL_LOG="$CALL_LOG" \
    bash "$INSTALLER" --stage "$WORK/stage-arm" "$FIXTURE_TGZ"
assert_rc "aarch64 is rejected" 1
assert_output_contains "aarch64 error is explicit" "$LAST_ERR" \
    "unsupported architecture: aarch64"

# Strict real-install platform guards happen before require_root and before
# any file mutation, so they are safe to test as a normal user too.
: >"$CALL_LOG"
run_installer env \
    PATH="$MOCK_BIN:$PATH" \
    GP_OS_RELEASE="$WORK/debian-release" \
    GP_TEST_CALL_LOG="$CALL_LOG" \
    /bin/bash "$INSTALLER" "$FIXTURE_TGZ"
assert_rc "strict install on a non-Arch distro is rejected" 1
assert_output_contains "strict distro error is explicit" "$LAST_ERR" \
    "unsupported distribution"
if [ ! -s "$CALL_LOG" ]; then
    ok "strict distro guard did not invoke host commands"
else
    not_ok "strict distro guard invoked host commands: $(tr '\n' ';' <"$CALL_LOG")"
fi

# Missing tools -------------------------------------------------------------

MIN_BIN="$WORK/min-bin"
mkdir -p "$MIN_BIN"
ln -s "$MOCK_BIN/uname" "$MIN_BIN/uname"
# The uname mock's shebang is /usr/bin/env bash, so keep bash visible while
# hiding the tools the installer must validate.
ln -s /bin/bash "$MIN_BIN/bash"
run_installer env \
    PATH="$MIN_BIN" \
    GP_OS_RELEASE="$WORK/arch-release" \
    /bin/bash "$INSTALLER" --stage "$WORK/stage-notools" "$FIXTURE_TGZ"
assert_rc "missing required tools are rejected" 1
assert_output_contains "missing bsdtar is reported" "$LAST_ERR" \
    "missing required tool: bsdtar"

run_installer env \
    PATH="$MIN_BIN" \
    GP_OS_RELEASE="$WORK/arch-release" \
    /bin/bash "$INSTALLER" "$FIXTURE_TGZ"
assert_rc "strict install without pacman is rejected" 1
assert_output_contains "pacman guard error" "$LAST_ERR" "pacman not found"

# Malformed packages --------------------------------------------------------

BAD_TAR="$WORK/bad-no-deb.tgz"
mkdir -p "$WORK/bad-tar"
printf 'not a package\n' >"$WORK/bad-tar/readme.txt"
tar -C "$WORK/bad-tar" -czf "$BAD_TAR" readme.txt
run_installer bash "$INSTALLER" --stage "$WORK/stage-bad-tar" "$BAD_TAR"
assert_rc "tarball without a .deb is rejected" 1
assert_output_contains "missing .deb error" "$LAST_ERR" \
    "CLI .deb not found in tarball"

BAD_DEB_TGZ="$WORK/bad-deb.tgz"
mkdir -p "$WORK/bad-deb-tar" "$WORK/bad-deb-content"
printf 'no data archive here\n' >"$WORK/bad-deb-content/README"
tar -C "$WORK/bad-deb-content" -czf "$WORK/bad-deb-tar/GlobalProtect_deb-1.0-1.deb" .
tar -C "$WORK/bad-deb-tar" -czf "$BAD_DEB_TGZ" GlobalProtect_deb-1.0-1.deb
run_installer bash "$INSTALLER" --stage "$WORK/stage-bad-deb" "$BAD_DEB_TGZ"
assert_rc ".deb without data.tar.* is rejected" 1
assert_output_contains "missing data.tar.* error" "$LAST_ERR" \
    "cannot find data.tar.*"

# Successful staging, layout and isolation ---------------------------------

STAGE_DIR="$WORK/stage tree"
CALL_LOG="$WORK/command-calls.log"
: >"$CALL_LOG"
run_installer env \
    PATH="$MOCK_BIN:$PATH" \
    GP_OS_RELEASE="$WORK/arch-release" \
    GP_TEST_CALL_LOG="$CALL_LOG" \
    bash "$INSTALLER" --stage "$STAGE_DIR" "$FIXTURE_TGZ"
assert_rc "staging succeeds without root" 0
assert_output_contains "staging reports success" "$LAST_OUT" \
    "staged successfully"
assert_file "GP_DIR/globalprotect is staged" \
    "$STAGE_DIR/opt/paloaltonetworks/globalprotect/globalprotect"
assert_executable "staged globalprotect is executable" \
    "$STAGE_DIR/opt/paloaltonetworks/globalprotect/globalprotect"
assert_file "/usr/bin/globalprotect symlink is staged" \
    "$STAGE_DIR/usr/bin/globalprotect"
assert_file "gpd.service is staged" \
    "$STAGE_DIR/usr/lib/systemd/system/gpd.service"
assert_file "gpa.service is staged" \
    "$STAGE_DIR/usr/lib/systemd/user/gpa.service"
assert_file "gp.desktop is staged" \
    "$STAGE_DIR/usr/share/applications/gp.desktop"
assert_file "man page is staged" \
    "$STAGE_DIR/usr/share/man/man1/globalprotect.1.gz"
assert_file "NetworkManager gpd0 config is staged" \
    "$STAGE_DIR/etc/NetworkManager/conf.d/99-globalprotect-unmanaged.conf"
assert_missing "no gpd.service drop-in is staged" \
    "$STAGE_DIR/etc/systemd/system/gpd.service.d"

if grep -q 'TimeoutStopSec' \
    "$STAGE_DIR/usr/lib/systemd/system/gpd.service"; then
    not_ok "staged gpd.service does not override the vendor timeout"
else
    ok "staged gpd.service keeps the vendor timeout policy"
fi

if [ "$(readlink "$STAGE_DIR/usr/bin/globalprotect" 2>/dev/null || true)" = \
    "/opt/paloaltonetworks/globalprotect/globalprotect" ]; then
    ok "globalprotect symlink points at GP_DIR"
else
    not_ok "globalprotect symlink points at GP_DIR"
fi

if grep -q '^unmanaged-devices=interface-name:gpd0$' \
    "$STAGE_DIR/etc/NetworkManager/conf.d/99-globalprotect-unmanaged.conf"; then
    ok "NetworkManager config marks gpd0 unmanaged"
else
    not_ok "NetworkManager config marks gpd0 unmanaged"
fi

# NetworkManager is optional. Without nmcli the installer must not create its
# config directory, but everything else must still be staged.
NO_NM_BIN="$WORK/no-nm-bin"
mkdir -p "$NO_NM_BIN"
for tool in bash bsdtar tar install find uname mktemp basename rm mkdir cp ln \
    head dirname cat chmod gzip; do
    if tool_path="$(command -v "$tool" 2>/dev/null)"; then
        ln -sf "$tool_path" "$NO_NM_BIN/$tool"
    fi
done
run_installer env \
    PATH="$NO_NM_BIN" \
    GP_OS_RELEASE="$WORK/arch-release" \
    bash "$INSTALLER" --stage "$WORK/stage-no-nm" "$FIXTURE_TGZ"
assert_rc "staging without nmcli succeeds" 0
assert_missing "no NetworkManager config without nmcli" \
    "$WORK/stage-no-nm/etc/NetworkManager/conf.d/99-globalprotect-unmanaged.conf"
assert_file "staging without nmcli still installs the CLI" \
    "$WORK/stage-no-nm/opt/paloaltonetworks/globalprotect/globalprotect"

if [ ! -s "$CALL_LOG" ]; then
    ok "staging did not invoke systemctl, nmcli or runuser"
else
    not_ok "staging invoked host commands: $(tr '\n' ';' <"$CALL_LOG")"
fi

FIRST_LIST="$WORK/stage-first.list"
SECOND_LIST="$WORK/stage-second.list"
find "$STAGE_DIR" -mindepth 1 \( -type f -o -type l \) \
    -printf '%P -> %l\n' | sort >"$FIRST_LIST"

run_installer env \
    PATH="$MOCK_BIN:$PATH" \
    GP_OS_RELEASE="$WORK/arch-release" \
    GP_TEST_CALL_LOG="$CALL_LOG" \
    bash "$INSTALLER" --stage "$STAGE_DIR" "$FIXTURE_TGZ"
assert_rc "staging is repeatable" 0
find "$STAGE_DIR" -mindepth 1 \( -type f -o -type l \) \
    -printf '%P -> %l\n' | sort >"$SECOND_LIST"
if diff -u "$FIRST_LIST" "$SECOND_LIST" >"$WORK/stage-diff"; then
    ok "second staging run leaves the same file set"
else
    not_ok "second staging run changed the file set"
    sed -n '1,40p' "$WORK/stage-diff"
fi

# Guard tests that must fail before touching the host ----------------------

if [ "$(id -u)" -eq 0 ]; then
    skip "real install without root refuses early (running as root)"
    skip "uninstall without root refuses early (running as root)"
else
    : >"$CALL_LOG"
    run_installer env \
        PATH="$MOCK_BIN:$PATH" \
        GP_OS_RELEASE="$WORK/arch-release" \
        GP_TEST_CALL_LOG="$CALL_LOG" \
        bash "$INSTALLER" "$FIXTURE_TGZ"
    assert_rc "real install without root is refused" 1
    case "$LAST_ERR" in
    *"run as root"* | *"systemd is not running"*)
        ok "real install refused before touching files"
        ;;
    *)
        not_ok "real install refusal message is unexpected"
        ;;
    esac

    : >"$CALL_LOG"
    run_installer env \
        PATH="$MOCK_BIN:$PATH" \
        GP_OS_RELEASE="$WORK/arch-release" \
        GP_TEST_CALL_LOG="$CALL_LOG" \
        bash "$INSTALLER" --uninstall
    assert_rc "uninstall without root is refused" 1
    assert_output_contains "uninstall root error" "$LAST_ERR" "run as root"
    if [ ! -s "$CALL_LOG" ]; then
        ok "uninstall guard did not invoke systemctl"
    else
        not_ok "uninstall guard invoked host commands: $(tr '\n' ';' <"$CALL_LOG")"
    fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

printf '\n%d passed, %d failed, %d skipped\n' "$PASS" "$FAIL" "$SKIP"
if [ "$FAIL" -ne 0 ]; then
    exit 1
fi
