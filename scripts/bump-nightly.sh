#!/usr/bin/env bash
#
# Bump the pinned nightly toolchain in lockstep with the ABI-coupled `unwinding`
# crate, then validate. See docs/toolchain-notes.md for why the toolchain is
# pinned and why `unwinding` must move with it.
#
# Usage:
#   scripts/bump-nightly.sh 2026-06-01   # pin to a specific dated nightly
#   scripts/bump-nightly.sh              # pin to today's nightly (UTC)
#
# What it does:
#   1. Rewrites the pin in rust-toolchain.toml and .github/workflows/pre-merge.yml.
#   2. Runs `just check` (compiles `unwinding` under build-std via clippy-rv32, and
#      catches new-nightly clippy lints) using the *current* `unwinding`.
#   3. Only if that fails: advances `unwinding` to the latest 0.2.x and re-checks
#      (a forward bump past the `catch_unwind` int->bool change needs 0.2.9+).
#   4. Leaves all changes in the working tree for review; never commits. On an
#      unrecoverable failure it reverts only the speculative `unwinding` bump and
#      reports — the toolchain edits stay so you can iterate.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$WORKSPACE_ROOT"

TOOLCHAIN_FILE="rust-toolchain.toml"
WORKFLOW_FILE=".github/workflows/pre-merge.yml"

# Resolve the target date: explicit arg, else today (UTC).
DATE="${1:-}"
if [ -z "$DATE" ]; then
    DATE="$(date -u +%Y-%m-%d)"
    echo "No date given; using today (UTC): $DATE"
fi
if ! printf '%s' "$DATE" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'; then
    echo "error: expected a date in YYYY-MM-DD form, got '$DATE'" >&2
    echo "usage: just bump-nightly [YYYY-MM-DD]   (no arg = today, UTC)" >&2
    exit 1
fi
CHANNEL="nightly-$DATE"

# Portable in-place sed: BSD (macOS) needs an explicit backup suffix arg to -i.
sedi() {
    if sed --version >/dev/null 2>&1; then sed -i "$@"; else sed -i '' "$@"; fi
}

unwinding_version() {
    grep -A1 -E '^name = "unwinding"$' Cargo.lock | grep -E '^version' | head -1 \
        | sed -E 's/version = "(.*)"/\1/'
}

CURRENT_PIN="$(grep -Eo 'nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}' "$TOOLCHAIN_FILE" | head -1 || true)"
echo "Pinning toolchain: ${CURRENT_PIN:-<unpinned>} -> $CHANNEL (unwinding currently $(unwinding_version))"

# 1. rust-toolchain.toml: channel = "nightly[-DATE]"
sedi -E "s/^channel = \"nightly(-[0-9]{4}-[0-9]{2}-[0-9]{2})?\"/channel = \"$CHANNEL\"/" "$TOOLCHAIN_FILE"
grep -q "channel = \"$CHANNEL\"" "$TOOLCHAIN_FILE" \
    || { echo "error: failed to update channel in $TOOLCHAIN_FILE" >&2; exit 1; }

# 2. workflow: every `toolchain: nightly[-DATE]` (active + commented-out jobs, kept consistent)
sedi -E "s/(toolchain: )nightly(-[0-9]{4}-[0-9]{2}-[0-9]{2})?/\1$CHANNEL/g" "$WORKFLOW_FILE"

# Snapshot Cargo.lock so we can revert a speculative unwinding bump if it doesn't help.
LOCK_BACKUP="$(mktemp)"
cp Cargo.lock "$LOCK_BACKUP"
trap 'rm -f "$LOCK_BACKUP"' EXIT

echo
echo "Validating with 'just check' (installs $CHANNEL, compiles unwinding under build-std)..."
if just check; then
    echo
    echo "OK: $CHANNEL builds clean with unwinding $(unwinding_version) (unchanged)."
    echo "Review and commit:"
    echo "    git add $TOOLCHAIN_FILE $WORKFLOW_FILE && git commit"
    exit 0
fi

echo
echo "Initial check failed. Advancing 'unwinding' to match the new nightly's catch_unwind ABI..."
cargo update -p unwinding
if cmp -s Cargo.lock "$LOCK_BACKUP"; then
    echo
    echo "FAILED: 'just check' did not pass and 'unwinding' is already at the latest 0.2.x." >&2
    echo "The failure is unrelated to the unwinding ABI (new clippy lint, other breakage)." >&2
    echo "Toolchain edits left in place. Inspect the output above, or abandon with:" >&2
    echo "    git checkout $TOOLCHAIN_FILE $WORKFLOW_FILE" >&2
    exit 1
fi

echo "  unwinding -> $(unwinding_version); re-validating..."
if just check; then
    echo
    echo "OK: $CHANNEL builds clean after bumping unwinding to $(unwinding_version)."
    echo "Review and commit (note the Cargo.lock change):"
    echo "    git add $TOOLCHAIN_FILE $WORKFLOW_FILE Cargo.lock && git commit"
    exit 0
fi

# Neither worked: drop the speculative unwinding bump, keep the toolchain edits.
cp "$LOCK_BACKUP" Cargo.lock
echo
echo "FAILED: $CHANNEL does not build even after bumping unwinding (reverted that bump)." >&2
echo "Toolchain edits are left in place for iteration. Options:" >&2
echo "  - pin a specific unwinding: cargo update -p unwinding --precise <ver>, then 'just check'" >&2
echo "  - if unwinding needs a new MAJOR (e.g. 0.3), bump the req in the crates' Cargo.toml" >&2
echo "  - abandon the bump: git checkout $TOOLCHAIN_FILE $WORKFLOW_FILE Cargo.lock" >&2
exit 1
