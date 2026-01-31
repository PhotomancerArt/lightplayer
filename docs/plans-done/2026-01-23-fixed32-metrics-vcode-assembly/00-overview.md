# Plan: Add VCode and Assembly Generation to Q32 Metrics

## Overview

Extend the `lp-glsl-q32-metrics-app` app to generate vcode and assembly files in addition to CLIF
files, enabling size comparison at multiple compilation stages (CLIF → VCode → Assembly). The app
will switch from `JITModule` to `ObjectModule` to access compiled code, use RISC-V 32-bit target,
and add vcode/assembly size metrics to statistics.

## Phases

1. Update dependencies and module types
2. Switch compiler to ObjectModule and RISC-V target
3. Implement vcode and assembly extraction
4. Update statistics with vcode/assembly sizes
5. Update reports to include new metrics
6. Test and cleanup

## Success Criteria

- App compiles with `emulator` feature enabled
- VCode files (`.pre.vcode`, `.post.vcode`) are generated for all functions
- Assembly files (`.pre.s`, `.post.s`) are generated for all functions
- Statistics include `vcode_size` and `assembly_size` fields
- Report TOML files include vcode and assembly size metrics
- All existing functionality (CLIF generation, statistics) continues to work
- Code is formatted and warnings are fixed
