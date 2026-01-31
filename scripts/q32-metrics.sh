#!/bin/bash
set -e

# Find workspace root (directory containing Cargo.toml)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default paths relative to workspace root
TESTS_DIR="${WORKSPACE_ROOT}/lp-glsl/lp-glsl-q32-metrics-app/glsl"
OUTPUT_DIR="${WORKSPACE_ROOT}/docs/reports/q32"
FORMAT="Fixed16x16"

# Report name is required
if [ $# -eq 0 ]; then
  echo "Error: Report name is required"
  echo "Usage: $0 <report-name> [additional-args...]"
  echo "Example: $0 pre-div-fix"
  exit 1
fi

REPORT_NAME="$1"
shift

# Run the app with defaults
cd "$WORKSPACE_ROOT"
cargo run --bin lp-glsl-q32-xform-metrics-app -- \
  --tests-dir "$TESTS_DIR" \
  --output-dir "$OUTPUT_DIR" \
  --format "$FORMAT" \
  "$REPORT_NAME" \
  "$@"
