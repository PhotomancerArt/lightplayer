//! Minimal implementations of memcpy, memset, memcmp for no_std environments.
//!
//! These are only used on baremetal targets; on targets with libc, the linker
//! will prefer the libc versions if available.

/// Copy n bytes from src to dest.
///
/// # Safety
/// - `dest` must be valid for writes of `n` bytes
/// - `src` must be valid for reads of `n` bytes
/// - The memory regions must not overlap
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        unsafe {
            *dest.add(i) = *src.add(i);
        }
        i += 1;
    }
    dest
}

/// Fill n bytes at s with the value c.
///
/// # Safety
/// - `s` must be valid for writes of `n` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let c = c as u8;
    let mut i = 0;
    while i < n {
        unsafe {
            *s.add(i) = c;
        }
        i += 1;
    }
    s
}

/// Compare n bytes at s1 and s2.
///
/// Returns:
/// - 0 if the memory regions are equal
/// - < 0 if s1 < s2 (lexicographically)
/// - > 0 if s1 > s2 (lexicographically)
///
/// # Safety
/// - `s1` and `s2` must be valid for reads of `n` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let a = unsafe { *s1.add(i) };
        let b = unsafe { *s2.add(i) };
        if a != b {
            return (a as i32) - (b as i32);
        }
        i += 1;
    }
    0
}
