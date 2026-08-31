#!/usr/bin/env bash
# Pick a dev-server port that lets multiple worktrees (agent sessions) coexist
# on one machine.
#
# Usage: scripts/dev-port.sh [--query] <service-name> [pinned-port]
#
# Prints the chosen port on stdout; everything else goes to stderr.
#
# --query computes the same port with NO side effects: no eviction, no
# probing. Use it to predict where a server will land (e.g. generating
# .claude/launch.json) without disturbing anything that is running. If a
# process outside this worktree holds the port, --query warns on stderr
# but still prints the hash slot — the real launch will probe past it.
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

# The bench block: a small port range reserved OUT of the hash space and
# granted standing WebSerial access by the serial-grant configuration
# profile (see `just serial-grant`). The Chromium device-access policies
# match exact origins — scheme, host, AND port — so the grant can only
# cover ports known when the profile was installed. Bench mode hands out
# ports from this block with the same semantics as hashing (never steal a
# foreign listener, evict own stale servers, printed URL is the source of
# truth). Use it for hardware-walk sessions ONLY: it partially
# reintroduces the shared-fixed-port risk that
# docs/defects/2026-07-27-launch-json-pinned-port.md exists to warn about,
# so ordinary dev stays on hashed ports. getPorts() probe sinks should
# also sit on block ports, or they escape the grant.
BENCH_BASE=36000
BENCH_SIZE=10

query=0
bench=0
while [[ "${1:-}" == --* ]]; do
    case "$1" in
        --query) query=1; shift ;;
        --bench) bench=1; shift ;;
        --bench-block) echo "${BENCH_BASE} $((BENCH_BASE + BENCH_SIZE - 1))"; exit 0 ;;
        *) echo "dev-port: unknown flag $1" >&2; exit 2 ;;
    esac
done

service="${1:?usage: dev-port.sh [--query] <service-name> [pinned-port]}"
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

if [[ "$bench" == 1 ]]; then
    if [[ -n "$pinned" ]]; then
        echo "dev-port: --bench and a pinned port are contradictory; drop one" >&2
        exit 2
    fi
    # Same rules as hash probing, confined to the block: free → take,
    # own stale server → evict and take, foreign → next slot.
    port="$BENCH_BASE"
    while [[ "$port" -lt $((BENCH_BASE + BENCH_SIZE)) ]]; do
        pids="$(listeners "$port")"
        if [[ -z "$pids" ]]; then
            echo "$port"
            exit 0
        fi
        if owned_by_this_worktree "$pids"; then
            if [[ "$query" == 1 ]]; then
                echo "$port"
                exit 0
            fi
            evict "$port" "$pids"
            echo "$port"
            exit 0
        fi
        echo "dev-port: bench port ${port} is held by another worktree's server; trying $((port + 1))" >&2
        port=$((port + 1))
    done
    echo "dev-port: bench block ${BENCH_BASE}-$((BENCH_BASE + BENCH_SIZE - 1)) is exhausted" >&2
    exit 1
fi

if [[ -n "$pinned" ]]; then
    pids="$(listeners "$pinned")"
    if [[ "$query" == 1 ]]; then
        if [[ -n "$pids" ]] && ! owned_by_this_worktree "$pids"; then
            echo "dev-port: pinned port ${pinned} is currently held by another process (pid ${pids//$'\n'/ }, not this worktree)" >&2
        fi
        echo "$pinned"
        exit 0
    fi
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

# Hashed ports never land in (or probe into) the bench block — those slots
# belong to bench mode.
skip_bench_block() {
    if [[ "$1" -ge "$BENCH_BASE" && "$1" -lt $((BENCH_BASE + BENCH_SIZE)) ]]; then
        echo $((BENCH_BASE + BENCH_SIZE))
    else
        echo "$1"
    fi
}

hash="$(printf '%s' "${worktree_root}:${service}" | cksum | cut -d' ' -f1)"
port="$(skip_bench_block $((PORT_BASE + hash % PORT_RANGE)))"

if [[ "$query" == 1 ]]; then
    pids="$(listeners "$port")"
    if [[ -n "$pids" ]] && ! owned_by_this_worktree "$pids"; then
        echo "dev-port: hash port ${port} is currently held by another worktree's server; a real launch would probe upward" >&2
    fi
    echo "$port"
    exit 0
fi

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
    port="$(skip_bench_block $((port + 1)))"
done

echo "dev-port: no free port found after ${MAX_PROBES} probes from the hash slot" >&2
exit 1
