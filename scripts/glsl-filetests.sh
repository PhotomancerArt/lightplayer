#!/bin/bash

# Get the script's directory and workspace root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Try to find workspace root if script is run from elsewhere
# Look for Cargo.toml and lp-glsl directory
find_workspace_root() {
  local dir="$1"
  while [ "$dir" != "/" ]; do
    if [ -f "$dir/Cargo.toml" ] && [ -d "$dir/lp-glsl" ]; then
      echo "$dir"
      return 0
    fi
    dir="$(dirname "$dir")"
  done
  return 1
}

# If workspace root detection from script location fails, try current directory
if [ ! -f "$WORKSPACE_ROOT/Cargo.toml" ] || [ ! -d "$WORKSPACE_ROOT/lp-glsl" ]; then
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

# Check if lp-glsl directory exists
if [ ! -d "$WORKSPACE_ROOT/lp-glsl" ]; then
  echo "Error: lp-glsl directory not found at $WORKSPACE_ROOT/lp-glsl" >&2
  exit 1
fi

# Parse command line arguments
SHOW_HELP=false
SHOW_LIST=false
REGEN_GEN_FILES=false
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
  FILETESTS_DIR="$WORKSPACE_ROOT/lp-glsl/lp-glsl-filetests/filetests"

  # Ensure lp-glsl directory exists
  if [ ! -d "$WORKSPACE_ROOT/lp-glsl" ]; then
    echo "Error: lp-glsl directory not found at $WORKSPACE_ROOT/lp-glsl" >&2
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

# Ensure lp-glsl directory exists before running tests
if [ ! -d "$WORKSPACE_ROOT/lp-glsl" ]; then
  echo "Error: lp-glsl directory not found at $WORKSPACE_ROOT/lp-glsl" >&2
  exit 1
fi

# Build builtins executable before running tests to catch any changes
echo "Building lp-glsl-builtins-emu-app..."
"$SCRIPT_DIR/build-builtins.sh" || {
  echo "Error: Failed to build lp-glsl-builtins-emu-app" >&2
  exit 1
}

# Change to lp-glsl directory where lp-glsl-filetests-app workspace is located
cd "$WORKSPACE_ROOT/lp-glsl" || {
  echo "Error: Failed to change to lp-glsl directory" >&2
  exit 1
}

# Regenerate .gen.glsl files if -g flag is set
if [ "$REGEN_GEN_FILES" = true ]; then
  # Pass all test args to the generator - it will handle expansion
  cargo run -p lp-glsl-filetests-gen-app -- "${TEST_ARGS[@]}" --write || {
    echo "Error: Failed to regenerate test files" >&2
    exit 1
  }
fi

# Run the GLSL filetests using lp-glsl-filetests-app binary with cargo run
# This ensures cargo run picks up all compilation changes in the lp-glsl workspace
# Pass all remaining arguments directly to the test runner
# Pass through DEBUG environment variable for debug logging
cargo run -p lp-glsl-filetests-app --bin lp-glsl-filetests-app -- test "${TEST_ARGS[@]}"
