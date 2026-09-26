#!/usr/bin/env bash
# Build the host `lp-cli` a pinned-firmware walk talks to.
#
#   scripts/emu/build-reference-cli.sh <commit>
#   → target/emu-ref/<commit>-lp-cli/lp-cli
#
# `lp-cli/tests/emu_serve_walk.rs` uploads to the reference image at
# d6cfaa205, which speaks wire protocol 20 for ever. The current `lp-cli`
# speaks whatever `WIRE_PROTO_VERSION` is today, and the handshake refuses a
# mismatch — correctly — so a test that paired the two broke on the first wire
# bump after the image was pinned (20 → 21, PR #785). The walk now uploads
# with a client pinned the same way the firmware is: the LAST commit that
# speaks the image's protocol. That pair never drifts, whatever the wire does
# next.
#
# The client cannot be d6cfaa205 itself: `serial:ws://` — the only way to
# reach `lp-cli emu serve`'s door — landed on 2026-09-08 (76c67d592), three
# days after that commit. Any commit from then until the 20 → 21 bump is a
# proto-20 client that can reach the door; the test pins one.
#
# Same shape as `build-reference-image.sh`: a detached worktree at the commit
# under target/emu-ref/, a cold target directory of its own, a `mkdir` lock so
# parallel tests build it once, and a no-op when the binary is already there.
set -euo pipefail

commit="${1:?usage: build-reference-cli.sh <commit>}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"

out_dir="$repo/target/emu-ref/$commit-lp-cli"
bin="$out_dir/lp-cli"
wt="$repo/target/emu-ref/wt-$commit-lp-cli"
lock="$repo/target/emu-ref/.build.lock"

if [[ -x "$bin" ]]; then
    echo "build-reference-cli: $bin is already built"
    exit 0
fi

mkdir -p "$(dirname "$lock")"
waited=0
until mkdir "$lock" 2>/dev/null; do
    if [[ -x "$bin" ]]; then
        echo "build-reference-cli: another process published $bin while we waited"
        exit 0
    fi
    if (( waited >= ${LOCK_TIMEOUT:-900} )); then
        echo "build-reference-cli: waited ${waited}s for $lock; remove it if it is stale" >&2
        exit 4
    fi
    sleep 1
    waited=$((waited + 1))
done
trap 'rmdir "$lock" 2>/dev/null || true' EXIT

full_commit="$(git -C "$repo" rev-parse "$commit")"
if [[ ! -d "$wt" ]]; then
    git -C "$repo" worktree prune
    git -C "$repo" worktree add --detach "$wt" "$full_commit"
fi
head_now="$(git -C "$wt" rev-parse HEAD)"
if [[ "$head_now" != "$full_commit" ]]; then
    echo "build-reference-cli: worktree $wt is at $head_now, expected $full_commit — remove it and re-run" >&2
    exit 1
fi

echo "build-reference-cli: building lp-cli at $commit in $wt"
(cd "$wt" && CARGO_TARGET_DIR="$out_dir/target" cargo build --release -p lp-cli)
mkdir -p "$out_dir"
cp "$out_dir/target/release/lp-cli" "$bin"
echo "build-reference-cli: $bin"
