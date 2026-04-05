#!/bin/bash

# Get the script's directory and workspace root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Try to find workspace root if script is run from elsewhere
# Look for Cargo.toml and lps directory
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

# Change to workspace root
cd "$WORKSPACE_ROOT" || {
  echo "Error: Failed to change to workspace root: $WORKSPACE_ROOT" >&2
  exit 1
}

# Check if lps directory exists
if [ ! -d "$WORKSPACE_ROOT/lp-shader" ]; then
  echo "Error: lps directory not found at $WORKSPACE_ROOT/lp-shader" >&2
  exit 1
fi

# Parse command line arguments
SHOW_HELP=false
SHOW_LIST=false
REGEN_GEN_FILES=false
TARGET_ARG=()
TEST_ARGS=()

while [[ $# -gt 0 ]]; do
  case $1 in
  --help | -h)
    SHOW_HELP=true
    shift
    ;;
  --list | -l)
    SHOW_LIST=true
    shift
    ;;
  -g)
    REGEN_GEN_FILES=true
    shift
    ;;
  --target)
    TARGET_ARG=("--target" "$2")
    shift 2
    ;;
  --summary)
    TEST_ARGS+=("--concise")
    shift
    ;;
  --fix)
    TEST_ARGS+=("--fix")
    shift
    ;;
  --mark-unimplemented)
    TEST_ARGS+=("--mark-unimplemented")
    shift
    ;;
  --assume-yes)
    TEST_ARGS+=("--assume-yes")
    shift
    ;;
  --debug)
    TEST_ARGS+=("--debug")
    shift
    ;;
  --concise)
    TEST_ARGS+=("--concise")
    shift
    ;;
  --detail)
    TEST_ARGS+=("--detail")
    shift
    ;;
  *)
    TEST_ARGS+=("$1")
    shift
    ;;
  esac
done

# Show help if requested
if [ "$SHOW_HELP" = true ]; then
  cat <<'EOF'
GLSL Filetests Runner

Run GLSL filetests with flexible pattern matching support.

USAGE:
    glsl-filetests.sh [OPTIONS] [PATTERN]...

OPTIONS:
    -h, --help          Show this help message
    -l, --list          List all available test files
    -g                  Regenerate .gen.glsl files before running tests
    --target SPEC       Run target(s): comma-separated, backend shorthand (jit,wasm,rv32), or full names (jit.q32)
    --summary           Same as --concise (alias for the wrapper script)
    --debug             Full output plus CLIF/disassembly on failure (same as DEBUG=1)
    --concise           Minimal output even for a single file
    --detail            Verbose per-// run: output even for many files
    --fix               Remove @unimplemented annotations from tests that now pass
    --mark-unimplemented  Add @unimplemented(backend=…) to failing tests (baseline); use with --target
    --assume-yes        With --mark-unimplemented, skip the interactive confirmation

ENVIRONMENT:
    DEBUG=1             Show debug output (CLIF, WAT) when a test fails
    LP_FIX_XFAIL=1      Same as --fix; remove annotations from newly passing tests
    LP_MARK_UNIMPLEMENTED=1  Same as --mark-unimplemented
    LP_FILETESTS_THREADS=N   Worker threads for concurrent filetests (default: num_cpus).
                        WASM and RV32 are thread-safe. Use N=1 when testing JIT to avoid segfaults.

PATTERNS:
    Patterns can be filenames, glob patterns, or directory paths.
    Filenames are searched recursively across all subdirectories.

EXAMPLES:
    # Run all tests
    glsl-filetests.sh

    # Run specific test file (searched recursively)
    glsl-filetests.sh postinc-scalar-int.glsl

    # Run tests in a directory
    glsl-filetests.sh math/

    # Run tests matching glob patterns
    glsl-filetests.sh "*add*" "operators/*"

    # Run specific test case by line number
    glsl-filetests.sh postinc-scalar-int.glsl:10

    # Run tests in math directory with specific pattern
    glsl-filetests.sh "math/float*"

    # Regenerate .gen.glsl file before running tests
    glsl-filetests.sh vec/vec4/fn-equal.gen.glsl -g

    # Fix unexpected passes: remove @unimplemented from tests that now pass
    glsl-filetests.sh --target wasm.q32 --fix

    # Baseline: mark all current failures @unimplemented(backend=jit), then re-run to get exit 0
    glsl-filetests.sh --target jit.q32 --mark-unimplemented --assume-yes

