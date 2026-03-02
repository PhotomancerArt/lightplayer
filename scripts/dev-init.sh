#!/bin/bash
# Development initialization script
# Ensures required tools are installed and git hooks are set up

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Script directory (where this script is located)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Project root (parent of scripts directory)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Initializing development environment..."
echo "Project root: $PROJECT_ROOT"
echo ""

# Function to check if a command exists
check_command() {
    if command -v "$1" > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} $1 is installed"
        return 0
    else
        echo -e "${RED}✗${NC} $1 is not installed"
        return 1
    fi
}

# Function to check Rust version
check_rust_version() {
    local min_version="1.90.0"
    if command -v rustc > /dev/null 2>&1; then
        # Extract version number from "rustc 1.91.1 (ed61e7d7e 2025-11-07)" format
        local rust_version=$(rustc --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
        
        if [ -z "$rust_version" ]; then
            echo -e "${RED}✗${NC} Could not parse rustc version"
            return 1
        fi
        
        echo -e "${GREEN}✓${NC} rustc version: $rust_version"
        
        # Compare versions (simple numeric comparison)
        local rust_major=$(echo "$rust_version" | cut -d. -f1)
        local rust_minor=$(echo "$rust_version" | cut -d. -f2)
        local min_major=$(echo "$min_version" | cut -d. -f1)
        local min_minor=$(echo "$min_version" | cut -d. -f2)
        
        # Validate that we got numeric values
        if [ -z "$rust_major" ] || [ -z "$rust_minor" ] || [ -z "$min_major" ] || [ -z "$min_minor" ]; then
            echo -e "${RED}✗${NC} Could not parse version numbers for comparison"
            return 1
        fi
        
        if [ "$rust_major" -lt "$min_major" ] || ([ "$rust_major" -eq "$min_major" ] && [ "$rust_minor" -lt "$min_minor" ]); then
            echo -e "${RED}✗${NC} Rust version $rust_version is below minimum required version $min_version"
            echo "  Install or update Rust: https://rustup.rs/"
            return 1
        fi
        return 0
    else
        echo -e "${RED}✗${NC} rustc is not installed"
        echo "  Install Rust: https://rustup.rs/"
        return 1
    fi
}

# Check required tools
echo "Checking required tools..."
MISSING_TOOLS=0

# Note: We check for rustc instead of rust (rustc is the actual compiler)
if ! check_command "rustc"; then
    echo "  Install Rust: https://rustup.rs/"
    MISSING_TOOLS=1
fi

if ! check_command "cargo"; then
    echo "  Install Rust (includes cargo): https://rustup.rs/"
    MISSING_TOOLS=1
fi

if ! check_command "rustup"; then
    echo "  Install Rust (includes rustup): https://rustup.rs/"
    MISSING_TOOLS=1
fi

if ! check_command "just"; then
    echo "  Install just: cargo install just"
    echo "  Or via package manager: https://github.com/casey/just#installation"
    MISSING_TOOLS=1
fi

# Check Rust version
if ! check_rust_version; then
    MISSING_TOOLS=1
fi

if [ $MISSING_TOOLS -eq 1 ]; then
    echo ""
    echo -e "${RED}Error: Missing required tools. Please install them and run this script again.${NC}"
    exit 1
fi

echo ""

# Update tools to latest (keeps machines in sync with CI)
echo "Updating tools to latest versions..."
if rustup self update 2>/dev/null; then
    echo -e "${GREEN}✓${NC} rustup updated"
else
    echo -e "${YELLOW}⚠${NC} rustup self-update skipped (may require network)"
fi
rustup update stable
echo -e "${GREEN}✓${NC} Rust toolchain updated to latest stable"
if cargo install just; then
    echo -e "${GREEN}✓${NC} just updated"
else
    echo -e "${YELLOW}⚠${NC} just update skipped (try: cargo install just)"
fi
echo ""

# Install RISC-V target
RISC_V_TARGET="riscv32imac-unknown-none-elf"
echo "Checking RISC-V target ($RISC_V_TARGET)..."
if rustup target list --installed | grep -q "^${RISC_V_TARGET}$"; then
    echo -e "${GREEN}✓${NC} RISC-V target already installed"
else
    echo "Installing RISC-V target..."
    rustup target add "$RISC_V_TARGET"
    echo -e "${GREEN}✓${NC} RISC-V target installed"
fi

echo ""
echo -e "${GREEN}Development environment initialized successfully!${NC}"
