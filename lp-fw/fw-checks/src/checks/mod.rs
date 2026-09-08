// `test` as well as the feature: the module is `no_std` and `alloc`-free, so
// the feature gate is only about what a device image links. Without `test`
// here, a plain `cargo test -p fw-checks` would compile the module out and
// silently run none of its tests — the feature is only on in this workspace
// because `lp-cli` happens to enable it.
#[cfg(any(feature = "check-gpio-calibrate", test))]
pub mod gpio_calibrate;
#[cfg(feature = "check-jit-math-perf")]
pub mod jit_math_perf;
#[cfg(any(feature = "check-render-loop", test))]
pub mod render_loop;
#[cfg(any(feature = "check-rmt", test))]
pub mod rmt_chase;
#[cfg(feature = "check-shader-compile")]
pub mod shader_compile;
#[cfg(any(feature = "check-uart-bridge", test))]
pub mod uart_bridge;
