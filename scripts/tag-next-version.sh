#!/usr/bin/env bash
set -euo pipefail

#
# Tags the current git commit with the next date-based version.
# Format: vYYYY.MM.DD-N (e.g. v2025.02.26-1)
#
# Only works on the main branch.
#

CURRENT_BRANCH=${BRANCH:-$(git rev-parse --abbrev-ref HEAD)}
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo "Error: This script can only be run on the main branch"
    echo "Current branch: $CURRENT_BRANCH"
    exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "Error: There are uncommitted changes"
    exit 1
fi

git pull origin main

CURRENT_COMMIT=$(git rev-parse HEAD)
if git tag --points-at "$CURRENT_COMMIT" | grep -qE 'v[0-9]{4}\.[0-9]{2}\.[0-9]{2}-[0-9]+'; then
    echo "Error: A version tag already exists for this commit"
    exit 1
fi

export TZ=America/Los_Angeles
DATE=$(date "+%Y.%m.%d")

LAST_BUILD=$(git ls-remote --tags origin | grep "${DATE}-" | sed 's/.*\///' | sort -V | tail -n1 | grep -oE '[0-9]+$' || echo "0")
LAST_BUILD=${LAST_BUILD:-0}
BUILD_NUM=$((10#${LAST_BUILD} + 1))

NEW_TAG="v${DATE}-$(printf "%d" ${BUILD_NUM})"

echo "Creating new tag: $NEW_TAG (Pacific Time)"

if [ "${CI:-}" = "true" ]; then
    git config --local user.email "github-actions[bot]@users.noreply.github.com"
    git config --local user.name "github-actions[bot]"
fi

git tag "$NEW_TAG"
git push origin "$NEW_TAG"

echo "Successfully created and pushed tag: $NEW_TAG"
