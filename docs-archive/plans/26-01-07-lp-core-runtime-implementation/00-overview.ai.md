# LP-Core Runtime Implementation Plan

## Overview

This plan covers implementing the runtime architecture designed in `26-01-06-lp-core-runtime-architecture/00-design.md`.

## Scope

Implement the complete runtime system for `lp-core`:
- Type-safe node IDs
- Texture utility abstraction
- FrameTime tracking
- Node lifecycle trait and contexts
- Node runtime implementations (Texture, Shader, Fixture, Output)
- ProjectRuntime with initialization and update loop
- ProjectBuilder for easy test setup

## Current State

- Design document is complete with all issues resolved
- Basic project config structures exist (`ProjectConfig`, node types)
- Abstraction traits exist (`Filesystem`, `Transport`, `LedOutput`)
- Error types exist
- No runtime implementation yet

## Goals

1. Implement all runtime types according to design
2. Implement ProjectRuntime with lifecycle management
3. Create ProjectBuilder for easy project setup
4. Ensure all code compiles and follows existing style
5. Add basic tests for runtime functionality

