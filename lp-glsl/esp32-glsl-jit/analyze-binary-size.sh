#!/bin/bash
# Script to analyze binary size for esp32-glsl-jit

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.."  # Go to lightplayer root

TARGET="riscv32imac-unknown-none-elf"
PACKAGE="esp32-glsl-jit"
BINARY="target/$TARGET/release/$PACKAGE"

echo "=========================================="
echo "Binary Size Analysis for $PACKAGE"
echo "Target: $TARGET"
echo "=========================================="
echo ""

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo "⚠️  Binary not found at $BINARY"
    echo "Building release binary..."
    cargo build --package "$PACKAGE" --target "$TARGET" --release
fi

# Binary size
echo "=== Binary Size ==="
ls -lh "$BINARY"
echo ""

# Check if cargo-bloat is available
if ! command -v cargo-bloat &> /dev/null; then
    echo "⚠️  cargo-bloat not installed. Install with: cargo install cargo-bloat"
    echo ""
else
    # Size by crate
    echo "=== Size Breakdown by Crate (Top 20) ==="
    cargo bloat --release --target "$TARGET" --package "$PACKAGE" --crates | head -25
    echo ""
    
    # Size by function
    echo "=== Size Breakdown by Function (Top 30) ==="
    cargo bloat --release --target "$TARGET" --package "$PACKAGE" -n 30 | head -40
    echo ""
fi

# Dependency tree
echo "=== Dependency Tree ==="
cargo tree --package "$PACKAGE" --target "$TARGET" | head -50
echo ""

# Features enabled
echo "=== Features Enabled ==="
cargo tree --package "$PACKAGE" --target "$TARGET" -e features | head -50
echo ""

# Check profile settings
echo "=== Release Profile Settings ==="
if grep -A 10 "\[profile.release.package.esp32-glsl-jit\]" ../../Cargo.toml 2>/dev/null; then
    echo "✓ Profile settings found in workspace Cargo.toml"
else
    echo "⚠️  No profile settings found"
fi
echo ""

# Check for .cargo/config.toml
if [ -f ".cargo/config.toml" ]; then
    echo "=== Linker Flags (.cargo/config.toml) ==="
    cat .cargo/config.toml
    echo ""
fi

echo "=========================================="
echo "Analysis complete!"
echo "=========================================="







