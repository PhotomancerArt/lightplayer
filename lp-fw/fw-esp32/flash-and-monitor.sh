#!/bin/bash
# Don't use set -e here - we want to continue even if probe-rs has disconnect errors

# Get the binary path (first argument from cargo run)
BINARY="$1"

if [ -z "$BINARY" ]; then
    echo "Error: No binary path provided"
    exit 1
fi

# Give the device a moment to stabilize after reset
sleep 1

# Flash using probe-rs (handles large binaries)
# Use 'download' instead of 'run' - it flashes and exits immediately without running/resetting
# --connect-under-reset helps with ESP32-C6 USB-JTAG connection issues
echo "Flashing with probe-rs..."
probe-rs download --chip esp32c6 --connect-under-reset "$BINARY" || {
    echo "probe-rs exited (disconnect errors are normal after flashing)"
}

# Reset and monitor using espflash in one command
# The --after flag ensures the chip is reset before monitoring starts
echo "Resetting chip and starting serial monitor (press Ctrl+C to exit)..."
espflash monitor --chip esp32c6 --after hard-reset
