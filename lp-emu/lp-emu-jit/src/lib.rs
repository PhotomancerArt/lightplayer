//! Translate RV32IMC guest code to WebAssembly, so the emulator's hot path is
//! the host engine's compiled code rather than an interpreter loop.
//!
//! # What this crate is
//!
//! A **translator**, and the pieces that make a translator checkable:
//!
//! - [`decode`] — RV32IMC word → a small [`decode::Inst`] with the emulator's
//!   own [`lp_emu_core::InstClass`] cost class and the instruction's width
//!   attached. It is a second decoder, and it exists because the translator
//!   must decode without running: `lp-riscv-emu`'s decode is fused into its
//!   executors and cannot be asked "what is this?" without also being told
//!   "and do it". `tests/decoder_agreement.rs` is what makes a second decoder
//!   acceptable rather than a divergence waiting to happen (M7 JD3).
//! - [`blocks`] — the guest blocks a translation is *of*, and the simple walk
//!   P3 uses to find some. Discovery proper is P4's.
//! - [`translate`] — the block set to one wasm function: registers in locals,
//!   the arena as the imported memory, RAM behind a permission byte, MMIO and
//!   the escape hatch through imports.
//! - [`host`] — the contract an emitted module is run against, written out
//!   once so a reader does not have to reconstruct the exit protocol from the
//!   emitter.
//! - [`replay`] — the record/compare shapes of the identity harness: an
//!   interpreter run records what each entry into translated code did, and the
//!   translated module is replayed against that recording, in every engine it
//!   will ever run in.
//!
//! # What this crate is not
//!
//! Not an emulator, not a host, and not a policy. It owns no bus, no machine
//! and no cycle counter; it does not decide *when* to translate, *what* to
//! translate, or *where* the resulting module runs. Those live in the machine
//! crates, which install a translated core through `lp-riscv-emu`'s seam. It
//! also never guesses: an encoding [`decode::decode`] does not recognise ends a
//! block and is handed back to the interpreter, so a mis-swept block, an
//! unsupported extension and data-in-text all degrade to interpretation rather
//! than to a wrong answer (JD7).
//!
//! # Licence posture (JD2)
//!
//! Everything under `lp-emu/` is **MIT** as a unit while the rest of the
//! repository is AGPL-3.0-or-later, and `just lint-emu-fence` is what keeps
//! that real. This crate's only default dependency outside the fence is
//! [`wasm_encoder`](https://docs.rs/wasm-encoder) (Apache-2.0 WITH
//! LLVM-exception) — a byte emitter, not a compiler. `wasmtime` is optional
//! (`host-wasmtime`) and never a default. No workspace-local AGPL edge is
//! added: in particular the translator does not use `lp-riscv-inst`, even
//! though the fence allowlist would permit it.

#![no_std]

extern crate alloc;

pub mod blocks;
pub mod decode;
pub mod host;
#[cfg(feature = "host-wasmtime")]
pub mod host_wasmtime;
pub mod replay;
pub mod translate;
