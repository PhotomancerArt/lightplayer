//! Run tests concurrently.
//!
//! This module provides the `ConcurrentRunner` struct which uses a pool of threads to run tests
//! concurrently.

use crate::output_mode::OutputMode;
use crate::targets::Target;
use crate::test_run::{PerTargetStats, TestCaseStats};
use anyhow::Result;
use std::collections::BTreeMap;
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, Once};
use std::thread;

static WORKER_PANIC_HOOK: Once = Once::new();

/// Request sent to worker threads.
struct Request {
    jobid: usize,
    path: PathBuf,
    line_filter: Option<usize>,
    output_mode: OutputMode,
    targets: Vec<&'static Target>,
    /// When true, individual test failure messages omit rerun commands (used in
    /// mark-unimplemented mode).
    suppress_rerun: bool,
}

/// Reply from worker thread.
pub enum Reply {
    /// Test execution completed.
    Done {
        /// Job ID matching the request.
        jobid: usize,
        /// Test execution result.
        result: Result<()>,
        /// Per-target stats for summary table.
        per_target: PerTargetStats,
        /// Test case statistics (aggregated).
        stats: TestCaseStats,
        /// Line numbers with unexpected passes per target (e.g. "jit.q32").
        unexpected_pass_by_target: BTreeMap<String, Vec<usize>>,
        /// Line numbers that failed per target.
        failed_lines_by_target: BTreeMap<String, Vec<usize>>,
        /// Whole-file compile failed (summary mode) before executing `// run:` directives.
        compile_failed_by_target: BTreeMap<String, bool>,
        /// True if any target had a whole-file compile failure.
        compile_failed: bool,
        /// False if `run_filetest_with_line_filter` returned `Err` or the worker panicked.
        /// Then `stats` are usually from `count_test_cases` (totals only, no pass/fail).
        harness_completed: bool,
    },
}

/// Manage threads that run test jobs concurrently.
pub struct ConcurrentRunner {
    /// Channel for sending requests to the worker threads.
    /// The workers are sharing the receiver with an `Arc<Mutex<Receiver>>`.
    /// This is `None` when shutting down.
    request_tx: Option<Sender<Request>>,

    /// Channel for receiving replies from the workers.
    /// Workers have their own `Sender`.
    reply_rx: Receiver<Reply>,

    handles: Vec<thread::JoinHandle<()>>,
}

impl ConcurrentRunner {
    /// Create a new `ConcurrentRunner` with threads spun up.
    pub fn new() -> Self {
        let (request_tx, request_rx) = channel();
        let request_mutex = Arc::new(Mutex::new(request_rx));
        let (reply_tx, reply_rx) = channel();

        // Default to num_cpus: WASM and RV32 backends are thread-safe. JIT has issues with
        // multi-file runs (see docs/bugs/2026-03-30-jit-filetest-segfault.md) - skip JIT for bulk
        // operations or use single-threaded mode when JIT testing.
        let num_threads = std::env::var("LP_FILETESTS_THREADS")
            .ok()
            .and_then(|s| {
                use std::str::FromStr;
                usize::from_str(&s).ok().filter(|&n| n > 0)
            })
            .unwrap_or_else(num_cpus::get);

        let handles = (0..num_threads)
            .map(|num| worker_thread(num, request_mutex.clone(), reply_tx.clone()))
            .collect();

        Self {
            request_tx: Some(request_tx),
            reply_rx,
            handles,
        }
    }

    /// Shut down worker threads orderly. They will finish any queued jobs first.
    pub fn shutdown(&mut self) {
        self.request_tx = None;
    }

    /// Join all the worker threads.
    pub fn join(&mut self) {
        assert!(self.request_tx.is_none(), "must shutdown before join");
        for h in self.handles.drain(..) {
            if let Err(e) = h.join() {
                eprintln!("worker thread panicked: {e:?}");
            }
        }
    }

