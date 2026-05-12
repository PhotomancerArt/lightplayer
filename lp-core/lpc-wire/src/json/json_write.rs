//! Minimal write trait for bounded JSON emission.

use alloc::vec::Vec;
use core::convert::Infallible;

/// Byte sink used by the semantic JSON writer.
///
/// This intentionally mirrors only the operation the JSON writer needs. It is
/// small enough to adapt to host buffers, `ser-write-json`, and embedded serial
/// chunk writers without requiring `std::io::Write`.
pub trait JsonWrite {
    type Error;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

impl<T> JsonWrite for &mut T
where
    T: JsonWrite + ?Sized,
{
    type Error = T::Error;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        (**self).write_all(bytes)
    }
}

impl JsonWrite for Vec<u8> {
    type Error = Infallible;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

/// Test/helper writer that records how many bounded chunks were crossed.
#[derive(Debug)]
pub struct ChunkCountingWrite {
    bytes: Vec<u8>,
    chunk_size: usize,
    chunk_count: usize,
}

impl ChunkCountingWrite {
    #[must_use]
    pub fn new(chunk_size: usize) -> Self {
        Self {
            bytes: Vec::new(),
            chunk_size: chunk_size.max(1),
            chunk_count: 0,
        }
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunk_count
    }
}

impl JsonWrite for ChunkCountingWrite {
    type Error = Infallible;

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        for chunk in bytes.chunks(self.chunk_size) {
            self.bytes.extend_from_slice(chunk);
            self.chunk_count += 1;
        }
        Ok(())
    }
}
