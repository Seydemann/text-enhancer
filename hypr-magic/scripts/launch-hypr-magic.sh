#!/usr/bin/env bash
set -euo pipefail

APP_DIR="/home/seydemann/product-for-me/hypr-magic"
BIN="$APP_DIR/target/release/hypr-magic"
ENV_FILE="$HOME/.config/hypr-magic/env"
LOCK_FILE="${XDG_RUNTIME_DIR:-/tmp}/hypr-magic.lock"

if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a
  source "$ENV_FILE"
  set +a
fi

if [[ ! -x "$BIN" ]]; then
  cd "$APP_DIR"
  cargo build --release
fi

# Prevent multiple instances from launching via wofi/menu.
if command -v flock >/dev/null 2>&1; then
  exec 9>"$LOCK_FILE"
  if ! flock -n 9; then
    exit 0
  fi
else
  if pgrep -x hypr-magic >/dev/null 2>&1; then
    exit 0
  fi
fi

exec "$BIN"
