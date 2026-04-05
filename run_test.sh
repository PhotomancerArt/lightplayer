#!/bin/bash
cd /Users/yona/dev/photomancer/lp2025
cargo run -p lps-filetests-app -- test function/define-simple.glsl --target rv32.q32 2>&1 | tail -100
