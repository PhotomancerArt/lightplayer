//! Run tests concurrently.
//!
//! This module provides the `ConcurrentRunner` struct which uses a pool of threads to run tests
//! concurrently.

use crate::output_mode::OutputMode;
use crate::test_run::TestCaseStats;
use anyhow::Result;
use std::panic::catch_unwind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

/// Request sent to worker threads contains jobid, path, line filter, and output mode.
struct Request {
    jobid: usize,
    path: PathBuf,
    line_filter: Option<usize>,
    output_mode: OutputMode,
}

/// Reply from worker thread.
pub enum Reply {
    /// Test execution completed.
    Done {
        /// Job ID matching the request.
        jobid: usize,
        /// Test execution result.
        result: Result<()>,
        /// Test case statistics.
        stats: TestCaseStats,
        /// Line numbers with unexpected passes (tests marked [expect-fail] that passed).
        unexpected_pass_lines: Vec<usize>,
        /// Line numbers that failed (tests not marked [expect-fail] that failed).
        failed_lines: Vec<usize>,
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

        let num_threads = std::env::var("LP_FILETESTS_THREADS")
            .ok()
            .and_then(|s| {
                use std::str::FromStr;
                usize::from_str(&s).ok().filter(|&n| n > 0)
            })
            .unwrap_or_else(|| num_cpus::get());

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
    ) {
        self.request_tx
            .as_ref()
            .expect("cannot push after shutdown")
            .send(Request {
                jobid,
                path: path.to_owned(),
                line_filter,
                output_mode,
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
        .name(format!("lp-test-worker-{thread_num}"))
        .spawn(move || {
            // Set a custom panic hook for this worker thread that suppresses default output
            // since we catch panics with catch_unwind and convert them to test failures
            std::panic::set_hook(Box::new(|_panic_info| {
                // Suppress default panic output - we handle panics via catch_unwind
            }));

            loop {
                // Lock the mutex only long enough to extract a request.
                let Request {
                    jobid,
                    path,
                    line_filter,
                    output_mode,
                } = match requests.lock().unwrap().recv() {
                    Err(..) => break, // TX end shut down. exit thread.
                    Ok(req) => req,
                };

                // Use AssertUnwindSafe to allow catching panics from code that isn't unwind-safe
                let (result, stats, unexpected_pass_lines, failed_lines) =
                    match catch_unwind(std::panic::AssertUnwindSafe(|| {
                        crate::run_filetest_with_line_filter(
                            path.as_path(),
                            line_filter,
                            output_mode,
                        )
                    })) {
                        Ok(Ok((inner_result, inner_stats, unexpected_lines, failed_lines))) => {
                            (inner_result, inner_stats, unexpected_lines, failed_lines)
                        }
                        Ok(Err(e)) => {
                            // Error occurred, but try to preserve test case count
                            // Count test cases even on error so we can show stats
                            let error_stats = crate::count_test_cases(path.as_path(), line_filter);
                            (Err(e), error_stats, Vec::new(), Vec::new())
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
                            let panic_stats = crate::count_test_cases(path.as_path(), line_filter);

                            (
                                Err(anyhow::anyhow!("panicked: {short_msg}")),
                                panic_stats,
                                Vec::new(),
                                Vec::new(),
                            )
                        }
                    };

                replies
                    .send(Reply::Done {
                        jobid,
                        result,
                        stats,
                        unexpected_pass_lines,
                        failed_lines,
                    })
                    .unwrap();
            }
        })
        .unwrap()
}
