//! Xtensa builtins image — the guest-side half of host emulation.
//!
//! Its only product is the **code**: every `__lps_*` builtin, linked at the
//! addresses `lp-xt-emu` models, with a symbol table the host can resolve
//! against. `rt_emu_xt` loads this executable as its base image and places
//! compiled shader functions after it, patching their call relocations
//! against the merged symbol map.
//!
//! Deliberately simpler than the RV32 counterpart
//! (`lp-shader/lps-builtins-emu-app`): that one carries a `__USER_MAIN_PTR`
//! indirection which `lp-riscv-elf::load_object_file` patches to reach a
//! merged object's `_init`. The Xtensa host path does not merge relocatable
//! objects — it resolves shader entry points by symbol and calls them
//! directly (`rt_emu/instance.rs`) — so that indirection has no consumer here
//! and is not reproduced.

#![no_std]
#![no_main]

// The builtin reference list is auto-generated but checked in, and its
// contents are plain `use` statements — entirely ISA-independent. Share the
// RV32 app's copy rather than generating a second one that can drift; the
// generator (`lps-builtins-gen-app`) stays single-output.
#[path = "../../../lp-shader/lps-builtins-emu-app/src/builtin_refs.rs"]
mod builtin_refs;

use lp_xt_emu_guest::emu_main;

/// Entry exists only to anchor the image: it references every builtin so the
/// linker cannot garbage-collect them. The host never calls it — it looks up
/// shader functions by symbol.
fn main(_arg: u32) -> u32 {
    builtin_refs::ensure_builtins_referenced();
    0
}

emu_main!(main);
