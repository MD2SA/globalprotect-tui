#!/usr/bin/env bash
# Compatibility entry point. The installer lives in installers/arch/.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
exec "$root/installers/arch/gp_install_arch.sh" "$@"
