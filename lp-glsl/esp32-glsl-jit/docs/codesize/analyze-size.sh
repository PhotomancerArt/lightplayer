#!/bin/bash
# Script to analyze what's being compiled into embive-program for riscv32

set -e

# Change to repository root (where Cargo.toml is)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"
cd "$REPO_ROOT"

TARGET="riscv32imac-unknown-none-elf"
PACKAGE="embive-program"

echo "=========================================="
echo "Size Analysis for $PACKAGE"
echo "Target: $TARGET"
echo "=========================================="
echo ""

# Method 1: Build with verbose output to see all files being compiled
echo "=== Method 1: Verbose build to trace source files ==="
echo "Building with -v to see all rustc invocations..."
cargo build --package "$PACKAGE" --target "$TARGET" --release -v 2>&1 | \
    grep -E "Running.*rustc.*--crate-name" | \
    sed 's/.*--crate-name \([^ ]*\).*/\1/' | \
    sort -u > /tmp/crates-compiled.txt

echo "Crates compiled:"
cat /tmp/crates-compiled.txt
echo ""
echo "Total unique crates: $(wc -l < /tmp/crates-compiled.txt)"
echo ""

# Method 2: Dependency tree
echo "=== Method 2: Dependency tree ==="
cargo tree --package "$PACKAGE" --target "$TARGET" -e normal
echo ""

# Method 3: Binary size breakdown with cargo-bloat
echo "=== Method 3: Binary size breakdown (cargo bloat) ==="
if ! command -v cargo-bloat &> /dev/null; then
    echo "cargo-bloat not installed. Installing..."
    cargo install cargo-bloat
fi

cargo bloat --release --target "$TARGET" --package "$PACKAGE" -n 50
echo ""

# Method 4: Show actual binary size
echo "=== Method 4: Final binary size ==="
BINARY="target/$TARGET/release/$PACKAGE"
if [ -f "$BINARY" ]; then
    ls -lh "$BINARY"
    size "$BINARY"
else
    echo "Binary not found at $BINARY"
fi
echo ""

# Method 5: Strip and show size comparison
echo "=== Method 5: Size after stripping ==="
if [ -f "$BINARY" ]; then
    cp "$BINARY" "${BINARY}.stripped"
    rust-strip "${BINARY}.stripped" 2>/dev/null || strip "${BINARY}.stripped" 2>/dev/null || echo "Could not strip (no strip tool found)"
    if [ -f "${BINARY}.stripped" ]; then
        ls -lh "${BINARY}.stripped"
        size "${BINARY}.stripped"
        rm "${BINARY}.stripped"
    fi
fi
echo ""

echo "=========================================="
echo "Analysis complete!"
echo "=========================================="




