#!/usr/bin/env bash
#
# Bump the pinned nightly toolchain, then validate. See docs/toolchain-notes.md
# for why the toolchain is pinned at all.
#
# Usage:
#   scripts/bump-nightly.sh 2026-06-01   # pin to a specific dated nightly
#   scripts/bump-nightly.sh              # pin to today's nightly (UTC)
#
# What it does:
#   1. Rewrites the pin in rust-toolchain.toml and .github/workflows/pre-merge.yml.
#   2. Runs `just check` and reports.
#   3. Leaves all changes in the working tree for review; never commits.
#
# This script used to be a two-variable search: the `unwinding` crate was bound
# to the nightly `core::intrinsics::catch_unwind` ABI (0.2.8 integer return,
# 0.2.9 bool), so a toolchain bump could require advancing the crate in lockstep
# and the script did that speculatively, with a Cargo.lock snapshot to revert.
# `unwinding` is gone (ADR docs/adr/2026-08-02-rv32-firmwares-are-abort-tier.md),
# so nothing in the tree is coupled to a nightly's internal ABI and the bump is
# a one-variable change. A failure now means a real regression — a new clippy
# lint or genuine breakage — not a version-matching puzzle.

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

CURRENT_PIN="$(grep -Eo 'nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}' "$TOOLCHAIN_FILE" | head -1 || true)"
echo "Pinning toolchain: ${CURRENT_PIN:-<unpinned>} -> $CHANNEL"

# 1. Every rust-toolchain.toml in the repo (workspace root + per-crate pins, e.g.
#    lp-fw/fw-esp32c6 which the recipes `cd` into). They must all match or build-std
#    crates resolve a different, possibly unpinned, toolchain.
while IFS= read -r tc; do
    if grep -q '^channel = "esp"' "$tc"; then
        echo "  skipped (esp channel) $tc"
        continue
    fi
    sedi -E "s/^channel = \"nightly(-[0-9]{4}-[0-9]{2}-[0-9]{2})?\"/channel = \"$CHANNEL\"/" "$tc"
    grep -q "channel = \"$CHANNEL\"" "$tc" \
        || { echo "error: failed to update channel in $tc" >&2; exit 1; }
    echo "  pinned $tc"
done < <(find . -name rust-toolchain.toml -not -path './target/*' -not -path '*/target/*')

# 2. workflow: every `toolchain: nightly[-DATE]` (active + commented-out jobs, kept consistent)
sedi -E "s/(toolchain: )nightly(-[0-9]{4}-[0-9]{2}-[0-9]{2})?/\1$CHANNEL/g" "$WORKFLOW_FILE"

echo
echo "Validating with 'just check' (installs $CHANNEL and rebuilds core/alloc)..."
if just check; then
    echo
    echo "OK: $CHANNEL builds clean."
    echo "Review and commit:"
    echo "    git add $TOOLCHAIN_FILE $WORKFLOW_FILE && git commit"
    exit 0
fi

echo
echo "FAILED: 'just check' did not pass on $CHANNEL." >&2
echo "Toolchain edits are left in place for iteration. Inspect the output above," >&2
echo "or abandon the bump with:" >&2
echo "    git checkout $TOOLCHAIN_FILE $WORKFLOW_FILE" >&2
exit 1
