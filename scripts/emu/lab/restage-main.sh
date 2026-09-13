#!/usr/bin/env bash
# Keep the newest `main` queueable with nobody in the loop — the perf lab's
# first follow-up (ADR 2026-09-12-emulator-perf-lab-job-queue-with-presence).
#
#   scripts/emu/lab/restage-main.sh          # by hand; launchd runs it otherwise
#
# Every fire of `com.yona.emu-lab-restage` (StartInterval, 10 minutes) this
# fetches `origin main` into the PRIMARY checkout's object store — a fetch,
# never a checkout, so no working tree is touched — and, when that head has no
# build in `$LAB_HOME/builds/<short>/`, hands it to `lab.sh stage`. The
# director then has the newest head in the store without staging it by hand.
#
# **One line per run** goes to `$LAB_HOME/log/restage.log`, skips included;
# the build's own minutes of chatter are launchd's `restage.stdout.log`.
#
# A build takes minutes and the timer keeps firing under it, so `lab.sh stage`
# holds an atomic lock at `$LAB_HOME/.stage/lock` (pid + what it is building)
# and this script peeks at it before doing any work: a fire that lands on a
# running stage — a director's hand-run one included — logs `busy` and exits
# 0. The lock is the only overlap guard; nothing here waits.
#
# launchd sources no rc file, so the plist spells PATH out (cargo lives under
# ~/.cargo/bin) and names an absolute interpreter.
set -euo pipefail

home="${LAB_HOME:-$HOME/.photomancer/emu-lab}"
here="$(cd "$(dirname "$0")" && pwd)"
lock="$home/.stage/lock"
started="$(date +%s)"

log() {
    mkdir -p "$home/log"
    printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >>"$home/log/restage.log"
    # A hand run says it out loud; under launchd stderr is a different file
    # (restage.stdout.log) and one copy of the line is enough.
    [[ -t 2 ]] && printf 'restage: %s\n' "$*" >&2 || true
}
elapsed() { echo "$(( $(date +%s) - started ))s"; }

[[ -f "$home/config.json" && -f "$home/token" ]] || { log "no lab at $home"; exit 1; }

# The lock `lab.sh stage` holds. A stale one (its pid is gone) is not this
# script's business — `lab.sh stage` clears it when it takes it.
if [[ -d "$lock" ]]; then
    pid="$(cat "$lock/pid" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        log "busy: a stage is already running (pid $pid, $(cat "$lock/what" 2>/dev/null || echo '?'))"
        exit 0
    fi
fi

# `lab.sh stage` builds out of the primary checkout, so that is where the ref
# has to land. No `head` in the pipeline: under pipefail a closed pipe makes
# git exit non-zero and `set -e` would leave silently.
primary="$(git -C "$here" worktree list --porcelain | sed -n '1s/^worktree //p')"
git -C "$primary" fetch --quiet origin main || { log "fetch failed (origin main in $primary)"; exit 1; }
full="$(git -C "$primary" rev-parse --verify FETCH_HEAD^{commit})" || { log "origin/main did not resolve to a commit"; exit 1; }
short="${full:0:7}"

[[ ! -d "$home/builds/$short" ]] || { log "$short already in the store"; exit 0; }

# `lab.sh stage` refuses the primary checkout's own HEAD while that tree is
# dirty. That guard is about a director staging what they are looking at; here
# it is just a reason to wait, not a failure, and it clears itself.
if [[ "$full" == "$(git -C "$primary" rev-parse HEAD)" && -n "$(git -C "$primary" status --porcelain)" ]]; then
    log "skip $short: it is the primary checkout's HEAD and that tree is dirty"
    exit 0
fi

if id="$("$here/lab.sh" stage "$full")"; then
    log "staged $short as $id in $(elapsed)"
else
    rc=$?
    [[ $rc -ne 3 ]] || { log "busy: lab.sh stage found the lock taken ($short)"; exit 0; }
    log "FAILED to stage $short after $(elapsed) (rc $rc — see log/restage.stdout.log)"
    exit 1
fi
