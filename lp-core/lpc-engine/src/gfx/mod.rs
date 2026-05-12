//! Graphics abstraction (`LpGraphics` / `LpShader`): boundary between the engine and shader backends.
//!
//! Backend selection is target-driven: exactly one `Graphics` impl is compiled
//! per target. There is no Cargo feature for picking a backend.
//!
//! | Target                                  | Module                | Backend                            |
//! |-----------------------------------------|-----------------------|------------------------------------|
//! | `cfg(target_arch = "riscv32")`          | [`native_jit`]        | `lpvm-native::rt_jit`              |
//! | `cfg(target_arch = "wasm32")`           | [`wasm_guest`]        | `lpvm-wasm::rt_browser`            |
//! | catchall (host)                         | [`host`]              | `lpvm-wasm::rt_wasmtime`           |

pub mod compute_desc;
pub mod lp_gfx;
pub mod lp_shader;
pub(crate) mod uniforms;

pub mod convert_type;
pub mod convert_value;

#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
pub mod host;
#[cfg(target_arch = "riscv32")]
pub mod native_jit;
#[cfg(target_arch = "wasm32")]
pub mod wasm_guest;

pub use compute_desc::{ComputeDescError, compute_desc_from_model_def};
pub use convert_type::model_type_to_lps_type;
pub use convert_value::{LpsValueToModelConversionError, lps_value_f32_to_model_value};
pub use lp_gfx::LpGraphics;
pub use lp_shader::{LpShader, ShaderCompileOptions};

#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
pub use host::Graphics;
#[cfg(target_arch = "riscv32")]
pub use native_jit::Graphics;
#[cfg(target_arch = "wasm32")]
pub use wasm_guest::Graphics;
