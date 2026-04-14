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
    -t, --target        Backend target (rv32fa, rv32lp, rv32, emu)
                        [default: rv32fa]
    --fn <NAME>        Show only specific function (default: all functions)
    --float-mode <MODE> Floating point mode (q32, f32) [default: q32]

EXAMPLES:
    # Show debug output for all functions (rv32fa backend)
    shader-debug.sh file.glsl

    # Show debug output for rv32lp backend
    shader-debug.sh -t rv32lp file.glsl

    # Show only specific function
    shader-debug.sh file.glsl --fn myFunction

    # Full example with all options
    shader-debug.sh -t rv32fa --float-mode q32 file.glsl --fn main

NOTE:
    After running with multiple functions, copy-pasteable commands are
    displayed to show individual functions.

EOF
  exit 0
fi

# Build lp-cli if needed (silent check)
if [ ! -f "$WORKSPACE_ROOT/target/debug/lp-cli" ]; then
  echo "Building lp-cli..."
  cargo build -p lp-cli -q || {
    echo "Error: Failed to build lp-cli" >&2
    exit 1
  }
fi

# Run shader-debug command
# All arguments are passed through to lp-cli
exec "$WORKSPACE_ROOT/target/debug/lp-cli" shader-debug "$@"
