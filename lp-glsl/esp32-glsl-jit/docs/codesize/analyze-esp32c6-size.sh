#!/bin/bash
# Script to analyze what's being compiled into esp32c6-glsl-jit for riscv32

set -e

# Change to repository root (where Cargo.toml is)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"
cd "$REPO_ROOT"

TARGET="riscv32imac-unknown-none-elf"
PACKAGE="esp32c6-glsl-jit"

echo "=========================================="
echo "Size Analysis for $PACKAGE"
echo "Target: $TARGET"
echo "=========================================="
echo ""

# Method 1: Build with verbose output to see all files being compiled
echo "=== Method 1: Verbose build to trace source files ==="
echo "Building with -v to see all rustc invocations..."
if cargo build --package "$PACKAGE" --target "$TARGET" --release -v 2>&1 | \
    grep -E "Running.*rustc.*--crate-name" | \
    sed 's/.*--crate-name \([^ ]*\).*/\1/' | \
    sort -u > /tmp/crates-compiled-esp32c6.txt 2>/dev/null; then
    echo "Crates compiled:"
    cat /tmp/crates-compiled-esp32c6.txt
    echo ""
    echo "Total unique crates: $(wc -l < /tmp/crates-compiled-esp32c6.txt)"
else
    echo "Build failed, but continuing with dependency analysis..."
fi
echo ""

# Method 2: Dependency tree
echo "=== Method 2: Dependency tree ==="
cargo tree --package "$PACKAGE" --target "$TARGET" -e normal 2>/dev/null || echo "Could not generate dependency tree"
echo ""

# Method 3: Dependency tree with features
echo "=== Method 3: Dependency tree with features ==="
cargo tree --package "$PACKAGE" --target "$TARGET" -e features 2>/dev/null || echo "Could not generate feature tree"
echo ""

# Method 4: Binary size breakdown with cargo-bloat
echo "=== Method 4: Binary size breakdown (cargo bloat) ==="
if ! command -v cargo-bloat &> /dev/null; then
    echo "cargo-bloat not installed. Installing..."
    cargo install cargo-bloat --quiet 2>/dev/null || echo "Could not install cargo-bloat"
fi

BINARY="target/$TARGET/release/$PACKAGE"
if [ -f "$BINARY" ]; then
    echo "Top 50 functions by size:"
    cargo bloat --release --target "$TARGET" --package "$PACKAGE" -n 50 2>/dev/null || echo "Could not run cargo bloat"
    echo ""
    
    echo "Size by crate:"
    cargo bloat --release --target "$TARGET" --package "$PACKAGE" --crates 2>/dev/null || echo "Could not run cargo bloat --crates"
else
    echo "Binary not found at $BINARY - build may have failed"
fi
echo ""

# Method 5: Show actual binary size
echo "=== Method 5: Final binary size ==="
if [ -f "$BINARY" ]; then
    echo "Binary size:"
    ls -lh "$BINARY"
    echo ""
    echo "Section sizes:"
    size "$BINARY" 2>/dev/null || riscv32-unknown-elf-size "$BINARY" 2>/dev/null || echo "size command not available"
    echo ""
    
    # Check if binary is stripped
    if file "$BINARY" | grep -q "not stripped"; then
        echo "⚠️  Binary is NOT stripped - debug info included!"
        echo "   Add 'strip = true' to [profile.release] in Cargo.toml"
    else
        echo "✓ Binary is stripped"
    fi
else
    echo "Binary not found at $BINARY"
fi
echo ""

# Method 6: Strip and show size comparison
echo "=== Method 6: Size after stripping ==="
if [ -f "$BINARY" ]; then
    cp "$BINARY" "${BINARY}.stripped"
    rust-strip "${BINARY}.stripped" 2>/dev/null || \
    riscv32-unknown-elf-strip "${BINARY}.stripped" 2>/dev/null || \
    strip "${BINARY}.stripped" 2>/dev/null || \
    echo "Could not strip (no strip tool found)"
    if [ -f "${BINARY}.stripped" ]; then
        echo "Stripped binary size:"
        ls -lh "${BINARY}.stripped"
        size "${BINARY}.stripped" 2>/dev/null || riscv32-unknown-elf-size "${BINARY}.stripped" 2>/dev/null || echo "size command not available"
        rm "${BINARY}.stripped"
    fi
fi
echo ""

# Method 7: Check release profile settings
echo "=== Method 7: Release profile configuration ==="
if [ -f "apps/$PACKAGE/Cargo.toml" ]; then
    echo "Checking apps/$PACKAGE/Cargo.toml for [profile.release]:"
    if grep -A 10 "\[profile.release\]" "apps/$PACKAGE/Cargo.toml" 2>/dev/null; then
        echo ""
    else
        echo "⚠️  No [profile.release] section found in apps/$PACKAGE/Cargo.toml"
        echo "   Consider adding:"
        echo "   [profile.release]"
        echo "   strip = true"
        echo "   opt-level = \"z\""
        echo "   lto = true"
        echo "   codegen-units = 1"
    fi
else
    echo "Could not find apps/$PACKAGE/Cargo.toml"
fi
echo ""

echo "=========================================="
echo "Analysis complete!"
echo "=========================================="


