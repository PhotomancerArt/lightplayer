#!/usr/bin/env bash
# The director's CLI over the emulator perf lab (scripts/emu/lab/server.mjs).
#
#   scripts/emu/lab/lab.sh status              # devices, builds, job counts
#   scripts/emu/lab/lab.sh devices             # the device table
#   scripts/emu/lab/lab.sh home                # prints LAB_HOME
#   scripts/emu/lab/lab.sh token               # prints the token FILE's path, never the value
#   scripts/emu/lab/lab.sh curl /status        # authenticated curl passthrough (any curl args after the path)
#
# Talks to http://127.0.0.1:<port> — the director is on the desk, never through
# the tunnel. The port comes from $LAB_HOME/config.json, the token from
# $LAB_HOME/token (0600). LAB_HOME defaults to ~/.photomancer/emu-lab (D6).
#
# stderr is for chatter, stdout is for the value: `queue` prints a job id,
# `wait` prints a report path, `curl` prints the body.
set -euo pipefail

home="${LAB_HOME:-$HOME/.photomancer/emu-lab}"

usage() { sed -n '2,15p' "$0"; }

need_home() {
    [[ -f "$home/config.json" && -f "$home/token" ]] || {
        echo "lab: no lab at $home (start the server once: node scripts/emu/lab/server.mjs)" >&2
        exit 1
    }
}

port() { jq -r '.port // 41111' "$home/config.json"; }
token() { tr -d '\n' <"$home/token"; }
base() { echo "http://127.0.0.1:$(port)"; }

# `curl -sS -f` so a 401/404/408 is a non-zero exit the caller can see; the
# body is still printed on failure because the server's JSON error is the
# useful part.
api() {
    local p="$1"; shift
    curl -sS --show-error -H "Authorization: Bearer $(token)" "$@" "$(base)$p"
}

cmd="${1:-}"; [[ $# -gt 0 ]] && shift
case "$cmd" in
    home) echo "$home" ;;
    token) need_home; echo "$home/token" ;;
    status)
        need_home
        api /status | jq -r '
            "lab \(.home) :\(.port) up \(.uptimeS)s · builds \(.builds|length) · jobs queued \(.jobs.queued) running \(.jobs.running) done \(.jobs.done) · results \(.results)",
            (.devices[] | "device \(.id) \(.name // "-")  \(if .present then "PRESENT" else "away" end)  vis=\(.lastState.visibility // "?") lock=\(.lastState.wakeLock // "?")  seen \(.lastSeen // "-")"),
            (.builds[] | "build  \(.id)  \(.branch)\(if .dirty then " (dirty)" else "" end)  built \(.built_at)")' ;;
    devices)
        need_home
        api /status | jq -r '
            ["id","name","present","visibility","focus","lock","cores","lastSeen"], (.devices[] | [.id, (.name // "-"), (.present|tostring), (.lastState.visibility // "-"), (.lastState.hasFocus|tostring), (.lastState.wakeLock // "-"), (.cores|tostring), (.lastSeen // "-")]) | @tsv' | column -t -s $'\t' ;;
    curl)
        need_home
        p="${1:?lab curl needs a path}"; shift
        api "$p" "$@" ;;
    -h|--help|help|"") usage; [[ -n "$cmd" ]] || exit 2 ;;
    *) echo "lab: unknown command $cmd" >&2; usage >&2; exit 2 ;;
esac
