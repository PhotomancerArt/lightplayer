#!/bin/bash
# Build lp-builtins-app executable with aggressive optimizations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LIGHTPLAYER_DIR="$WORKSPACE_ROOT/lp-glsl"
BUILTINS_APP="$LIGHTPLAYER_DIR/apps/lp-builtins-app"
TARGET="riscv32imac-unknown-none-elf"
OUTPUT_DIR="$LIGHTPLAYER_DIR/target/$TARGET/release"
BINARY="$OUTPUT_DIR/lp-builtins-app"
BUILTINS_SRC_DIR="$LIGHTPLAYER_DIR/crates/lp-builtins/src/builtins"
BUILTIN_GEN_DIR="$LIGHTPLAYER_DIR/apps/lp-builtin-gen"
HASH_FILE="$WORKSPACE_ROOT/.builtins-source-hash"

# Compute hash of all builtin source files and generator
compute_source_hash() {
    {
        if [ -d "$BUILTINS_SRC_DIR" ]; then
            find "$BUILTINS_SRC_DIR" -name "*.rs" -type f | sort | xargs cat 2>/dev/null || true
        fi
        if [ -d "$BUILTIN_GEN_DIR" ]; then
            find "$BUILTIN_GEN_DIR" -name "*.rs" -type f | sort | xargs cat 2>/dev/null || true
        fi
    } | shasum -a 256 | cut -d' ' -f1
}

# Get stored hash
get_stored_hash() {
    if [ -f "$HASH_FILE" ]; then
        cat "$HASH_FILE"
    else
        echo ""
    fi
}

# Store hash
store_hash() {
    echo "$1" > "$HASH_FILE"
}

# Check if we need to regenerate
current_hash=$(compute_source_hash)
stored_hash=$(get_stored_hash)

if [ "$current_hash" != "$stored_hash" ]; then
    echo "Building lp-builtins-app for $TARGET with aggressive optimizations..."
    echo "Generating builtin boilerplate..."
    cd "$LIGHTPLAYER_DIR"
    cargo run --bin lp-builtin-gen --manifest-path apps/lp-builtin-gen/Cargo.toml
    store_hash "$current_hash"
else
    echo "Builtins source unchanged, skipping code generation..."
fi

# Ensure target is installed
if ! rustup target list --installed | grep -q "^$TARGET$"; then
    echo "Installing target $TARGET..."
    rustup target add $TARGET
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Build using cargo but with RUSTFLAGS for optimization
# Cargo will handle its own caching
cd "$LIGHTPLAYER_DIR"

RUSTFLAGS="-C opt-level=1 \
           -C panic=abort \
           -C overflow-checks=off \
           -C debuginfo=0 \
           -C link-dead-code=off \
           -C codegen-units=1" \
cargo build \
    --target $TARGET \
    --package lp-builtins-app \
    --release \
    --bin lp-builtins-app

# Count symbols
LP_SYMBOLS=$(nm "$BINARY" 2>/dev/null | grep "__lp_" | wc -l | xargs)

# Output formatted results
GREEN='\033[0;32m'
NC='\033[0m' # No Color
echo -e "${GREEN}lp-builtins-app:${NC} built with $LP_SYMBOLS built-ins"

