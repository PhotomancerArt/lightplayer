#!/bin/bash
# Trace all Rust source files that get included when compiling embive-program
# This helps identify unnecessary dependencies for size optimization

set -e

# Change to repository root (where Cargo.toml is)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"
cd "$REPO_ROOT"

TARGET="riscv32imac-unknown-none-elf"
PACKAGE="embive-program"

echo "========================================================"
echo "Tracing compiled files for $PACKAGE (target: $TARGET)"
echo "========================================================"
echo ""

# Create output directory for analysis (in script directory)
OUTPUT_DIR="$SCRIPT_DIR/analysis-output"
mkdir -p "$OUTPUT_DIR"

# Method 1: Show dependency tree (what crates are pulled in)
echo "=== 1. Dependency Tree (what crates are included) ==="
cargo tree --package "$PACKAGE" --target "$TARGET" -e normal --prefix none | tee "$OUTPUT_DIR/dep-tree.txt"
echo ""

# Method 2: Show dependency tree with features
echo "=== 2. Dependency Tree with Features ==="
cargo tree --package "$PACKAGE" --target "$TARGET" -e features --prefix none | head -50
echo "(Full output in $OUTPUT_DIR/dep-tree-features.txt)"
cargo tree --package "$PACKAGE" --target "$TARGET" -e features --prefix none > "$OUTPUT_DIR/dep-tree-features.txt"
echo ""

# Method 3: Build with verbose output and extract file list
echo "=== 3. Extracting compiled source files ==="
echo "Rebuilding with verbose output to trace source files..."

# Clean just this package to force recompile
cargo clean -p "$PACKAGE" -q

# Build with very verbose output and capture
cargo rustc --package "$PACKAGE" --target "$TARGET" --release -- --emit=dep-info -v 2>&1 | tee "$OUTPUT_DIR/build-verbose.log" | grep "Compiling" || true

# Also try with cargo build -v to see all rustc invocations
echo ""
echo "Analyzing .d files to find all source files..."
find target/$TARGET/release -name "*.d" -newer "$OUTPUT_DIR" 2>/dev/null | while read dfile; do
    echo "  From: $(basename $dfile)"
    cat "$dfile" | tr ' ' '\n' | grep "\.rs$" | head -5
done | tee "$OUTPUT_DIR/source-files-from-d.txt" | head -20
echo "(More in $OUTPUT_DIR/source-files-from-d.txt)"
echo ""

# Method 4: Analyze the binary itself
echo "=== 4. Binary Size Analysis ==="
BINARY="target/$TARGET/release/$PACKAGE"

if [ -f "$BINARY" ]; then
    echo "Binary location: $BINARY"
    ls -lh "$BINARY"
    echo ""
    
    # Show section sizes
    echo "Section sizes:"
    size "$BINARY" | tee "$OUTPUT_DIR/binary-size.txt"
    echo ""
    
    # If nm is available, show symbol counts
    if command -v nm &> /dev/null; then
        echo "Symbol analysis (top crate prefixes):"
        nm "$BINARY" 2>/dev/null | \
            grep -E " [TtRrDdBb] " | \
            sed 's/.* [TtRrDdBb] //' | \
            sed 's/_.*$//' | \
            sort | uniq -c | sort -rn | head -20 | tee "$OUTPUT_DIR/symbol-prefixes.txt"
        echo ""
    fi
fi

# Method 5: Use cargo-bloat if available
echo "=== 5. Size breakdown by function (cargo bloat) ==="
if command -v cargo-bloat &> /dev/null; then
    cargo bloat --release --target "$TARGET" --package "$PACKAGE" -n 30 | tee "$OUTPUT_DIR/bloat-report.txt"
    echo ""
    
    echo "Crate-level breakdown:"
    cargo bloat --release --target "$TARGET" --package "$PACKAGE" --crates | tee "$OUTPUT_DIR/bloat-crates.txt"
else
    echo "cargo-bloat not installed."
    echo "Install with: cargo install cargo-bloat"
    echo ""
fi

# Method 6: Show what features are actually enabled
echo "=== 6. Features enabled ==="
cargo tree --package "$PACKAGE" --target "$TARGET" -e features -i "$PACKAGE" | head -20
echo ""

# Summary
echo "========================================================"
echo "Analysis complete!"
echo ""
echo "Output files created in $OUTPUT_DIR/:"
ls -lh "$OUTPUT_DIR/"
echo ""
echo "Key files to review:"
echo "  - dep-tree.txt: Full dependency tree"
echo "  - bloat-report.txt: Functions taking most space"
echo "  - bloat-crates.txt: Crates taking most space"
echo "========================================================"