PATTERN SYNTAX:
    *         Matches any sequence of characters
    ?         Matches any single character
    [abc]     Matches any character in the set
    {a,b,c}   Matches any of the comma-separated patterns

    Patterns without '/' are searched recursively.
    Patterns with '/' are treated as directory paths.

TEST CATEGORIES:
    math/          - Arithmetic operations
    operators/     - Increment/decrement operators
    type_errors/   - Type checking and error handling

EOF
  exit 0
fi

# Show list of tests if requested
if [ "$SHOW_LIST" = true ]; then
  FILETESTS_DIR="$WORKSPACE_ROOT/lp-shader/lps-filetests/filetests"

  # Ensure lps directory exists
  if [ ! -d "$WORKSPACE_ROOT/lp-shader" ]; then
    echo "Error: lps directory not found at $WORKSPACE_ROOT/lp-shader" >&2
    exit 1
  fi

  echo "Available GLSL test files:"
  echo "=========================="

  # Find all .glsl files and group by directory
  find "$FILETESTS_DIR" -name "*.glsl" -type f | sort | while read -r file; do
    # Get relative path from filetests directory
    rel_path="${file#$FILETESTS_DIR/}"

    # Extract directory and filename
    dir=$(dirname "$rel_path")
    filename=$(basename "$rel_path")

    # Print with directory grouping
    if [ "$dir" != "." ]; then
      printf "  %-15s %s\n" "$dir/" "$filename"
    else
      printf "  %-15s %s\n" "" "$filename"
    fi
  done

  echo ""
  echo "Total: $(find "$FILETESTS_DIR" -name "*.glsl" -type f | wc -l | tr -d ' ') test files"
  echo ""
  echo "To run tests:"
  echo "  # Run all tests"
  echo "  ./scripts/glsl-filetests.sh"
  echo ""
  echo "  # Run specific test file (searched recursively)"
  echo "  ./scripts/glsl-filetests.sh filename.glsl"
  echo ""
  echo "  # Run tests in a directory"
  echo "  ./scripts/glsl-filetests.sh directory/"
  echo ""
  echo "  # Run tests matching patterns (supports wildcards)"
  echo "  ./scripts/glsl-filetests.sh \"*pattern*\" \"directory/*\""
  echo ""
  echo "  # Run specific test case by line number"
  echo "  ./scripts/glsl-filetests.sh filename.glsl:10"
  echo ""
  echo "Wildcard patterns:"
  echo "  *         - Matches any sequence of characters"
  echo "  ?         - Matches any single character"
  echo "  [abc]     - Matches any character in the set"
  echo "  {a,b,c}   - Matches any of the comma-separated patterns"
  exit 0
fi

# Ensure lps directory exists before running tests
if [ ! -d "$WORKSPACE_ROOT/lp-shader" ]; then
  echo "Error: lps directory not found at $WORKSPACE_ROOT/lp-shader" >&2
  exit 1
fi

# Build builtins executable before running tests to catch any changes
echo "Building lps-builtins-emu-app..."
"$SCRIPT_DIR/build-builtins.sh" || {
  echo "Error: Failed to build lps-builtins-emu-app" >&2
  exit 1
}

# Change to lps directory where lps-filetests-app workspace is located
cd "$WORKSPACE_ROOT/lp-shader" || {
  echo "Error: Failed to change to lps directory" >&2
  exit 1
}

# Regenerate .gen.glsl files if -g flag is set
if [ "$REGEN_GEN_FILES" = true ]; then
  # Pass all test args to the generator - it will handle expansion
  cargo run -p lps-filetests-gen-app -- "${TEST_ARGS[@]}" --write || {
    echo "Error: Failed to regenerate test files" >&2
    exit 1
  }
fi

# Run the GLSL filetests using lps-filetests-app binary with cargo run
# This ensures cargo run picks up all compilation changes in the lps workspace
# Pass all remaining arguments directly to the test runner
# Pass through DEBUG environment variable for debug logging
cargo run -p lps-filetests-app --bin lps-filetests-app -- test "${TARGET_ARG[@]}" "${TEST_ARGS[@]}"
