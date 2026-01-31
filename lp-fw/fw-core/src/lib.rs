//! Firmware core library.
//!
//! This crate provides the core functionality shared between firmware implementations,
//! including serial I/O and transport abstractions for embedded LightPlayer servers.

#![no_std]

pub mod serial;
pub mod transport;
