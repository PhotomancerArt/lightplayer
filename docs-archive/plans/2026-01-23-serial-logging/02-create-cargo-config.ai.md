# Phase 2: Create .cargo/config.toml

## Description

Create the `.cargo/config.toml` file to configure the cargo runner for espflash and set up linker flags for RISC-V32 target.

## Changes

Create `.cargo/config.toml` with:
- RISC-V32 target configuration:
  - Runner: `espflash flash --monitor`
  - Linker flags: `-Tlinkall.x` and `-C force-frame-pointers`
- Environment variable: `ESP_LOG = "info"`
- Unstable build-std configuration for `core` and `alloc`

## Success Criteria

- `.cargo/config.toml` file created
- Configuration matches embassy_hello_world example
- RISC-V32 target properly configured

## Code Organization

- Place configuration in logical sections
- Include comments if needed for clarity

## Formatting

- Follow TOML formatting standards
- Match the example format

## Language and Tone

- Use measured, factual descriptions
- Avoid overly optimistic language
