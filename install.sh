#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install rust (e.g. sudo pacman -S rust)." >&2
    exit 1
fi

cargo build --release --locked

if [ "${1:-}" = "--system" ]; then
    sudo install -Dm755 target/release/gp-tui /usr/local/bin/gp-tui
    echo "installed /usr/local/bin/gp-tui"
else
    dest="${XDG_BIN_HOME:-$HOME/.local/bin}"
    install -Dm755 target/release/gp-tui "$dest/gp-tui"
    echo "installed $dest/gp-tui"
    case ":$PATH:" in
    *":$dest:"*) ;;
    *)
        echo "note: add $dest to PATH to run 'gp-tui' directly." >&2
        ;;
    esac
fi
