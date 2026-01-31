//! Logging infrastructure for lp-shared
//!
//! This module provides a simple logger implementation that can be used
//! in both std and no_std environments. Firmware (fw-host, device) is
//! responsible for initializing the logger with platform-specific output.
//!
//! Note: The `log` crate will silently discard logs if no logger is set,
//! which is the safe default behavior.

#[cfg(test)]
mod test_logger {
    use log::{LevelFilter, Log, Metadata, Record};

    /// Test logger that prints to stderr
    ///
    /// This logger is used in tests to see log output.
    /// It prints logs in a simple format: `[LEVEL] message`
    pub struct TestLogger;

    impl Log for TestLogger {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            // Use eprintln! for test output (goes to stderr, doesn't interfere with test output)
            #[cfg(feature = "std")]
            #[cfg(feature = "std")]
            {
                std::eprintln!("[{}] {}", record.level(), record.args());
            }
        }

        fn flush(&self) {
            // No-op for tests
        }
    }

    /// Initialize the test logger
    ///
    /// Call this at the start of tests that need logging output.
    /// Sets log level to Debug to see all log messages in tests.
    #[cfg(feature = "std")]
    pub fn init_test_logger() {
        let logger = alloc::boxed::Box::new(TestLogger);
        log::set_logger(alloc::boxed::Box::leak(logger))
            .map(|()| log::set_max_level(LevelFilter::Debug))
            .expect("Failed to set test logger");
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
pub use test_logger::init_test_logger;
