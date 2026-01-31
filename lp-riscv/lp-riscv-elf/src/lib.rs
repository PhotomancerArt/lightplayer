//! RISC-V 32-bit ELF loading and linking utilities.
//!
//! This module provides utilities to load RISC-V ELF files into the emulator's memory.
//! It handles section loading and relocation application.

extern crate alloc;

// Debug macro (this crate requires std)
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var("DEBUG").as_deref() == Ok("1") {
            std::eprintln!("[{}:{}] {}", file!(), line!(), format_args!($($arg)*));
        }
    };
}

mod elf_linker;
mod elf_loader;

pub use elf_linker::{LinkerError, link_static_library};
pub use elf_loader::{ElfLoadInfo, find_symbol_address, load_elf, load_object_file};
