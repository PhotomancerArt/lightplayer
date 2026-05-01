//! Maps [`ArtifactLocation`](super::ArtifactLocation) to refcounted runtime entries.

use alloc::collections::BTreeMap;

use lpc_model::FrameId;

use super::{ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactState};

/// Cache of artifacts keyed by opaque handle and resolved location.
///
/// When the refcount of an entry in [`ArtifactState::Resolved`] or an error state reaches zero,
/// the entry is **removed** from both maps. Payload-bearing states transition to [`ArtifactState::Idle`]
/// instead so the location continues to resolve to the same handle for future acquires.
pub struct ArtifactManager<A> {
    by_handle: BTreeMap<u32, ArtifactEntry<A>>,
    location_to_handle: BTreeMap<ArtifactLocation, u32>,
    next_handle: u32,
}

impl<A> ArtifactManager<A> {
    pub fn new() -> Self {
        Self {
            by_handle: BTreeMap::new(),
            location_to_handle: BTreeMap::new(),
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

    /// Acquire (or reuse) an entry for `location`, increment refcount, and return its handle.
    ///
    /// New entries start as [`ArtifactState::Resolved`] with `content_frame = frame`.
    pub fn acquire_location(&mut self, location: ArtifactLocation, frame: FrameId) -> ArtifactId {
        if let Some(&handle) = self.location_to_handle.get(&location) {
            if let Some(entry) = self.by_handle.get_mut(&handle) {
                entry.refcount += 1;
                return ArtifactId::from_raw(handle);
            }
            self.location_to_handle.remove(&location);
        }
        let handle = self.alloc_handle();
        self.location_to_handle.insert(location.clone(), handle);
        let id = ArtifactId::from_raw(handle);
        self.by_handle.insert(
            handle,
            ArtifactEntry {
                id,
                location,
                state: ArtifactState::Resolved,
                refcount: 1,
                content_frame: frame,
                error: None,
            },
        );
        id
    }

    /// Run `loader` for this handle and update state / `content_frame` on success.
    ///
    /// Invokes `loader` regardless of current payload state so callers can reload and bump
    /// [`ArtifactEntry::content_frame`].
    pub fn load_with<F>(
        &mut self,
        r: &ArtifactId,
        frame: FrameId,
        loader: F,
    ) -> Result<(), ArtifactError>
    where
        F: FnOnce(&ArtifactLocation) -> Result<A, ArtifactError>,
    {
        let handle = r.handle();
        let entry = self
            .by_handle
            .get_mut(&handle)
            .ok_or(ArtifactError::UnknownHandle { handle })?;
        let location = entry.location.clone();
        match loader(&location) {
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
    pub fn release(&mut self, r: &ArtifactId, _frame: FrameId) -> Result<(), ArtifactError> {
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
                let location = entry.location.clone();
                self.location_to_handle.remove(&location);
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

    pub fn entry(&self, r: &ArtifactId) -> Option<&ArtifactEntry<A>> {
        self.by_handle.get(&r.handle())
    }

    pub fn content_frame(&self, r: &ArtifactId) -> Option<FrameId> {
        self.entry(r).map(|e| e.content_frame)
    }

    pub fn refcount(&self, r: &ArtifactId) -> Option<u32> {
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

    fn location(path: &str) -> ArtifactLocation {
        ArtifactLocation::file(path)
    }

    #[test]
    fn acquire_same_location_reuses_handle_and_increments_refcount() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let l = location("a.lp");
        let r1 = m.acquire_location(l.clone(), FrameId::new(1));
        let r2 = m.acquire_location(l, FrameId::new(2));
        assert_eq!(r1.handle(), r2.handle());
        assert_eq!(m.refcount(&r1), Some(2));
    }

    #[test]
    fn release_decrements_refcount() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let r = m.acquire_location(location("b.lp"), FrameId::new(1));
        let h = r.handle();
        let r2 = m.acquire_location(location("b.lp"), FrameId::new(1));
        assert_eq!(m.refcount(&r), Some(2));
        m.release(&r2, FrameId::new(1)).unwrap();
        assert_eq!(m.refcount(&r), Some(1));
        assert_eq!(m.entry(&r).unwrap().id.handle(), h);
        assert_eq!(m.entry(&r).unwrap().location, location("b.lp"));
        assert_eq!(m.entry(&ArtifactId::from_raw(h)).unwrap().refcount, 1);
    }

    #[test]
    fn loaded_moves_to_idle_when_refcount_reaches_zero() {
        let mut m: ArtifactManager<i32> = ArtifactManager::new();
        let r = m.acquire_location(location("c.lp"), FrameId::new(1));
        m.load_with(&r, FrameId::new(5), |_location| Ok(42))
            .unwrap();
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
        let r = m.acquire_location(location("d.lp"), FrameId::new(1));
        m.load_with(&r, FrameId::new(10), |_location| Ok(1))
            .unwrap();
        assert_eq!(m.content_frame(&r), Some(FrameId::new(10)));
        m.load_with(&r, FrameId::new(99), |_location| Ok(2))
            .unwrap();
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
        let r = m.acquire_location(location("e.lp"), FrameId::new(1));
        let err = m
            .load_with(&r, FrameId::new(3), |_location| {
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
        let bad = ArtifactId::from_raw(999);
        assert_eq!(
            m.release(&bad, FrameId::default()).unwrap_err(),
            ArtifactError::UnknownHandle { handle: 999 }
        );
        assert_eq!(
            m.load_with(&bad, FrameId::default(), |_location| Ok(0))
                .unwrap_err(),
            ArtifactError::UnknownHandle { handle: 999 }
        );
        assert!(m.entry(&bad).is_none());
    }
}
