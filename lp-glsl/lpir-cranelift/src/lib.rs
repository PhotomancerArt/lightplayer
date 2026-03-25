//! Experimental LPIR → Cranelift JIT (Stage II roadmap).
//!
//! Linear scalar translation and host `JITModule` only; control flow, calls,
//! and memory ops return [`CompileError::Unsupported`].

extern crate alloc;

mod emit;
pub mod error;
mod jit_module;

pub use error::CompileError;
pub use jit_module::jit_from_ir;

#[cfg(test)]
mod tests {
    use core::mem;

    use lpir::parse_module;

    use super::jit_from_ir;

    #[test]
    fn jit_linear_fadd_f32() {
        let ir = parse_module(
            r"func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
}
",
        )
        .expect("parse");

        let (jit, ids) = jit_from_ir(&ir).expect("jit");
        let code_ptr = jit.get_finalized_function(ids[0]);
        let add: extern "C" fn(f32, f32) -> f32 = unsafe { mem::transmute(code_ptr) };
        assert!((add(1.0, 2.0) - 3.0).abs() < 1e-5);
    }
}
