//! Debug macro for conditional logging.

#[cfg(feature = "std")]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var("DEBUG").as_deref() == Ok("1") {
            std::eprintln!("[{}:{}] {}", file!(), line!(), format_args!($($arg)*));
        }
    };
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        // No-op in no_std mode
    };
}
