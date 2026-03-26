#!/bin/bash

# LP Build Script
# Runs validation steps and only outputs failures/warnings to help agents fix problems
# Continues on errors to show all failures, then exits with error if any step failed

# Track failures - 0 means no failures, non-zero means at least one failure
FAILED=0

# Get the script's directory (scripts/) and workspace root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ============================================================================
# COMMAND CONFIGURATION
# Add new commands here in the format: "name|relative_directory|command"
# Directory is relative to WORKSPACE_ROOT
# ============================================================================
declare -a COMMANDS=(
    "GLSL filetests|lp-glsl|cargo test -p lp-glsl-filetests --test filetests"
    "ESP32 firmware (compiler in tree)|.|cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server"
)

# ============================================================================
# HELPER FUNCTIONS
# ============================================================================

# Filter output to only show errors, warnings, and failures
# This helps agents focus on what needs to be fixed
filter_output() {
    # Show lines containing errors, failures, panics, warnings
    # Include context lines around matches for better debugging
    grep -E -i --color=never -A 2 -B 2 \
        -e "error" \
        -e "failed" \
        -e "FAILED" \
        -e "panic" \
        -e "thread.*panicked" \
        -e "test.*FAILED" \
        -e "test result: FAILED" \
        -e "failures:" \
        -e "----.*FAIL" \
        -e "could not compile" \
        -e "compilation failed" \
        -e "error\[" \
        -e "warning\[" \
        -e "warning:" \
        -e "^\s*[0-9]+\serror" \
        -e "^\s*[0-9]+\swarning" \
        -e "^\s*[0-9]+\serrors?" \
        -e "^\s*[0-9]+\swarnings?" \
        || true  # Don't fail if grep finds nothing
}

# Run a command and filter output to only show failures/warnings
# Arguments: name, directory (relative to WORKSPACE_ROOT), command
run_command() {
    local name="$1"
    local dir="$2"
    local cmd="$3"
    local full_dir="$WORKSPACE_ROOT/$dir"
    
    echo "======================================"
    echo "Running: $name"
    echo "Directory: $full_dir"
    echo "Command: $cmd"
    echo "======================================"
    
    # Change to directory
    cd "$full_dir" || {
        echo "ERROR: Failed to change to directory: $full_dir"
        FAILED=1
        return 1
    }
    
    # Run command, capturing output and exit code
    # Use a temporary file to capture all output, then filter it
    local temp_output=$(mktemp)
    local exit_code=0
    
    # Run command, redirecting both stdout and stderr to temp file
    # Use eval to properly expand the command string with arguments
    eval "$cmd" > "$temp_output" 2>&1 || exit_code=$?
    
    # Filter and display only errors/warnings/failures
    local filtered=$(filter_output < "$temp_output")
    
    # Always show filtered output if there are any errors/warnings
    if [ -n "$filtered" ]; then
        echo "$filtered"
    fi
    
    # Determine status based on exit code
    if [ $exit_code -ne 0 ]; then
        echo "✗ $name FAILED (exit code: $exit_code)"
        FAILED=1
    elif [ -n "$filtered" ]; then
        echo "⚠ $name completed with warnings (check output above)"
    else
        echo "✓ $name PASSED"
    fi
    
    # Clean up temp file
    rm -f "$temp_output"
    
    return $exit_code
}

# ============================================================================
# MAIN EXECUTION
# ============================================================================

echo "======================================"
echo "LP Build Script"
echo "Workspace: $WORKSPACE_ROOT"
echo "======================================"
echo

# Run each command
for cmd_def in "${COMMANDS[@]}"; do
    # Split by | separator: name|directory|command
    IFS='|' read -r name dir command <<< "$cmd_def"
    run_command "$name" "$dir" "$command"
    echo
done

# Final summary
echo "======================================"
if [ $FAILED -eq 0 ]; then
    echo "✅ LP Build Script: ALL TESTS PASSED"
    echo "======================================"
    exit 0
else
    echo "❌ LP Build Script: SOME TESTS FAILED"
    echo "======================================"
    exit 1
fi
