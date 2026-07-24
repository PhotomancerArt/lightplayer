#!/usr/bin/env bash
# Pick a dev-server port that lets multiple worktrees (agent sessions) coexist
# on one machine.
#
# Usage: scripts/dev-port.sh <service-name> [pinned-port]
#
# Prints the chosen port on stdout; everything else goes to stderr.
#
# The default port is derived from a hash of (worktree root, service name), so
# each worktree gets a stable port across restarts and different worktrees
# almost never collide. If the port is already bound:
#   - by a process whose cwd is inside THIS worktree → kill it (last-wins:
#     restarting a dev server always evicts its own stale predecessor);
#   - by anything else (a genuine hash collision with a live session) → probe
#     upward to the next free port instead of killing someone else's server.
# A pinned port (arg 2 or the caller's env var) skips hashing and probing:
# same-worktree occupants are still evicted, but a foreign occupant is a hard
# error — pinned means pinned.
set -euo pipefail

PORT_BASE=20000
PORT_RANGE=20000
MAX_PROBES=50

service="${1:?usage: dev-port.sh <service-name> [pinned-port]}"
pinned="${2:-}"

worktree_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# Pids listening on a TCP port, if any.
listeners() {
    lsof -nP -iTCP:"$1" -sTCP:LISTEN -t 2>/dev/null || true
}

# True if every listener on the port has its cwd inside this worktree.
owned_by_this_worktree() {
    local pid cwd
    for pid in $1; do
        cwd="$(lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1)"
        case "$cwd" in
            "$worktree_root" | "$worktree_root"/*) ;;
            *) return 1 ;;
        esac
    done
    return 0
}

evict() {
    local port="$1" pids="$2" i
    echo "dev-port: evicting stale ${service} server on port ${port} (pid ${pids//$'\n'/ })" >&2
    kill $pids 2>/dev/null || true
    for i in $(seq 1 50); do
        [[ -z "$(listeners "$port")" ]] && return 0
        sleep 0.1
    done
    kill -9 $pids 2>/dev/null || true
    for i in $(seq 1 20); do
        [[ -z "$(listeners "$port")" ]] && return 0
        sleep 0.1
    done
    echo "dev-port: port ${port} still bound after killing pid ${pids//$'\n'/ }" >&2
    return 1
}

if [[ -n "$pinned" ]]; then
    pids="$(listeners "$pinned")"
    if [[ -n "$pids" ]]; then
        if owned_by_this_worktree "$pids"; then
            evict "$pinned" "$pids"
        else
            echo "dev-port: pinned port ${pinned} is in use by another process (pid ${pids//$'\n'/ }, not this worktree). Refusing to steal it." >&2
            exit 1
        fi
    fi
    echo "$pinned"
    exit 0
fi

hash="$(printf '%s' "${worktree_root}:${service}" | cksum | cut -d' ' -f1)"
port=$((PORT_BASE + hash % PORT_RANGE))

for _ in $(seq 1 "$MAX_PROBES"); do
    pids="$(listeners "$port")"
    if [[ -z "$pids" ]]; then
        echo "$port"
        exit 0
    fi
    if owned_by_this_worktree "$pids"; then
        evict "$port" "$pids"
        echo "$port"
        exit 0
    fi
    echo "dev-port: port ${port} is held by another worktree's server; probing ${port}+1" >&2
    port=$((port + 1))
done

echo "dev-port: no free port found after ${MAX_PROBES} probes from the hash slot" >&2
exit 1
