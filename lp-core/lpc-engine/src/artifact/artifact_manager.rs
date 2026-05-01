//! Maps [`SrcArtifactSpec`](lpc_source::SrcArtifactSpec) to refcounted runtime entries.

use alloc::collections::BTreeMap;

use lpc_model::FrameId;
use lpc_source::SrcArtifactSpec;

use super::{ArtifactEntry, ArtifactError, ArtifactRef, ArtifactState};

/// Cache of artifacts keyed by opaque handle and by authored spec string.
///
/// When the refcount of an entry in [`ArtifactState::Resolved`] or an error state reaches zero,
/// the entry is **removed** from both maps. Payload-bearing states transition to [`ArtifactState::Idle`]
/// instead so the spec continues to resolve to the same handle for future acquires.
pub struct ArtifactManager<A> {
    by_handle: BTreeMap<u32, ArtifactEntry<A>>,
    spec_to_handle: BTreeMap<alloc::string::String, u32>,
    next_handle: u32,
}

impl<A> ArtifactManager<A> {
    pub fn new() -> Self {
        Self {
            by_handle: BTreeMap::new(),
            spec_to_handle: BTreeMap::new(),
            next_handle: 1,
        }
    }

    fn alloc_handle(&mut self) -> u32 {
        let h = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        if self.next_handle == 0 {
            self.next_handle = 1;
        }
        h
    }

    /// Acquire (or reuse) an entry for `spec`, increment refcount, and return its handle.
    ///
    /// New entries start as [`ArtifactState::Resolved`] with `content_frame = frame`.
    pub fn acquire_resolved(&mut self, spec: SrcArtifactSpec, frame: FrameId) -> ArtifactRef {
        let key = spec.0.clone();
        if let Some(&handle) = self.spec_to_handle.get(&key) {
            if let Some(entry) = self.by_handle.get_mut(&handle) {
                entry.refcount += 1;
                return ArtifactRef::from_raw(handle);
            }
            self.spec_to_handle.remove(&key);
        }
        let handle = self.alloc_handle();
        self.spec_to_handle.insert(key, handle);
        self.by_handle.insert(
            handle,
            ArtifactEntry {
                spec,
                state: ArtifactState::Resolved,
                refcount: 1,
                content_frame: frame,
                error: None,
            },
        );
        ArtifactRef::from_raw(handle)
    }

    /// Run `loader` for this handle and update state / `content_frame` on success.
    ///
    /// Invokes `loader` regardless of current payload state so callers can reload and bump
    /// [`ArtifactEntry::content_frame`].
    pub fn load_with<F>(
        &mut self,
        r: &ArtifactRef,
        frame: FrameId,
        loader: F,
    ) -> Result<(), ArtifactError>
    where
        F: FnOnce(&SrcArtifactSpec) -> Result<A, ArtifactError>,
    {
        let handle = r.handle();
        let entry = self
            .by_handle
            .get_mut(&handle)
            .ok_or(ArtifactError::UnknownHandle { handle })?;
        let spec = entry.spec.clone();
        match loader(&spec) {
            Ok(a) => {
                entry.state = ArtifactState::Loaded(a);
                entry.content_frame = frame;
                Ok(())
            }
            Err(e) => {
                entry.state = match &e {
                    ArtifactError::Prepare(s) => ArtifactState::PrepareError(s.clone()),
                    ArtifactError::Resolution(s) => ArtifactState::ResolutionError(s.clone()),
                    ArtifactError::Load(s) => ArtifactState::LoadError(s.clone()),
                    ArtifactError::UnknownHandle { .. } | ArtifactError::InvalidRelease { .. } => {
                        ArtifactState::LoadError(e.summary_for_state())
                    }
                };
                Err(e)
            }
        }
    }

    /// Decrement refcount. Payload-bearing entries become [`ArtifactState::Idle`] at zero refs;
    /// resolved-only and error entries are removed (see struct docs).
    pub fn release(&mut self, r: &ArtifactRef, _frame: FrameId) -> Result<(), ArtifactError> {
        let handle = r.handle();
        let entry = self
            .by_handle
            .get_mut(&handle)
            .ok_or(ArtifactError::UnknownHandle { handle })?;
        if entry.refcount == 0 {
            return Err(ArtifactError::InvalidRelease { handle });
        }
        entry.refcount -= 1;
        if entry.refcount != 0 {
            return Ok(());
        }
        let state = core::mem::replace(&mut entry.state, ArtifactState::Resolved);
        match state {
            ArtifactState::Resolved
            | ArtifactState::ResolutionError(_)
            | ArtifactState::LoadError(_)
            | ArtifactState::PrepareError(_) => {
                let key = entry.spec.0.clone();
                self.spec_to_handle.remove(&key);
                self.by_handle.remove(&handle);
            }
            ArtifactState::Loaded(a) | ArtifactState::Prepared(a) => {
                entry.state = ArtifactState::Idle(a);
            }
            ArtifactState::Idle(a) => {
                entry.state = ArtifactState::Idle(a);
            }
        }
        Ok(())
    }

