//! A cursor over a relay frame's body, shared by the frame codec and the
//! frame types that read their own fields.

use crate::lan_address::LanAddress;
use crate::relay_frame::RelayFrameError;

/// A cursor over a frame's body; every read is length-checked.
pub(crate) struct FrameReader<'a> {
    pub(crate) rest: &'a [u8],
}

impl<'a> FrameReader<'a> {
    pub(crate) fn new(body: &'a [u8]) -> Self {
        Self { rest: body }
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], RelayFrameError> {
        if self.rest.len() < n {
            return Err(RelayFrameError::Truncated);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], RelayFrameError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, RelayFrameError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, RelayFrameError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, RelayFrameError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// A presence flag: `0` = absent, `1` = present, anything else is
    /// [`RelayFrameError::BadField`].
    pub(crate) fn flag(&mut self) -> Result<bool, RelayFrameError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(RelayFrameError::BadField),
        }
    }

    /// A `len u8` and that many bytes of UTF-8, at most `max` bytes.
    pub(crate) fn short_str(&mut self, max: usize) -> Result<&'a str, RelayFrameError> {
        let len = usize::from(self.u8()?);
        if len > max {
            return Err(RelayFrameError::BadField);
        }
        core::str::from_utf8(self.take(len)?).map_err(|_| RelayFrameError::BadField)
    }

    pub(crate) fn lan(&mut self) -> Result<Option<LanAddress>, RelayFrameError> {
        if !self.flag()? {
            return Ok(None);
        }
        Ok(Some(LanAddress {
            ip: self.array()?,
            port: self.u16()?,
        }))
    }
}
