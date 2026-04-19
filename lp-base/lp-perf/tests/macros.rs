// Default (no features) — emit calls compile to noop.
#[test]
fn noop_macros_compile() {
    use lp_perf::{emit_begin, emit_end, emit_instant, EVENT_FRAME};
    emit_begin!(EVENT_FRAME);
    emit_end!(EVENT_FRAME);
    emit_instant!(EVENT_FRAME);
}

#[cfg(feature = "log")]
#[test]
fn log_macros_emit_to_log() {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use log::{LevelFilter, Log, Metadata, Record};
    use lp_perf::{emit_begin, emit_end, emit_instant, EVENT_FRAME};
    use std::sync::Once;

    static INIT: Once = Once::new();
    static HITS: AtomicUsize = AtomicUsize::new(0);

    struct HitLogger;

    impl Log for HitLogger {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn log(&self, _record: &Record<'_>) {
            HITS.fetch_add(1, Ordering::Relaxed);
        }

        fn flush(&self) {}
    }

    static LOGGER: HitLogger = HitLogger;

    INIT.call_once(|| {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(LevelFilter::Trace);
    });

    let before = HITS.load(Ordering::Relaxed);
    emit_begin!(EVENT_FRAME);
    emit_end!(EVENT_FRAME);
    emit_instant!(EVENT_FRAME);
    let after = HITS.load(Ordering::Relaxed);

    assert!(
        after >= before + 3,
        "expected at least three trace hooks (begin, end, instant); before={before} after={after}"
    );
}
