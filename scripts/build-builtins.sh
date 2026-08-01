#!/bin/bash
# Build lps-builtins-emu-app executable with aggressive optimizations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LIGHTPLAYER_DIR="$WORKSPACE_ROOT/lp-shader"
TARGET="riscv32imac-unknown-none-elf"
OUTPUT_DIR="$WORKSPACE_ROOT/target/$TARGET/release"
BINARY="$OUTPUT_DIR/lps-builtins-emu-app"
BUILTINS_SRC_DIR="$LIGHTPLAYER_DIR/lps-builtins/src/builtins"
BUILTIN_GEN_DIR="$LIGHTPLAYER_DIR/lps-builtins-gen-app"
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
  echo "$1" >"$HASH_FILE"
}

# Check if we need to regenerate
current_hash=$(compute_source_hash)
stored_hash=$(get_stored_hash)

if [ "$current_hash" != "$stored_hash" ]; then
  echo "Building lps-builtins-emu-app for $TARGET with aggressive optimizations..."
  echo "Generating builtin boilerplate..."
  cd "$LIGHTPLAYER_DIR"
  cargo run --bin lps-builtins-gen-app --manifest-path lps-builtins-gen-app/Cargo.toml
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
  --package lps-builtins-emu-app \
  --release \
  --bin lps-builtins-emu-app \
  --features float-f32

# Count symbols
LP_SYMBOLS=$(nm "$BINARY" 2>/dev/null | grep "__lp_" | wc -l | xargs)
F32_SYMBOLS=$(nm "$BINARY" 2>/dev/null | grep -c "_f32$" | xargs)

# The image exists to carry builtin symbols; one that lost them to dead-code
# elimination links "successfully" and then calls a null pointer at run time.
# `lpvm-cranelift`'s linker additionally asserts that *every* `BuiltinId` symbol
# is present, so a missing f32 family fails 800+ filetests with one opaque
# message — cheaper to catch here, by name.
if [ "$LP_SYMBOLS" -eq 0 ]; then
  echo "error: $BINARY contains no __lp_ symbols (dead-code elimination?)" >&2
  exit 1
fi
if [ "$F32_SYMBOLS" -eq 0 ]; then
  echo "error: $BINARY contains no native-f32 builtins (is --features float-f32 set?)" >&2
  exit 1
fi

# Output formatted results
GREEN='\033[0;32m'
NC='\033[0m' # No Color
echo -e "${GREEN}lps-builtins-emu-app:${NC} built with $LP_SYMBOLS built-ins ($F32_SYMBOLS native-f32)"
