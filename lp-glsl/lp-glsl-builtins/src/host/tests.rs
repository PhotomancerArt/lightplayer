//! Tests for host functions.

#[cfg(test)]
#[cfg(feature = "test")]
extern crate std;

#[cfg(test)]
#[cfg(feature = "test")]
mod tests {
    use crate::host_debug;

    #[test]
    fn test_host_debug_with_env_var() {
        // Set DEBUG=1
        unsafe {
            std::env::set_var("DEBUG", "1");
        }

        // Test that debug prints when DEBUG=1
        host_debug!("test debug message: {}", 42);
        // If we get here without panicking, it worked
    }

    #[test]
    fn test_host_debug_without_env_var() {
        // Unset DEBUG
        unsafe {
            std::env::remove_var("DEBUG");
        }

        // Test that debug doesn't print when DEBUG is not set
        host_debug!("this should not print: {}", 42);
        // If we get here without panicking, it worked
    }

    #[test]
    fn test_host_functions_format_strings() {
        unsafe {
            std::env::set_var("DEBUG", "1");
        }

        // Test various format specifiers
        host_debug!("hex: {:x}, decimal: {}, binary: {:b}", 255, 255, 255);
    }
}
