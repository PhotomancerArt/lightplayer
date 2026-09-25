#!/usr/bin/env bash
set -euo pipefail

#
# Tests scripts/tag-next-version.sh against throwaway git repos: a bare
# "origin" and a clone standing in for a Main push checkout. Run by
# `just lint-tag-next-version` (in check-lint, so in CI).
#
# A concurrent run is simulated with the bare remote's `update` hook, which
# claims a tag itself the first time a push arrives and then refuses that
# push — exactly what the remote does when another run won the race.
#

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
TAG_SCRIPT="$SCRIPT_DIR/tag-next-version.sh"
PRINT_VERSION="$SCRIPT_DIR/print-app-version.sh"
DATE=2026.09.25

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

FAILURES=0
fail() { echo "FAIL: $*"; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $*"; }

export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.com
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.com
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
unset GITHUB_SHA TAG_SHA || true

# A fresh origin with main = A, and a clone of it. Echoes the clone's path.
fresh() {
    local name=$1
    local origin="$WORK/$name-origin.git" clone="$WORK/$name"
    git init --quiet --bare --initial-branch=main "$origin"
    git clone --quiet "$origin" "$clone" 2>/dev/null
    git -C "$clone" checkout --quiet -b main
    commit "$clone" A
    git -C "$clone" push --quiet origin main
    echo "$clone"
}

commit() {
    echo "$2" > "$1/$2"
    git -C "$1" add "$2"
    git -C "$1" commit --quiet -m "$2"
}

sha() { git -C "$1" rev-parse "$2"; }

# Runs the script in a clone, as a Main push run for $2.
run_tag() {
    local clone=$1 target=$2
    (cd "$clone" && BRANCH=main GITHUB_SHA="$target" TAG_DATE=$DATE "$TAG_SCRIPT") \
        >"$WORK/out.log" 2>&1
}

remote_tag() { git -C "$1" ls-remote origin "refs/tags/$2" | awk '{ print $1 }'; }
remote_tag_count() { git -C "$1" ls-remote --tags origin | grep -c "refs/tags/v$DATE-" || true; }

# --- 1. Back-to-back merges: each run tags its OWN commit, not the tip. ---
c=$(fresh race)
A=$(sha "$c" HEAD)
commit "$c" B
git -C "$c" push --quiet origin main
B=$(sha "$c" HEAD)
# The checkout is at A (the first run's GITHUB_SHA) while main is already B.
git -C "$c" checkout --quiet "$A"
if run_tag "$c" "$A" && [ "$(remote_tag "$c" "v$DATE-1")" = "$A" ]; then
    pass "first run tags its own commit A, not main's tip B"
else
    fail "first run did not tag A as -1"; cat "$WORK/out.log"
fi
if [ "$(cd "$c" && "$PRINT_VERSION" --require-tag)" = "$DATE-1" ]; then
    pass "print-app-version --require-tag sees the new tag in the checkout"
else
    fail "print-app-version did not see v$DATE-1"
fi
git -C "$c" checkout --quiet "$B"
if run_tag "$c" "$B" && [ "$(remote_tag "$c" "v$DATE-2")" = "$B" ]; then
    pass "second run tags B as -2"
else
    fail "second run did not tag B as -2"; cat "$WORK/out.log"
fi

# --- 2. A re-run for an already-tagged commit exits 0 and mints nothing. ---
if run_tag "$c" "$A" && [ "$(remote_tag_count "$c")" = 2 ] \
    && grep -q "already tagged v$DATE-1" "$WORK/out.log"; then
    pass "re-run on a tagged commit exits 0 without a new tag"
else
    fail "re-run on a tagged commit"; cat "$WORK/out.log"
fi

# The already-tagged path fetches the tag into a checkout that lacks it.
git -C "$c" tag -d "v$DATE-1" >/dev/null
git -C "$c" checkout --quiet "$A"
if run_tag "$c" "$A" && [ "$(cd "$c" && "$PRINT_VERSION" --require-tag)" = "$DATE-1" ]; then
    pass "already-tagged path fetches the tag for print-app-version"
else
    fail "already-tagged path left the checkout without the tag"
fi

# --- 3. Lost race to another commit: retry with the next number. ---
c=$(fresh lost)
commit "$c" C
git -C "$c" push --quiet origin main
C=$(sha "$c" HEAD)
commit "$c" D
git -C "$c" push --quiet origin main
D=$(sha "$c" HEAD)
hook="$WORK/lost-origin.git/hooks/update"
cat > "$hook" <<EOF
#!/bin/sh
# First push: a concurrent run for C claims -1 first; refuse this push.
if [ ! -e "$WORK/lost.claimed" ]; then
    touch "$WORK/lost.claimed"
    git update-ref "refs/tags/v$DATE-1" "$C"
    exit 1
fi
EOF
chmod +x "$hook"
if run_tag "$c" "$D" && [ "$(remote_tag "$c" "v$DATE-1")" = "$C" ] \
    && [ "$(remote_tag "$c" "v$DATE-2")" = "$D" ]; then
    pass "losing the race for -1 retries and claims -2"
else
    fail "lost race did not retry to -2"; cat "$WORK/out.log"
fi

# --- 4. Lost race to a run for the SAME commit: exit 0, no second tag. ---
c=$(fresh same)
E=$(sha "$c" HEAD)
hook="$WORK/same-origin.git/hooks/update"
cat > "$hook" <<EOF
#!/bin/sh
if [ ! -e "$WORK/same.claimed" ]; then
    touch "$WORK/same.claimed"
    git update-ref "refs/tags/v$DATE-7" "$E"
    exit 1
fi
EOF
chmod +x "$hook"
if run_tag "$c" "$E" && [ "$(remote_tag_count "$c")" = 1 ] \
    && grep -q "tagged v$DATE-7 by a concurrent run" "$WORK/out.log"; then
    pass "losing the race to a run for the same commit exits 0"
else
    fail "same-commit race"; cat "$WORK/out.log"
fi

# --- 5. Numbering: numeric, per date, annotated tags recognized. ---
c=$(fresh number)
F=$(sha "$c" HEAD)
commit "$c" G
git -C "$c" push --quiet origin main
G=$(sha "$c" HEAD)
git -C "$c" push --quiet origin "$F:refs/tags/v$DATE-9" "$F:refs/tags/v2026.09.24-40"
git -C "$c" tag -a -m annotated "v$DATE-10" "$F"
git -C "$c" push --quiet origin "refs/tags/v$DATE-10"
if run_tag "$c" "$G" && [ "$(remote_tag "$c" "v$DATE-11")" = "$G" ]; then
    pass "next number is numeric max + 1 for the date (-10 beats -9)"
else
    fail "numbering"; cat "$WORK/out.log"
fi
if run_tag "$c" "$F" && grep -q "already tagged v$DATE-10" "$WORK/out.log"; then
    pass "an annotated tag counts as already tagged (peeled sha)"
else
    fail "annotated tag not recognized"; cat "$WORK/out.log"
fi

# --- 6. Refusals. ---
c=$(fresh refuse)
git -C "$c" checkout --quiet -b side
commit "$c" S
S=$(sha "$c" HEAD)
if ! run_tag "$c" "$S" && grep -q "is not on origin/main" "$WORK/out.log"; then
    pass "a commit not on origin/main is refused"
else
    fail "off-main commit was not refused"; cat "$WORK/out.log"
fi
if ! (cd "$c" && BRANCH=side TAG_DATE=$DATE "$TAG_SCRIPT") >/dev/null 2>&1; then
    pass "a non-main branch is refused"
else
    fail "non-main branch was not refused"
fi

if [ "$FAILURES" -gt 0 ]; then
    echo "$FAILURES failure(s)"
    exit 1
fi
echo "all tag-next-version tests passed"