    pub fn entry(&self, r: &ArtifactRef) -> Option<&ArtifactEntry<A>> {
        self.by_handle.get(&r.handle())
    }

    pub fn content_frame(&self, r: &ArtifactRef) -> Option<FrameId> {
        self.entry(r).map(|e| e.content_frame)
    }

    pub fn refcount(&self, r: &ArtifactRef) -> Option<u32> {
        self.entry(r).map(|e| e.refcount)
    }
}

impl<A> Default for ArtifactManager<A> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    fn spec(path: &str) -> SrcArtifactSpec {
        SrcArtifactSpec(String::from(path))
    }

    #[test]
    fn acquire_same_spec_reuses_handle_and_increments_refcount() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let s = spec("a.lp");
        let r1 = m.acquire_resolved(s.clone(), FrameId::new(1));
        let r2 = m.acquire_resolved(s, FrameId::new(2));
        assert_eq!(r1.handle(), r2.handle());
        assert_eq!(m.refcount(&r1), Some(2));
    }

    #[test]
    fn release_decrements_refcount() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let r = m.acquire_resolved(spec("b.lp"), FrameId::new(1));
        let h = r.handle();
        let r2 = m.acquire_resolved(spec("b.lp"), FrameId::new(1));
        assert_eq!(m.refcount(&r), Some(2));
        m.release(&r2, FrameId::new(1)).unwrap();
        assert_eq!(m.refcount(&r), Some(1));
        assert_eq!(m.entry(&r).unwrap().spec.0, "b.lp");
        assert_eq!(m.entry(&ArtifactRef::from_raw(h)).unwrap().refcount, 1);
    }

    #[test]
    fn loaded_moves_to_idle_when_refcount_reaches_zero() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let r = m.acquire_resolved(spec("c.lp"), FrameId::new(1));
        m.load_with(&r, FrameId::new(5), |_s| Ok(42)).unwrap();
        assert!(matches!(
            m.entry(&r).unwrap().state,
            ArtifactState::Loaded(42)
        ));
        m.release(&r, FrameId::new(1)).unwrap();
        let e = m.entry(&r).unwrap();
        assert_eq!(e.refcount, 0);
        assert!(matches!(&e.state, ArtifactState::Idle(42)));
    }

    #[test]
    fn load_success_bumps_content_frame() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let r = m.acquire_resolved(spec("d.lp"), FrameId::new(1));
        m.load_with(&r, FrameId::new(10), |_s| Ok(1)).unwrap();
        assert_eq!(m.content_frame(&r), Some(FrameId::new(10)));
        m.load_with(&r, FrameId::new(99), |_s| Ok(2)).unwrap();
        assert_eq!(m.content_frame(&r), Some(FrameId::new(99)));
        if let ArtifactState::Loaded(v) = &m.entry(&r).unwrap().state {
            assert_eq!(*v, 2);
        } else {
            panic!("expected Loaded");
        }
    }

    #[test]
    fn load_failure_records_load_error() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let r = m.acquire_resolved(spec("e.lp"), FrameId::new(1));
        let err = m
            .load_with(&r, FrameId::new(3), |_s| {
                Err(ArtifactError::Load(String::from("boom")))
            })
            .unwrap_err();
        assert_eq!(err, ArtifactError::Load(String::from("boom")));
        let e = m.entry(&r).unwrap();
        assert!(matches!(
            &e.state,
            ArtifactState::LoadError(msg) if msg == "boom"
        ));
    }

    #[test]
    fn unknown_handle_returns_structured_error() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let bad = ArtifactRef::from_raw(999);
        assert_eq!(
            m.release(&bad, FrameId::default()).unwrap_err(),
            ArtifactError::UnknownHandle { handle: 999 }
        );
        assert_eq!(
            m.load_with(&bad, FrameId::default(), |_s| Ok(0))
                .unwrap_err(),
            ArtifactError::UnknownHandle { handle: 999 }
        );
        assert!(m.entry(&bad).is_none());
    }
}
