//! The one place this crate decides what to do when the disk says no.
//!
//! # Fail-fast, on purpose
//!
//! [`MetaStore`](lp_cloud_domain::MetaStore) and
//! [`BlobStore`](lp_cloud_domain::BlobStore) are **infallible ports**: the
//! domain's error vocabulary is what a client is told, and there is nothing
//! useful to tell a client about a database that stopped answering. So the
//! backend-failure policy lives here, in the adapter, and it is to **abort
//! the process with the operation named**.
//!
//! That is the honest policy for this deployment shape, not a shortcut:
//!
//! - The service is one process over one local SQLite file. A failing
//!   statement is a bug in this crate, a corrupt file, or a dead disk —
//!   node-level faults, every one of them.
//! - Recovery is the process supervisor restarting us onto a Litestream
//!   restore, not a `Result` threaded up through the domain to a request
//!   handler that could only answer "500" anyway.
//! - Continuing past a failed write is the one outcome worse than
//!   stopping: it acknowledges a push whose events were never stored.
//!
//! The message always names the operation, because "database is locked" on
//! its own does not tell an operator whether they lost a login or a commit.

use core::fmt::Display;

/// Unwrap a backend result, or die naming `operation`.
///
/// "Die" means the **process**, not the thread. A `panic!` here unwinds
/// into tokio's task machinery, poisons the store mutex, and leaves a
/// zombie: every later request panics on the poison while `/healthz`
/// (which never takes the lock) keeps answering ok — production served
/// exactly that on 2026-08-08 (`docs/defects/
/// 2026-08-08-fatal-store-panic-poisons-not-restarts.md`). The module
/// doc's recovery story is "the supervisor restarts us onto a Litestream
/// restore", and only an abort actually invokes the supervisor.
pub(crate) fn fatal<T, E: Display>(operation: &str, result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            // In this crate's own unit tests, unwind instead: `should_panic`
            // is how the fatal contract is asserted, and an abort would kill
            // the whole test binary. Production (and every other crate's
            // build of this one) aborts.
            #[cfg(test)]
            panic!("lp-cloud-store-sqlite: {operation} failed: {error}");
            #[cfg(not(test))]
            {
                // eprintln! rather than log:: so the message survives even
                // if the logger is not initialized; abort flushes nothing.
                eprintln!(
                    "lp-cloud-store-sqlite: FATAL: {operation} failed: {error} — aborting so the supervisor restarts us"
                );
                std::process::abort();
            }
        }
    }
}
