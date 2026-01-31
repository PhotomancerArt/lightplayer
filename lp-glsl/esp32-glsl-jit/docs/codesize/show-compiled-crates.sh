#!/bin/bash
# Simple script to show which crates get compiled for embive-program

set -e

# Change to repository root (where Cargo.toml is)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"
cd "$REPO_ROOT"

TARGET="riscv32imac-unknown-none-elf"
PACKAGE="embive-program"

echo "=========================================="
echo "Crates compiled for $PACKAGE"
echo "=========================================="
echo ""

echo "1. Dependency Tree (what gets pulled in):"
echo "------------------------------------------"
cargo tree --package "$PACKAGE" --target "$TARGET" -e normal --prefix none | \
    sed 's/ (.*)//' | \
    awk '{print $1}' | \
    sort -u | \
    head -30

echo ""
echo "Total unique crates in tree: $(cargo tree --package "$PACKAGE" --target "$TARGET" -e normal --prefix none | sed 's/ (.*)//' | awk '{print $1}' | sort -u | wc -l)"
echo ""

echo "2. Feature Flags Enabled:"
echo "-------------------------"
echo "embive-program:"
cargo metadata --format-version=1 --no-deps 2>/dev/null | \
    jq -r ".packages[] | select(.name == \"$PACKAGE\") | .features | keys[]" 2>/dev/null || \
    echo "  (use 'cargo metadata' to see features)"

echo ""
echo "3. Current Binary Info:"
echo "-----------------------"
BINARY="target/$TARGET/release/$PACKAGE"
if [ -f "$BINARY" ]; then
    echo "File: $BINARY"
    ls -lh "$BINARY" | awk '{print "Size:", $5}'
    file "$BINARY" | sed 's/.*: //'
    echo ""
    echo "Sections:"
    if command -v llvm-size &> /dev/null; then
        llvm-size "$BINARY"
    elif command -v size &> /dev/null; then
        size "$BINARY" 2>/dev/null || echo "  (size command doesn't support RISC-V)"
    else
        echo "  (install llvm-tools: rustup component add llvm-tools)"
    fi
else
    echo "Binary not found. Build with:"
    echo "  cargo build --package $PACKAGE --target $TARGET --release"
fi

echo ""
echo "=========================================="
echo "Quick size optimization tips:"
echo "  1. Add 'strip = true' to [profile.release]"
echo "  2. Add 'opt-level = \"z\"' for size optimization"
echo "  3. Run: cargo bloat --release --target $TARGET -p $PACKAGE"
echo "=========================================="




