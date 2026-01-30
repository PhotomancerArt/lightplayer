//! Shared code for RISC-V JIT testing using Cranelift.
//!
//! This crate provides common functionality for building and compiling
//! toy language code to RISC-V that can be used both in the embive VM and on real hardware.

#![no_std]

extern crate alloc;

mod simple_elf;
