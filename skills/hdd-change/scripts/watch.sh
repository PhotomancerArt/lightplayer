#!/usr/bin/env bash
# Debounced directory watcher for HDD skills.
# Usage: watch.sh <dir> [debounce-seconds]
# Prints one line listing changed files each time edits settle for the debounce period.
set -u
dir=${1:?usage: watch.sh <dir> [debounce-seconds]}
debounce=${2:-30}
cd "$dir" || exit 1

snapshot() {
  find . -type f ! -name '.DS_Store' -exec cksum {} + 2>/dev/null | sort -k3
}

base=$(snapshot)
last=$base
stable_since=$(date +%s)
while true; do
  sleep 3
  cur=$(snapshot)
  now=$(date +%s)
  if [ "$cur" != "$last" ]; then
    last=$cur
    stable_since=$now
    continue
  fi
  if [ "$cur" != "$base" ] && [ $((now - stable_since)) -ge "$debounce" ]; then
    changed=$(diff <(echo "$base") <(echo "$cur") | grep -E '^[<>]' | cut -d' ' -f4- | sort -u | paste -sd' ' -)
    echo "settled changes in $dir: $changed"
    base=$cur
  fi
done
