/// Implementation for foreign types
#[cfg(feature = "std")]
use std::{vec::Vec, collections::VecDeque, io::Cursor};
#[cfg(all(feature = "alloc",not(feature = "std")))]
use alloc::{vec::Vec, collections::VecDeque};

#[allow(unused_imports)]
use super::*;

#[cfg(feature = "std")]
use std::collections::TryReserveError;

#[cfg(all(not(feature = "std"), feature = "alloc"))]
use alloc::collections::TryReserveError;

#[cfg(any(feature = "std", feature = "alloc"))]
impl From<TryReserveError> for SerError {
    fn from(_err: TryReserveError) -> SerError {
        SerError::BufferFull
    }
}

#[cfg(any(feature = "std", feature = "alloc"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
impl SerWrite for Vec<u8> {
    type Error = SerError;

    #[inline]
    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        self.try_reserve(buf.len())?;
        self.extend_from_slice(buf);
        Ok(())
    }
    #[inline]
    fn write_byte(&mut self, byte: u8) -> SerResult<()> {
        // FIXME: (vec_push_within_capacity #100486)
        // if let Err(byte) = self.push_within_capacity(byte) {
        //     self.try_reserve(1)?;
        //     let _ = vec.push_within_capacity(byte);
        // }
        self.try_reserve(1)?;
        self.push(byte);
        Ok(())
    }
}

#[cfg(any(feature = "std", feature = "alloc"))]
#[cfg_attr(docsrs, doc(cfg(any(feature = "std", feature = "alloc"))))]
impl SerWrite for VecDeque<u8> {
    type Error = SerError;

    #[inline]
    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        self.try_reserve(buf.len())?;
        self.extend(buf.iter().copied());
        Ok(())
    }
    #[inline]
    fn write_byte(&mut self, byte: u8) -> SerResult<()> {
        self.try_reserve(1)?;
        self.push_back(byte);
        Ok(())
    }
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl<T> SerWrite for Cursor<T>
    where Cursor<T>: std::io::Write
{
    type Error = SerError;

    #[inline]
    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        std::io::Write::write_all(self, buf).map_err(|_| SerError::BufferFull)
    }
}

#[cfg(feature = "arrayvec")]
#[cfg_attr(docsrs, doc(cfg(feature = "arrayvec")))]
impl<T> From<arrayvec::CapacityError<T>> for SerError {
    fn from(_err: arrayvec::CapacityError<T>) -> SerError {
        SerError::BufferFull
    }
}

#[cfg(feature = "arrayvec")]
#[cfg_attr(docsrs, doc(cfg(feature = "arrayvec")))]
impl<const CAP: usize> SerWrite for arrayvec::ArrayVec<u8, CAP> {
    type Error = SerError;

    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        self.try_extend_from_slice(buf).map_err(From::from)
    }
    #[inline]
    fn write_byte(&mut self, byte: u8) -> SerResult<()> {
        self.try_push(byte).map_err(From::from)
    }
}

#[cfg(feature = "heapless")]
#[cfg_attr(docsrs, doc(cfg(feature = "heapless")))]
impl<const CAP: usize> SerWrite for heapless::Vec<u8, CAP> {
    type Error = SerError;

    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        self.extend_from_slice(buf).map_err(|_| SerError::BufferFull)
    }
    #[inline]
    fn write_byte(&mut self, byte: u8) -> SerResult<()> {
        self.push(byte).map_err(|_| SerError::BufferFull)
    }
}

#[cfg(feature = "smallvec")]
#[cfg_attr(docsrs, doc(cfg(feature = "smallvec")))]
impl From<smallvec::CollectionAllocErr> for SerError {
    fn from(_err: smallvec::CollectionAllocErr) -> SerError {
        SerError::BufferFull
    }
}

#[cfg(feature = "smallvec")]
#[cfg_attr(docsrs, doc(cfg(feature = "smallvec")))]
impl<const CAP: usize> SerWrite for smallvec::SmallVec<[u8; CAP]>
    where [u8; CAP]: smallvec::Array<Item=u8>,
{
    type Error = SerError;

    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        self.try_reserve(buf.len())?;
        self.extend_from_slice(buf);
        Ok(())
    }
    #[inline]
    fn write_byte(&mut self, byte: u8) -> SerResult<()> {
        self.try_reserve(1)?;
        self.push(byte);
        Ok(())
    }
}

#[cfg(feature = "tinyvec")]
macro_rules! implement_tinyvec_write {
    () => {
        fn write(&mut self, buf: &[u8]) -> SerResult<()> {
            let add_len = buf.len();
            if add_len == 0 {
                return Ok(())
            }
            if let Some(target) = self.grab_spare_slice_mut().get_mut(..add_len) {
                target.clone_from_slice(buf);
                self.set_len(self.len() + add_len);
                Ok(())
            }
            else {
                Err(SerError::BufferFull)
            }
        }
    };
}

#[cfg(feature = "tinyvec")]
#[cfg_attr(docsrs, doc(cfg(feature = "tinyvec")))]
impl<const CAP: usize> SerWrite for tinyvec::ArrayVec<[u8; CAP]>
    where [u8; CAP]: tinyvec::Array<Item=u8>,
{
    type Error = SerError;

    implement_tinyvec_write!{}

    #[inline]
    fn write_byte(&mut self, byte: u8) -> SerResult<()> {
        if self.try_push(byte).is_none() {
            Ok(())
        }
        else {
            Err(SerError::BufferFull)
        }
    }
}

