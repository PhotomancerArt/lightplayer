# FW-Host Runtime Integration Plan

## Overview

This plan covers integrating the `lp-core` runtime with `fw-host` to create a fully functional demo showing animated LED output. This involves creating a high-level `LpApp` in `lp-core` that manages project lifecycle and message handling, then hooking it up to `fw-host` with an update loop and animated demo scene.

## Scope

- Create `LpApp` in `lp-core` to manage project lifecycle
- Create `Platform` struct to wrap platform-specific traits
- Create `MsgIn`/`MsgOut` enums for message handling
- Implement `HostOutputProvider` for fw-host
- Integrate `LpApp` into `fw-host` with update loop
- Create animated demo scene (rotating color wheel)

## Current State

- `lp-core` runtime is complete (`ProjectRuntime`, node runtimes)
- `fw-host` exists but runtime is not integrated
- Default project exists but has static shader (no animation)
- LED visualization exists but shows static data

## Goals

1. Create `LpApp` as main entry point for firmware
2. Hook up runtime initialization and update loop
3. Create animated demo scene showing LED output changing
4. Ensure LEDs actually update and are visible
5. Set foundation for future client editing work

