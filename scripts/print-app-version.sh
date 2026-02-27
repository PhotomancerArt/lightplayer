#!/usr/bin/env bash
set -euo pipefail

# Prints the app version for the current commit.
# If a date-based tag exists (vYYYY.MM.DD-N), outputs that without the 'v'.
# Otherwise outputs branch@sha (or sha-dirty-timestamp for uncommitted changes).
#
# Usage: scripts/print-app-version.sh [--require-tag]

REQUIRE_TAG=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --require-tag)
            REQUIRE_TAG=true
            shift
            ;;
        *)
            echo "Error: Unknown argument $1"
            echo "Usage: $0 [--require-tag]"
            exit 1
            ;;
    esac
done

COMMIT=$(git rev-parse HEAD)
TAG=$(git tag --points-at "$COMMIT" | grep -E '^v[0-9]{4}\.[0-9]{2}\.[0-9]{2}-[0-9]+$' | sort -V | tail -n 1 || true)

if [ -n "$TAG" ]; then
    echo "${TAG#v}"
    exit 0
fi

if [ "$REQUIRE_TAG" = "true" ]; then
    echo
    echo "Error: The current commit is not tagged with a version." >&2
    echo
    exit 1
fi

SHA=$(git rev-parse --short HEAD)
DIRTY=$(git status --porcelain | wc -l | tr -d ' ')
if [ "$DIRTY" -gt 0 ]; then
    TIMESTAMP="$(TZ="America/Los_Angeles" date +"%H%M%S")PT"
    DIRTY="-dirty-$TIMESTAMP"
fi
echo "$SHA$DIRTY"
