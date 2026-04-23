#!/usr/bin/env bash
# Wipe all on-disk state for Friends Watcher so the next launch behaves like
# a fresh install: no logged-in user, no snapshot history, no cached avatars.
# Quits any running instance before deleting.
#
# macOS uses two different identifiers for per-app storage depending on how
# the binary is launched:
#   - Release .app bundle  -> tauri.conf.json's `identifier` (com.friendswatcher.app)
#   - `cargo run` / dev    -> binary basename (friends-watcher)
# so we wipe both sets.

set -euo pipefail

IDENTIFIERS=("com.friendswatcher.app" "friends-watcher")

# Subdirectories (one per identifier).
SUBDIRS=(
  "Library/WebKit"
  "Library/Application Support"
  "Library/Caches"
  "Library/Logs"
)

# Binary cookie files (one file per identifier, not a directory).
COOKIE_PARENT="Library/HTTPStorages"

if pgrep -f "target/debug/friends-watcher" >/dev/null || pgrep -x "Friends Watcher" >/dev/null; then
  echo "Quitting Friends Watcher..."
  pkill -f "target/debug/friends-watcher" 2>/dev/null || true
  pkill -x "Friends Watcher" 2>/dev/null || true
  sleep 1
fi

remove_if_exists() {
  local path=$1
  if [[ -e $path ]]; then
    rm -rf -- "$path"
    echo "removed $path"
  else
    echo "skipped $path (absent)"
  fi
}

for id in "${IDENTIFIERS[@]}"; do
  for sub in "${SUBDIRS[@]}"; do
    remove_if_exists "$HOME/$sub/$id"
  done
  remove_if_exists "$HOME/$COOKIE_PARENT/$id.binarycookies"
done

echo
echo "Fresh-install reset complete. Next launch will show the landing screen."