    /// Add a new job to the queue.
    pub fn put(
        &mut self,
        jobid: usize,
        path: &Path,
        line_filter: Option<usize>,
        output_mode: OutputMode,
        targets: &[&'static Target],
        suppress_rerun: bool,
    ) {
        self.request_tx
            .as_ref()
            .expect("cannot push after shutdown")
            .send(Request {
                jobid,
                path: path.to_owned(),
                line_filter,
                output_mode,
                targets: targets.to_vec(),
                suppress_rerun,
            })
            .expect("all the worker threads are gone");
    }

    /// Get a job reply without blocking.
    pub fn try_get(&mut self) -> Option<Reply> {
        self.reply_rx.try_recv().ok()
    }

    /// Get a job reply, blocking until one is available.
    pub fn get(&mut self) -> Option<Reply> {
        self.reply_rx.recv().ok()
    }
}

/// Spawn a worker thread running tests.
fn worker_thread(
    thread_num: usize,
    requests: Arc<Mutex<Receiver<Request>>>,
    replies: Sender<Reply>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name(format!("lps-filetests-app-worker-{thread_num}"))
        .spawn(move || {
            // Install once: replacing the process-global hook per worker was racy and could
            // interact badly with the runtime when many threads start at once.
            WORKER_PANIC_HOOK.call_once(|| {
                std::panic::set_hook(Box::new(|_panic_info| {
                    // Suppress default panic output — workers handle panics via catch_unwind.
                }));
            });

            loop {
                // Lock the mutex only long enough to extract a request.
                let Request {
                    jobid,
                    path,
                    line_filter,
                    output_mode,
                    targets,
                    suppress_rerun,
                } = match requests.lock().unwrap().recv() {
                    Err(..) => break, // TX end shut down. exit thread.
                    Ok(req) => req,
                };

                // Use AssertUnwindSafe to allow catching panics from code that isn't unwind-safe
                let (
                    result,
                    per_target,
                    stats,
                    unexpected_pass_by_target,
                    failed_lines_by_target,
                    compile_failed_by_target,
                    compile_failed,
                    harness_completed,
                ) = match catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::run_filetest_with_line_filter(
                        path.as_path(),
                        line_filter,
                        output_mode,
                        &targets,
                        suppress_rerun,
                    )
                })) {
                    Ok(Ok((r, pt, s, up, fl, cfmap, cf))) => (r, pt, s, up, fl, cfmap, cf, true),
                    Ok(Err(e)) => {
                        if std::env::var("LP_FILETESTS_HARNESS_LOG").is_ok() {
                            eprintln!(
                                "[filetests worker] run_filetest Err for {}: {e:#}",
                                path.display()
                            );
                        }
                        let error_stats = crate::count_test_cases(path.as_path(), line_filter);
                        (
                            Err(e),
                            BTreeMap::new(),
                            error_stats,
                            BTreeMap::new(),
                            BTreeMap::new(),
                            BTreeMap::new(),
                            false,
                            false,
                        )
                    }
                    Err(e) => {
                        // The test panicked, leaving us a `Box<Any>`.
                        // Panics are usually strings or &str.
                        let panic_msg = if let Some(msg) = e.downcast_ref::<String>() {
                            msg.clone()
                        } else if let Some(msg) = e.downcast_ref::<&'static str>() {
                            msg.to_string()
                        } else {
                            // Try to format the panic payload as debug string
                            format!("{e:?}")
                        };

                        // Extract just the essential panic message (first line usually)
                        let short_msg = panic_msg.lines().next().unwrap_or("panic").to_string();

                        // Count test cases even on panic so we can show stats
                        if std::env::var("LP_FILETESTS_HARNESS_LOG").is_ok() {
                            eprintln!(
                                "[filetests worker] panic running {}: {short_msg}",
                                path.display()
                            );
                        }

                        let panic_stats = crate::count_test_cases(path.as_path(), line_filter);

                        (
                            Err(anyhow::anyhow!("panicked: {short_msg}")),
                            BTreeMap::new(),
                            panic_stats,
                            BTreeMap::new(),
                            BTreeMap::new(),
                            BTreeMap::new(),
                            false,
                            false,
                        )
                    }
                };

                replies
                    .send(Reply::Done {
                        jobid,
                        result,
                        per_target,
                        stats,
                        unexpected_pass_by_target,
                        failed_lines_by_target,
                        compile_failed_by_target,
                        compile_failed,
                        harness_completed,
                    })
                    .unwrap();
            }
        })
        .unwrap()
}
