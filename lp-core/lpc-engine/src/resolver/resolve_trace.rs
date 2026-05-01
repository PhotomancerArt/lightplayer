//! Active query stack (correctness) plus optional structured resolver events.

use crate::binding::BindingId;
use crate::resolver::query_key::QueryKey;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;

/// How much optional trace data to retain.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ResolveLogLevel {
    /// No retained events; active stack still runs for cycle detection.
    #[default]
    Off,
    Basic,
    Detail,
}

/// One optional resolver trace record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveTraceEvent {
    BeginQuery(QueryKey),
    CacheHit(QueryKey),
    SelectBinding { query: QueryKey, binding: BindingId },
    ProduceStart(QueryKey),
    ProduceEnd(QueryKey),
    CycleDetected { query: QueryKey },
    ResolveError { query: QueryKey },
}

/// [`ResolveTraceError`] returns from [`ResolveTrace::enter`] on invalid re-entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveTraceError {
    Cycle { query: QueryKey },
}

/// RAII scope for one active query; pops the stack on drop.
pub struct TraceGuard<'a> {
    trace: &'a ResolveTrace,
    query: QueryKey,
}

impl fmt::Debug for TraceGuard<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TraceGuard")
            .field("query", &self.query)
            .finish()
    }
}

impl Drop for TraceGuard<'_> {
    fn drop(&mut self) {
        self.trace.pop_guarded(&self.query);
    }
}

impl<'a> TraceGuard<'a> {
    pub fn active_stack_len(&self) -> usize {
        self.trace.active_stack.borrow().len()
    }

    pub fn is_active(&self, key: &QueryKey) -> bool {
        self.trace.is_active(key)
    }

    pub fn record_event(&self, event: ResolveTraceEvent) {
        self.trace.record_event(event);
    }
}

/// Combined active stack and optional trace log.
pub struct ResolveTrace {
    log_level: ResolveLogLevel,
    active_stack: RefCell<Vec<QueryKey>>,
    events: RefCell<Vec<ResolveTraceEvent>>,
}

impl fmt::Debug for ResolveTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolveTrace")
            .field("log_level", &self.log_level)
            .field("active_stack", &*self.active_stack.borrow())
            .field("events_len", &self.events.borrow().len())
            .finish()
    }
}

impl Clone for ResolveTrace {
    fn clone(&self) -> Self {
        Self {
            log_level: self.log_level,
            active_stack: RefCell::new(self.active_stack.borrow().clone()),
            events: RefCell::new(self.events.borrow().clone()),
        }
    }
}

impl ResolveTrace {
    pub fn new(log_level: ResolveLogLevel) -> Self {
        Self {
            log_level,
            active_stack: RefCell::new(Vec::new()),
            events: RefCell::new(Vec::new()),
        }
    }

    /// Push `query` onto the active stack, or error if it is already active (cycle).
    pub fn try_push_active(&self, query: QueryKey) -> Result<(), ResolveTraceError> {
        {
            let stack = self.active_stack.borrow();
            if stack.contains(&query) {
                drop(stack);
                self.record_event_if_enabled(ResolveTraceEvent::CycleDetected {
                    query: query.clone(),
                });
                return Err(ResolveTraceError::Cycle { query });
            }
        }
        self.active_stack.borrow_mut().push(query.clone());
        self.record_event_if_enabled(ResolveTraceEvent::BeginQuery(query));
        Ok(())
    }

