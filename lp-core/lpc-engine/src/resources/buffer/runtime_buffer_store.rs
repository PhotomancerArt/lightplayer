//! Engine-managed storage for versioned runtime buffers.

use alloc::collections::BTreeMap;

use lpc_model::{NodeId, Revision, WithRevision};

use super::{RuntimeBuffer, RuntimeBufferId};

/// Failure when operating on [`RuntimeBufferStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeBufferError {
    UnknownBuffer { id: RuntimeBufferId },
}

impl RuntimeBufferError {
    #[must_use]
    pub fn unknown_buffer(id: RuntimeBufferId) -> Self {
        Self::UnknownBuffer { id }
    }
}

/// Maps [`RuntimeBufferId`] to [`WithRevision`] buffer payloads for [`crate::engine::Engine`].
///
/// [`insert`](RuntimeBufferStore::insert) allocates ids monotonically; ids are not reused for
/// the lifetime of this store.
pub struct RuntimeBufferStore {
    next_id: u32,
    buffers: BTreeMap<RuntimeBufferId, WithRevision<RuntimeBuffer>>,
    owners: BTreeMap<RuntimeBufferId, NodeId>,
}

impl RuntimeBufferStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 0,
            buffers: BTreeMap::new(),
            owners: BTreeMap::new(),
        }
    }

    /// Allocates a new id. Ids increase monotonically and are never reused after allocation.
    pub fn insert(&mut self, buffer: WithRevision<RuntimeBuffer>) -> RuntimeBufferId {
        self.insert_with_owner(buffer, None)
    }

    pub fn insert_owned(
        &mut self,
        owner: NodeId,
        buffer: WithRevision<RuntimeBuffer>,
    ) -> RuntimeBufferId {
        self.insert_with_owner(buffer, Some(owner))
    }

    fn insert_with_owner(
        &mut self,
        buffer: WithRevision<RuntimeBuffer>,
        owner: Option<NodeId>,
    ) -> RuntimeBufferId {
        let id = RuntimeBufferId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.buffers.insert(id, buffer);
        if let Some(owner) = owner {
            self.owners.insert(id, owner);
        }
        id
    }

    pub fn owner(&self, id: RuntimeBufferId) -> Option<NodeId> {
        self.owners.get(&id).copied()
    }

    pub fn get(&self, id: RuntimeBufferId) -> Option<&WithRevision<RuntimeBuffer>> {
        self.buffers.get(&id)
    }

    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    pub fn get_mut(&mut self, id: RuntimeBufferId) -> Option<&mut WithRevision<RuntimeBuffer>> {
        self.buffers.get_mut(&id)
    }

    /// Iterate all buffers in deterministic id order (for M4.1 resource summaries).
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (RuntimeBufferId, &WithRevision<RuntimeBuffer>)> + '_ {
        self.buffers.iter().map(|(&id, buf)| (id, buf))
    }

    pub fn get_mut_mark_updated(
        &mut self,
        id: RuntimeBufferId,
        frame: Revision,
    ) -> Result<&mut RuntimeBuffer, RuntimeBufferError> {
        let buffer = self
            .buffers
            .get_mut(&id)
            .ok_or_else(|| RuntimeBufferError::unknown_buffer(id))?;
        buffer.mark_updated(frame);
        Ok(buffer.get_mut())
    }

    pub fn replace(
        &mut self,
        id: RuntimeBufferId,
        buffer: WithRevision<RuntimeBuffer>,
    ) -> Result<(), RuntimeBufferError> {
        if !self.buffers.contains_key(&id) {
            return Err(RuntimeBufferError::unknown_buffer(id));
        }
        self.buffers.insert(id, buffer);
        Ok(())
    }
}

impl Default for RuntimeBufferStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use lpc_model::{Revision, WithRevision};

    use super::{RuntimeBufferError, RuntimeBufferStore};
    use crate::resource::{
        RuntimeBuffer, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
        RuntimeTextureFormat,
    };

    #[test]
    fn store_inserts_and_retrieves_versioned_texture() {
        let mut store = RuntimeBufferStore::new();
        let buf = RuntimeBuffer::texture_rgba16(2, 2, vec![0xab; 16]);
        let frame = Revision::new(3);
        let id = store.insert(WithRevision::new(frame, buf.clone()));

        let got = store.get(id).expect("inserted");
        assert_eq!(got.changed_at(), frame);
        assert_eq!(got.value(), &buf);
        assert_eq!(got.value().kind, RuntimeBufferKind::Texture);
        match &got.value().metadata {
            RuntimeBufferMetadata::Texture {
                width,
                height,
                format,
            } => {
                assert_eq!(*width, 2);
                assert_eq!(*height, 2);
                assert_eq!(*format, RuntimeTextureFormat::Rgba16);
            }
            _ => panic!("expected texture metadata"),
        }
    }

    #[test]
    fn store_replace_preserves_new_versioned_frame() {
        let mut store = RuntimeBufferStore::new();
        let id = store.insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![1]),
        ));
        let replacement = RuntimeBuffer::raw(vec![2, 3]);
        let new_frame = Revision::new(9);
        store
            .replace(id, WithRevision::new(new_frame, replacement.clone()))
            .expect("replace existing");

        let got = store.get(id).expect("still present");
        assert_eq!(got.changed_at(), new_frame);
        assert_eq!(got.value(), &replacement);
    }

    #[test]
    fn store_mut_marks_updated_frame() {
        let mut store = RuntimeBufferStore::new();
        let id = store.insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::raw(vec![1]),
        ));

        let buffer = store
            .get_mut_mark_updated(id, Revision::new(7))
            .expect("existing buffer");
        buffer.bytes.push(2);

        let got = store.get(id).expect("still present");
        assert_eq!(got.changed_at(), Revision::new(7));
        assert_eq!(got.value().bytes, vec![1, 2]);
    }

    #[test]
    fn store_replace_unknown_returns_error() {
        let mut store = RuntimeBufferStore::new();
        let missing = RuntimeBufferId::new(99);
        let err = store
            .replace(
                missing,
                WithRevision::new(Revision::new(0), RuntimeBuffer::raw(vec![])),
            )
            .expect_err("unknown id");
        assert_eq!(err, RuntimeBufferError::UnknownBuffer { id: missing });
    }
}
