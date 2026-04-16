#!/bin/bash

# shader-debug.sh - Wrapper for lp-cli shader-debug
#
# Provides convenient access to the unified shader debug command from anywhere
# in the workspace.

# Get the script's directory and workspace root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Try to find workspace root if script is run from elsewhere
find_workspace_root() {
  local dir="$1"
  while [ "$dir" != "/" ]; do
    if [ -f "$dir/Cargo.toml" ] && [ -d "$dir/lp-shader" ]; then
      echo "$dir"
      return 0
    fi
    dir="$(dirname "$dir")"
  done
  return 1
}

# If workspace root detection from script location fails, try current directory
if [ ! -f "$WORKSPACE_ROOT/Cargo.toml" ] || [ ! -d "$WORKSPACE_ROOT/lp-shader" ]; then
  WORKSPACE_ROOT="$(find_workspace_root "$(pwd)")" || {
    echo "Error: Could not find workspace root. Please run from the workspace root directory." >&2
    exit 1
  }
fi

# Show help
if [[ "$1" == "--help" || "$1" == "-h" ]]; then
  cat <<'EOF'
Shader Debug - Unified debug output for shader compilation

USAGE:
    shader-debug.sh [OPTIONS] <FILE.glsl>

OPTIONS:
    -h, --help          Show this help message
    -t, --target        Comma-separated list of backend targets
                        (rv32n, rv32c, emu) [default: rv32n]
    --fn <NAME>         Show only specific function (default: all functions)
    --float-mode <MODE> Floating point mode (q32, f32) [default: q32]
    --lpir              Show LPIR section
    --vinst            Show VInst/interleaved section
    --asm              Show assembly/disasm section
    --summary          Summary only (no detailed function output)

EXAMPLES:
    # Show debug output for all functions (rv32n backend)
    shader-debug.sh file.glsl

    # Compare multiple backends with comparison table
    shader-debug.sh -t rv32c,rv32n file.glsl

    # Show only specific function
    shader-debug.sh file.glsl --fn myFunction

    # Show only assembly (no interleaved LPIR)
    shader-debug.sh -t rv32n file.glsl --asm

    # Full example with all options
    shader-debug.sh -t rv32c,rv32n --float-mode q32 file.glsl --fn main

NOTE:
    When multiple targets are specified, a comparison table is shown
    with instruction counts and performance ratios (color-coded).

EOF
  exit 0
fi

# Change to workspace root for cargo
cd "$WORKSPACE_ROOT" || {
  echo "Error: Failed to change to workspace root: $WORKSPACE_ROOT" >&2
  exit 1
}

# Run shader-debug command using cargo run to ensure automatic rebuilds
# All arguments are passed through to lp-cli
exec cargo run -p lp-cli -q -- shader-debug "$@"
