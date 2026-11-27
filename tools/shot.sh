#!/bin/bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=$(realpath -m "${1:-/tmp/copilot-shots}")
SANDBOX=${COPILOT_SANDBOX:-/tmp/gnome-copilot-shot}
UUID=gnome-copilot@gnome-copilot.github.io
DEV=copilot-dev-unsafe@gnome-copilot.github.io
mkdir -p "$OUT" "$SANDBOX/config" "$SANDBOX/cache" "$SANDBOX/data/gnome-shell/extensions/$UUID" "$SANDBOX/data/gnome-shell/extensions/$DEV"
cp -rT "$ROOT/extension/$UUID" "$SANDBOX/data/gnome-shell/extensions/$UUID"
cp -rT "$ROOT/tools/testing/$DEV" "$SANDBOX/data/gnome-shell/extensions/$DEV"
export XDG_CONFIG_HOME="$SANDBOX/config" XDG_DATA_HOME="$SANDBOX/data" XDG_CACHE_HOME="$SANDBOX/cache"
export PATH="$ROOT/target/release:$PATH" OUT SCRIPT="${SCRIPT:-}" SCHEME="${SCHEME:-dark}" UUID DEV
exec dbus-run-session -- bash -c '
  gsettings set org.gnome.shell enabled-extensions "[\"$UUID\", \"$DEV\"]"
  gsettings set org.gnome.shell disable-user-extensions false
  gsettings set org.gnome.desktop.interface color-scheme "$([[ $SCHEME == dark ]] && echo prefer-dark || echo default)"
  copilotd > "$OUT/copilotd.log" 2>&1 &
  gnome-shell --headless --virtual-monitor 1440x900 > "$OUT/shell.log" 2>&1 &
  SHELL_PID=$!
  ev() { gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval "$1"; }
  for i in $(seq 60); do ev "1" 2>/dev/null | grep -q true && break; sleep 0.5; done
  sleep 2
  while IFS="|" read -r name js; do
    [[ -z "$name" || "$name" == \#* ]] && continue
    if [[ "$name" == sleep ]]; then sleep "$js"; continue; fi
    [[ -n "$js" ]] && echo "$name: $(ev "$js")"
    sleep 0.8
    gdbus call --session --dest org.gnome.Shell.Screenshot --object-path /org/gnome/Shell/Screenshot \
      --method org.gnome.Shell.Screenshot.Screenshot false false "$OUT/$name.png" > /dev/null
  done < "$SCRIPT"
  kill $SHELL_PID
'