#[cfg(feature = "tinyvec")]
#[cfg_attr(docsrs, doc(cfg(feature = "tinyvec")))]
impl SerWrite for tinyvec::SliceVec<'_, u8> {
    type Error = SerError;

    implement_tinyvec_write!{}
}

#[cfg(all(feature = "tinyvec", any(feature = "std", feature = "alloc")))]
#[cfg_attr(docsrs, doc(cfg(all(feature = "tinyvec", any(feature = "std", feature = "alloc")))))]
impl<const CAP: usize> SerWrite for tinyvec::TinyVec<[u8; CAP]>
    where [u8; CAP]: tinyvec::Array<Item=u8>
{
    type Error = SerError;

    fn write(&mut self, buf: &[u8]) -> SerResult<()> {
        self.try_reserve(buf.len())?;
        match self {
          tinyvec::TinyVec::Inline(a) => a.extend_from_slice(buf),
          tinyvec::TinyVec::Heap(h) => h.extend_from_slice(buf),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // SAFETY: this is safe only when the provided slice is never read from.
    #[cfg(any(feature = "std", feature = "alloc"))]
    unsafe fn oversize_bytes<'a>() -> &'a[u8] {
        let oversize = usize::try_from(i64::MAX).unwrap();
        let ptr = core::ptr::NonNull::<u8>::dangling().as_ptr();
        core::slice::from_raw_parts(ptr, oversize)
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_ser_write_vec() {
        let mut writer = Vec::new();
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(&writer, expected);
        unsafe {
            assert_eq!(writer.write(oversize_bytes()).unwrap_err(), SerError::BufferFull);
        }
    }

    #[cfg(any(feature = "std", feature = "alloc"))]
    #[test]
    fn test_ser_write_vec_deque() {
        let mut writer = VecDeque::new();
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(&writer, expected);
        unsafe {
            assert_eq!(writer.write(oversize_bytes()).unwrap_err(), SerError::BufferFull);
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_ser_write_cursor() {
        let mut writer = Cursor::new([0u8;22]);
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.get_ref(), expected);
        assert_eq!(writer.write_byte(b' ').unwrap_err(), SerError::BufferFull);
        assert_eq!(writer.write(b" ").unwrap_err(), SerError::BufferFull);
        writer.write(b"").unwrap();
    }

    #[cfg(feature = "arrayvec")]
    #[test]
    fn test_ser_write_arrayvec() {
        let mut writer = arrayvec::ArrayVec::<u8,22>::new();
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.as_slice(), expected);
        assert_eq!(writer.write_byte(b' ').unwrap_err(), SerError::BufferFull);
        assert_eq!(writer.write(b" ").unwrap_err(), SerError::BufferFull);
        writer.write(b"").unwrap();
    }

    #[cfg(feature = "heapless")]
    #[test]
    fn test_ser_write_heapless() {
        let mut writer = heapless::Vec::<u8,22>::new();
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.as_slice(), expected);
        assert_eq!(writer.write_byte(b' ').unwrap_err(), SerError::BufferFull);
        assert_eq!(writer.write(b" ").unwrap_err(), SerError::BufferFull);
        writer.write(b"").unwrap();
    }

    #[cfg(feature = "smallvec")]
    #[test]
    fn test_ser_write_smallvec() {
        let mut writer = smallvec::SmallVec::<[u8;12]>::new();
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.as_slice(), expected);
        unsafe {
            assert_eq!(writer.write(oversize_bytes()).unwrap_err(), SerError::BufferFull);
        }
    }

    #[cfg(feature = "tinyvec")]
    #[test]
    fn test_ser_write_tinyvec_arrayvec() {
        let mut writer = tinyvec::ArrayVec::<[u8; 22]>::new();
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.as_slice(), expected);
        assert_eq!(writer.write_byte(b' ').unwrap_err(), SerError::BufferFull);
        assert_eq!(writer.write(b" ").unwrap_err(), SerError::BufferFull);
        writer.write(b"").unwrap();
    }

    #[cfg(feature = "tinyvec")]
    #[test]
    fn test_ser_write_tinyvec_slicevec() {
        let mut buf = [0u8;22];
        let mut writer = tinyvec::SliceVec::from_slice_len(&mut buf, 0);
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.as_slice(), expected);
        assert_eq!(writer.write_byte(b' ').unwrap_err(), SerError::BufferFull);
        assert_eq!(writer.write(b" ").unwrap_err(), SerError::BufferFull);
        writer.write(b"").unwrap();
    }

    #[cfg(all(feature = "tinyvec", any(feature = "std", feature = "alloc")))]
    #[test]
    fn test_ser_write_tinyvec_tinyvec() {
        let mut writer = tinyvec::TinyVec::<[u8; 12]>::new();
        writer.write(b"Hello World!").unwrap();
        writer.write_byte(b' ').unwrap();
        writer.write_str("Good Bye!").unwrap();
        let expected = b"Hello World! Good Bye!";
        assert_eq!(writer.as_slice(), expected);
        unsafe {
            assert_eq!(writer.write(oversize_bytes()).unwrap_err(), SerError::BufferFull);
        }
    }
}