    /// Push `query` if not already active; on success returns a guard that pops on drop.
    pub fn enter<'a>(&'a self, query: QueryKey) -> Result<TraceGuard<'a>, ResolveTraceError> {
        let q = query.clone();
        self.try_push_active(query)?;
        Ok(TraceGuard {
            trace: self,
            query: q,
        })
    }

    /// Pop `query` if it is the top of the active stack.
    pub fn exit(&self, query: &QueryKey) {
        self.pop_guarded(query);
    }

    pub fn is_active(&self, query: &QueryKey) -> bool {
        self.active_stack.borrow().contains(query)
    }

    pub fn active_stack(&self) -> Vec<QueryKey> {
        self.active_stack.borrow().clone()
    }

    pub fn events(&self) -> Vec<ResolveTraceEvent> {
        self.events.borrow().clone()
    }

    pub fn log_level(&self) -> ResolveLogLevel {
        self.log_level
    }

    /// Append an event only when logging is enabled.
    pub fn record_event(&self, event: ResolveTraceEvent) {
        self.record_event_if_enabled(event);
    }

    fn record_event_if_enabled(&self, event: ResolveTraceEvent) {
        if self.log_level == ResolveLogLevel::Off {
            return;
        }
        self.events.borrow_mut().push(event);
    }

    fn pop_guarded(&self, query: &QueryKey) {
        let mut stack = self.active_stack.borrow_mut();
        debug_assert_eq!(
            stack.last(),
            Some(query),
            "ResolveTrace guard exit mismatch",
        );
        if stack.last() == Some(query) {
            stack.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ResolveLogLevel, ResolveTrace, ResolveTraceError, ResolveTraceEvent, TraceGuard};
    use crate::binding::BindingId;
    use crate::resolver::query_key::QueryKey;
    use lpc_model::NodeId;
    use lpc_model::prop::prop_path::parse_path;

    fn sample_key() -> QueryKey {
        QueryKey::NodeInput {
            node: NodeId::new(1),
            input: parse_path("in").unwrap(),
        }
    }

    #[test]
    fn detect_cycle_on_reenter_same_query() {
        let t = ResolveTrace::new(ResolveLogLevel::Off);
        let q = sample_key();
        let _g = t.enter(q.clone()).unwrap();
        let err = t.enter(q.clone()).unwrap_err();
        assert_eq!(err, ResolveTraceError::Cycle { query: q.clone() });
        assert!(t.is_active(&q));
    }

    #[test]
    fn trace_guard_pops_active_stack() {
        let t = ResolveTrace::new(ResolveLogLevel::Off);
        let q = sample_key();
        {
            let g: TraceGuard<'_> = t.enter(q.clone()).unwrap();
            assert_eq!(g.active_stack_len(), 1);
            assert!(g.is_active(&q));
        }
        assert!(t.active_stack().is_empty());
        assert!(!t.is_active(&q));
    }

    #[test]
    fn no_events_when_log_off() {
        let t = ResolveTrace::new(ResolveLogLevel::Off);
        let q = sample_key();
        {
            let _g = t.enter(q.clone()).unwrap();
            drop(_g);
        }
        assert!(t.events().is_empty());
    }

    #[test]
    fn no_events_when_log_off_including_cycle() {
        let t = ResolveTrace::new(ResolveLogLevel::Off);
        let q = sample_key();
        let _g = t.enter(q.clone()).unwrap();
        let _ = t.enter(q.clone());
        assert!(t.events().is_empty());
    }

    #[test]
    fn basic_level_records_useful_events() {
        let t = ResolveTrace::new(ResolveLogLevel::Basic);
        let q = sample_key();
        {
            let g = t.enter(q.clone()).unwrap();
            g.record_event(ResolveTraceEvent::CacheHit(q.clone()));
            g.record_event(ResolveTraceEvent::SelectBinding {
                query: q.clone(),
                binding: BindingId::new(1),
            });
        }
        let ev = t.events();
        assert!(!ev.is_empty());
        assert!(
            ev.iter()
                .any(|e| matches!(e, ResolveTraceEvent::BeginQuery(k) if k == &q))
        );
        assert!(
            ev.iter()
                .any(|e| matches!(e, ResolveTraceEvent::CacheHit(k) if k == &q))
        );
        assert!(ev.iter().any(|e| matches!(
            e,
            ResolveTraceEvent::SelectBinding { query: k, binding } if k == &q && *binding == BindingId::new(1)
        )));
    }

    #[test]
    fn cycle_emits_event_when_logging_on() {
        let t = ResolveTrace::new(ResolveLogLevel::Basic);
        let q = sample_key();
        let _g = t.enter(q.clone()).unwrap();
        let _ = t.enter(q.clone());
        assert!(t.events().iter().any(|e| matches!(
            e,
            ResolveTraceEvent::CycleDetected { query: k } if k == &q
        )));
    }
}
