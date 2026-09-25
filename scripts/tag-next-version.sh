#!/usr/bin/env bash
set -euo pipefail

#
# Tags ONE commit of main with the next date-based version and pushes the tag.
# Format: vYYYY.MM.DD-N (e.g. v2025.02.26-1), N counted per Pacific date.
#
# Which commit: $TAG_SHA, else $GITHUB_SHA (the commit a "Main push" run was
# triggered for), else HEAD. Never the current tip of main: this script used
# to `git pull` and tag HEAD, so when two PRs merged close together the first
# run tagged the SECOND merge commit, the first stayed untagged, and its
# workflow_run deploy failed `print-app-version.sh --require-tag`
# (docs/defects/2026-09-25-tag-next-version-tagged-the-tip-not-its-commit.md).
#
# Idempotent and race-safe:
# - a commit that already carries a version tag on origin is left alone
#   (exit 0), and that tag is fetched so `print-app-version.sh` sees it;
# - the number is claimed by pushing a NEW ref, which the remote refuses if a
#   concurrent run took it first — we then re-read the remote and try the next
#   number. "Main push" also serializes runs with a concurrency group; this is
#   the belt to that brace (manual runs, re-runs, a changed group).
#
# Env: BRANCH (default: current branch; must be main), REMOTE (default:
# origin), TAG_DATE (YYYY.MM.DD override, for tests), TAG_MAX_ATTEMPTS
# (default 5). The tags are lightweight, so no committer identity is needed.
#

REMOTE=${REMOTE:-origin}
MAX_ATTEMPTS=${TAG_MAX_ATTEMPTS:-5}

CURRENT_BRANCH=${BRANCH:-$(git rev-parse --abbrev-ref HEAD)}
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo "Error: This script can only be run on the main branch"
    echo "Current branch: $CURRENT_BRANCH"
    exit 1
fi

TARGET_REF=${TAG_SHA:-${GITHUB_SHA:-HEAD}}
TARGET=$(git rev-parse --verify "${TARGET_REF}^{commit}")

# Only a commit that is actually on the remote's main may get a version.
git fetch --quiet "$REMOTE" main
if ! git merge-base --is-ancestor "$TARGET" FETCH_HEAD; then
    echo "Error: $TARGET is not on $REMOTE/main"
    exit 1
fi

# Version tags on the remote, as "<sha> <tag>" lines. Annotated tags list the
# peeled commit on a `^{}` line; that line wins over the tag object's own.
remote_version_tags() {
    git ls-remote --tags "$REMOTE" \
        | awk '{ ref = $2; sub("^refs/tags/", "", ref);
                 if (ref ~ /\^\{\}$/) { sub(/\^\{\}$/, "", ref); peeled[ref] = $1 }
                 else { plain[ref] = $1 } }
               END { for (r in plain) print ((r in peeled) ? peeled[r] : plain[r]), r }' \
        | grep -E " v[0-9]{4}\.[0-9]{2}\.[0-9]{2}-[0-9]+$" || true
}

# Makes the tag visible to this checkout (print-app-version.sh reads it).
fetch_tag() {
    git fetch --quiet --force "$REMOTE" "refs/tags/$1:refs/tags/$1"
}

existing_tag_for_target() {
    remote_version_tags | awk -v sha="$TARGET" '$1 == sha { print $2 }' | sort -V | tail -n1
}

EXISTING=$(existing_tag_for_target)
if [ -n "$EXISTING" ]; then
    echo "$TARGET is already tagged $EXISTING; nothing to do"
    fetch_tag "$EXISTING"
    exit 0
fi

DATE=${TAG_DATE:-$(TZ=America/Los_Angeles date "+%Y.%m.%d")}

for attempt in $(seq 1 "$MAX_ATTEMPTS"); do
    LAST_BUILD=$(remote_version_tags | awk '{ print $2 }' \
        | grep -E "^v${DATE//./\\.}-[0-9]+$" | grep -oE '[0-9]+$' | sort -n | tail -n1 || true)
    BUILD_NUM=$((10#${LAST_BUILD:-0} + 1))
    NEW_TAG="v${DATE}-${BUILD_NUM}"

    echo "Creating new tag: $NEW_TAG -> $TARGET (Pacific Time, attempt $attempt)"

    # Push the commit straight to the new ref: no local tag to collide with,
    # and the remote refuses it if the name was taken since we looked.
    if git push --quiet "$REMOTE" "$TARGET:refs/tags/$NEW_TAG"; then
        fetch_tag "$NEW_TAG"
        echo "Successfully created and pushed tag: $NEW_TAG"
        exit 0
    fi

    # Lost a race. Maybe the winner was another run for this same commit.
    EXISTING=$(existing_tag_for_target)
    if [ -n "$EXISTING" ]; then
        echo "$TARGET was tagged $EXISTING by a concurrent run; nothing to do"
        fetch_tag "$EXISTING"
        exit 0
    fi
    echo "Tag $NEW_TAG was taken concurrently; retrying"
done

echo "Error: could not claim a version tag after $MAX_ATTEMPTS attempts"
exit 1
