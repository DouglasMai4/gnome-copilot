#!/bin/bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
UUID=gnome-copilot@gnome-copilot.github.io
SANDBOX=${COPILOT_SANDBOX:-/tmp/gnome-copilot-dev}

mkdir -p "$SANDBOX/config" "$SANDBOX/cache" "$SANDBOX/data/gnome-shell/extensions/$UUID"
cp -rT "$ROOT/extension/$UUID" "$SANDBOX/data/gnome-shell/extensions/$UUID"
if [[ -d "$HOME/.local/share/gnome-copilot" && ! -e "$SANDBOX/data/gnome-copilot" ]]; then
  ln -s "$HOME/.local/share/gnome-copilot" "$SANDBOX/data/gnome-copilot"
fi

export XDG_CONFIG_HOME="$SANDBOX/config" XDG_DATA_HOME="$SANDBOX/data" XDG_CACHE_HOME="$SANDBOX/cache"
export PATH="$ROOT/target/release:$PATH"
exec dbus-run-session -- bash -c "
  copilotd &
  gsettings set org.gnome.shell enabled-extensions \"['$UUID']\"
  gsettings set org.gnome.shell disable-user-extensions false
  exec gnome-shell --devkit --wayland
"
