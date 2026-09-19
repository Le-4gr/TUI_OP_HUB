#!/usr/bin/env bash
# TUI-OP-HUB release installer.
#
# Builds the release binary, installs it, and enables the background service
# (systemd user service when available). After this, the hub always runs in
# the background (scheduler + REST API) and you can launch the TUI anytime
# with `tui-op-hub`.
#
# Usage:  ./install.sh            build + install + enable service
#         ./install.sh --no-service  build + install, skip the service
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$REPO_DIR/TUI-OP-HUB"
BIN_NAME="tui-op-hub"

echo "==> Building release binary (this can take a while)"
cargo build --release --manifest-path "$CRATE_DIR/Cargo.toml"
BUILT="$CRATE_DIR/target/release/$BIN_NAME"
[ -x "$BUILT" ] || { echo "Build did not produce $BUILT"; exit 1; }

# Choose the install prefix
if [ "${EUID:-$(id -u)}" -eq 0 ]; then
    PREFIX="/usr/local/bin"
else
    PREFIX="$HOME/.local/bin"
    mkdir -p "$PREFIX"
fi

echo "==> Installing $BUILT -> $PREFIX/$BIN_NAME"
install -m 755 "$BUILT" "$PREFIX/$BIN_NAME"

case ":$PATH:" in
    *":$PREFIX:"*) ;;
    *) echo "NOTE: $PREFIX is not in your PATH. Add it to your shell profile." ;;
esac

if [ "${1:-}" = "--no-service" ]; then
    echo "==> Skipping service setup (--no-service)"
    echo "Done. Run the TUI with: $BIN_NAME"
    exit 0
fi

echo "==> Setting up the background service"
# systemd user service (includes generating the secrets-key env file once);
# on systems without systemctl the unit file is still written and the app
# can be wired into cron or any other init (see docs/GUIDE.md).
if command -v systemctl >/dev/null 2>&1; then
    "$PREFIX/$BIN_NAME" --install-service
    echo "==> Background service enabled (always runs after reboot)"
    echo "    Check it:   systemctl --user status $BIN_NAME"
    echo "    Logs:       journalctl --user -u $BIN_NAME -f"
else
    "$PREFIX/$BIN_NAME" --install-service || true
    echo "NOTE: systemctl not found. The unit file was written to"
    echo "      ~/.config/systemd/user/$BIN_NAME.service"
    echo "      Wire '$BIN_NAME --headless' into cron or your init of choice."
fi

cat <<EOF

Done!
  * Background: service is enabled now and after reboot (scheduler + API).
  * TUI:        run '$BIN_NAME' whenever you want — it shares the same
                database and skips starting a second API if one is running.
  * Secrets:    master key generated at ~/.config/tui-op-hub/env (mode 600).
                Back it up — without it existing secrets cannot be decrypted.
EOF
