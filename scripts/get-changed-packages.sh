#!/bin/bash
# Get list of changed packages since a git ref

set -e

REF="${1:-origin/main}"

# Check if ref exists
if ! git rev-parse --verify "$REF" > /dev/null 2>&1; then
    echo "Error: git ref '$REF' not found" >&2
    exit 1
fi

# Get changed files
CHANGED_FILES=$(git diff --name-only "$REF"...HEAD 2>/dev/null || true)

if [ -z "$CHANGED_FILES" ]; then
    exit 0
fi

# Get workspace root
WORKSPACE_ROOT=$(cargo metadata --format-version 1 --no-deps 2>/dev/null | jq -r '.workspace_root' 2>/dev/null || pwd)

# Get all packages with their manifest paths
PACKAGES_JSON=$(cargo metadata --format-version 1 --no-deps 2>/dev/null | jq -r '.packages[] | "\(.name)|\(.manifest_path)"' 2>/dev/null || true)

# Find packages that match changed files
CHANGED_PACKAGES=""
while IFS= read -r file; do
    if [ -z "$file" ]; then
        continue
    fi
    
    # Normalize file path
    FILE_PATH="$WORKSPACE_ROOT/$file"
    
    # Check each package's manifest path
    while IFS='|' read -r pkg_name manifest_path; do
        # Get directory containing Cargo.toml
        PKG_DIR=$(dirname "$manifest_path")
        
        # Check if changed file is in this package's directory
        if [[ "$FILE_PATH" == "$PKG_DIR"/* ]] || [[ "$FILE_PATH" == "$manifest_path" ]]; then
            CHANGED_PACKAGES="${CHANGED_PACKAGES}${pkg_name} "
        fi
    done <<< "$PACKAGES_JSON"
done <<< "$CHANGED_FILES"

# Remove duplicates and print space-separated
echo "$CHANGED_PACKAGES" | tr ' ' '\n' | sort -u | tr '\n' ' ' | xargs
