//! Growing the resident arrays by an eighth, not by doubling: `Vec`'s
//! default growth would make one write after mount double the index's
//! allocation (≈ 4.5 KB at c40) — resident RAM the store does not use.

use alloc::vec::Vec;

/// Make room for one more element.
pub fn grow_for_one<T>(v: &mut Vec<T>) {
    if v.len() == v.capacity() {
        v.reserve_exact((v.len() / 8).max(8));
    }
}
