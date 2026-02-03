//! RISC-V 32-bit ELF loading and linking utilities.
//!
//! This crate provides utilities for loading and linking RISC-V ELF files:
//! - Loading ELF files into emulator memory
//! - Applying relocations and resolving symbols
//! - Linking multiple object files into a single executable
//! - Support for both object files and fully linked executables

extern crate alloc;

mod elf_linker;
mod elf_loader;

pub use elf_linker::{LinkerError, link_static_library};
pub use elf_loader::{ElfLoadInfo, find_symbol_address, load_elf, load_object_file};
