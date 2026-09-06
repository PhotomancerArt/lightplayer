#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "check-shader-compile")]
extern crate alloc;

pub mod check;
pub mod checks;
pub mod header;

pub use check::{
    FW_CHECK_JSON_PREFIX, FwCheck, FwCheckConfig, FwCheckTarget, all_checks, find_check,
};
pub use header::{FW_CHECKS_HEADER_PREFIX, HEADER_SCHEMA, PayloadHeader, emit_header, str_is_true};

pub fn emit_record_json(args: core::fmt::Arguments<'_>) {
    log_record(args);
}

fn log_record(args: core::fmt::Arguments<'_>) {
    log::info!("{FW_CHECK_JSON_PREFIX}{args}");
}
