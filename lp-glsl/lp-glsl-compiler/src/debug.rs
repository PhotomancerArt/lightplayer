//! Debug logging macro for development
//!
//! The `debug!` macro only prints output when:
//! 1. The `std` feature is enabled
//! 2. The `DEBUG=1` environment variable is set
//!
//! Usage:
//! ```rust
//! use lp_glsl_compiler::debug;
//! let value = 42;
//! debug!("message: {:?}", value);
//! ```

#[cfg(feature = "std")]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var("DEBUG").as_deref() == Ok("1") {
            println!("[{}:{}] {}", file!(), line!(), format!($($arg)*));
        }
    };
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {};
}
