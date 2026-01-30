use alloc::collections::VecDeque;

struct SerialHost {
    pub(super) to_guest_buf: VecDeque<u8>,
    pub(super) from_guest_buf: VecDeque<u8>,
}

impl SerialHost {
    pub fn new(buffer_size: usize) -> Self {
        SerialHost {
            to_guest_buf: VecDeque::with_capacity(buffer_size),
            from_guest_buf: VecDeque::with_capacity(buffer_size),
        }
    }

    /// Handles guest to host writes
    /// Called by the handler for SYSCALL_SERIAL_WRITE
    pub fn guest_write(&mut self, buffer: &[u8]) -> i32 {
        // put data in from_guest_buf
        todo!()
    }


    /// Handles the guest writing data
    /// Called by the handler for SYSCALL_SERIAL_WRITE
    pub fn guest_read(
        &mut self,
        buffer: &mut [u8],
        offset: usize,
        max_len: usize
    ) -> i32 {
        // consume data from to_guest_buf
        todo!()
    }

    /// Handles the host writing data
    /// Called by the user of the emulator to send data to the guest
    pub fn host_write(&mut self, buffer: &[u8]) -> Result<usize, SerialError> {
        // write data to_guest_buf
        todo!()
    }

    /// Handles the host reading
    /// Called by the user of the emulator to read data from the guest
    pub fn host_read(
        &mut self,
        buffer: &mut [u8]
        // do we need offset and len? not sure what's idiomatic in rust
    ) -> Result<usize, SerialError> {
        // write data to_guest_buf
        todo!()
    }
}

#[cfg(test)]
mod tests {
    
}