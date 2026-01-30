# 32-bit Target Validation Plans

This directory contains plans for implementing comprehensive validation for the RISC-V32 backend in Cranelift.

## Overview

See [00-overview.md](00-overview.md) for the complete overview of the validation project.

## Phase Plans

- **[00-overview.md](00-overview.md)** - Project overview, phases, and approach
- **[01-infrastructure-setup.md](01-infrastructure-setup.md)** - Infrastructure setup and test directory creation
- **[02-function-calls.md](02-function-calls.md)** - Function call and multi-return value tests

### Future Phases (to be created)

- **02-control-flow.md** - Control flow instruction validation
- **03-arithmetic.md** - Arithmetic instruction validation
- **04-bitwise.md** - Bitwise operation validation
- **05-memory.md** - Memory operation validation
- **06-floating-point.md** - Floating point instruction validation
- **07-conversions.md** - Type conversion validation
- **08-simd.md** - SIMD/vector instruction validation
- **09-integer-extensions.md** - Integer extension/reduction validation
- **10-special.md** - Special/miscellaneous instruction validation

## Quick Start

1. Read [00-overview.md](00-overview.md) to understand the project
2. Start with [01-infrastructure-setup.md](01-infrastructure-setup.md) to set up the infrastructure
3. Work through phases 02-10 systematically

## Goals

- Catch unsupported instructions before lowering
- Provide clear error messages
- Document what is and isn't supported
- Create clean 32-bit test suite

## Related Work

- `lightplayer/plans/fix-filetests/` - Related work on fixing filetest failures
- Phase 10 in fix-filetests covers i64 division implementation




