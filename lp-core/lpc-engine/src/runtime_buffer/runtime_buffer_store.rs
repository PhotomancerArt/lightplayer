//! Engine-managed storage for versioned runtime buffers.

use alloc::collections::BTreeMap;

use lpc_model::{FrameId, Versioned};

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

/// Maps [`RuntimeBufferId`] to [`Versioned`] buffer payloads for [`crate::engine::Engine`].
pub struct RuntimeBufferStore {
    next_id: u32,
    buffers: BTreeMap<RuntimeBufferId, Versioned<RuntimeBuffer>>,
}

impl RuntimeBufferStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 0,
            buffers: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, buffer: Versioned<RuntimeBuffer>) -> RuntimeBufferId {
        let id = RuntimeBufferId::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.buffers.insert(id, buffer);
        id
    }

    pub fn get(&self, id: RuntimeBufferId) -> Option<&Versioned<RuntimeBuffer>> {
        self.buffers.get(&id)
    }

    pub fn get_mut(&mut self, id: RuntimeBufferId) -> Option<&mut Versioned<RuntimeBuffer>> {
        self.buffers.get_mut(&id)
    }

    pub fn get_mut_mark_updated(
        &mut self,
        id: RuntimeBufferId,
        frame: FrameId,
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
        buffer: Versioned<RuntimeBuffer>,
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

    use lpc_model::{FrameId, Versioned};

    use super::{RuntimeBufferError, RuntimeBufferStore};
    use crate::runtime_buffer::{
        RuntimeBuffer, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
        RuntimeTextureFormat,
    };

    #[test]
    fn store_inserts_and_retrieves_versioned_texture() {
        let mut store = RuntimeBufferStore::new();
        let buf = RuntimeBuffer::texture_rgba16(2, 2, vec![0xab; 16]);
        let frame = FrameId::new(3);
        let id = store.insert(Versioned::new(frame, buf.clone()));

        let got = store.get(id).expect("inserted");
        assert_eq!(got.changed_frame(), frame);
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
        let id = store.insert(Versioned::new(FrameId::new(1), RuntimeBuffer::raw(vec![1])));
        let replacement = RuntimeBuffer::raw(vec![2, 3]);
        let new_frame = FrameId::new(9);
        store
            .replace(id, Versioned::new(new_frame, replacement.clone()))
            .expect("replace existing");

        let got = store.get(id).expect("still present");
        assert_eq!(got.changed_frame(), new_frame);
        assert_eq!(got.value(), &replacement);
    }

    #[test]
    fn store_mut_marks_updated_frame() {
        let mut store = RuntimeBufferStore::new();
        let id = store.insert(Versioned::new(FrameId::new(1), RuntimeBuffer::raw(vec![1])));

        let buffer = store
            .get_mut_mark_updated(id, FrameId::new(7))
            .expect("existing buffer");
        buffer.bytes.push(2);

        let got = store.get(id).expect("still present");
        assert_eq!(got.changed_frame(), FrameId::new(7));
        assert_eq!(got.value().bytes, vec![1, 2]);
    }

    #[test]
    fn store_replace_unknown_returns_error() {
        let mut store = RuntimeBufferStore::new();
        let missing = RuntimeBufferId::new(99);
        let err = store
            .replace(
                missing,
                Versioned::new(FrameId::new(0), RuntimeBuffer::raw(vec![])),
            )
            .expect_err("unknown id");
        assert_eq!(err, RuntimeBufferError::UnknownBuffer { id: missing });
    }
}
