//! The byte pipe `lp-cli link lab` drives a link over: a serial device, or a
//! TCP socket to an emulated board (`lp-cli emu run --link`).
//!
//! `--termios chrome` opens the serial device the way Chromium's Web Serial
//! does on macOS (PARMRK set, IGNBRK clear, IGNPAR; M1's
//! `scripts/link/tty-soak.py` names each flag and why), and folds the
//! kernel's doubled `0xFF 0xFF` back to one byte as Chromium's reader does,
//! so the browser's receive path can be exercised with no browser.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// How the serial device's termios is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum TermiosMode {
    /// Raw 8N1, what the product's lp-cli transport uses.
    Raw,
    /// Chromium's Web Serial open (PARMRK, IGNBRK clear), with its 0xFF fold.
    Chrome,
}

pub enum LabPort {
    #[cfg(unix)]
    Serial {
        port: serialport::TTYPort,
        /// Fold `0xFF 0xFF` → `0xFF` (Chromium's un-marking).
        fold: bool,
        /// The last byte read was a lone `0xFF`, waiting for its pair.
        held_ff: bool,
    },
    Tcp(TcpStream),
}

impl LabPort {
    pub fn open(spec: &str, termios: TermiosMode) -> Result<Self> {
        if let Some(addr) = spec.strip_prefix("tcp://") {
            let s = TcpStream::connect(addr).with_context(|| format!("connect {addr}"))?;
            s.set_read_timeout(Some(Duration::from_millis(1)))?;
            s.set_nodelay(true)?;
            return Ok(LabPort::Tcp(s));
        }
        #[cfg(unix)]
        {
            let port = serialport::new(spec, lpc_model::DEFAULT_SERIAL_BAUD_RATE)
                .data_bits(serialport::DataBits::Eight)
                .stop_bits(serialport::StopBits::One)
                .parity(serialport::Parity::None)
                .flow_control(serialport::FlowControl::None)
                .timeout(Duration::from_millis(1))
                .open_native()
                .with_context(|| format!("open serial port {spec}"))?;
            let fold = termios == TermiosMode::Chrome;
            if fold {
                chrome_termios(&port)?;
            }
            Ok(LabPort::Serial {
                port,
                fold,
                held_ff: false,
            })
        }
        #[cfg(not(unix))]
        bail!("serial ports need a unix host here; use tcp://")
    }

    /// Read what is there (0 after a ~1 ms timeout).
    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let r = match self {
            #[cfg(unix)]
            LabPort::Serial { port, .. } => port.read(buf),
            LabPort::Tcp(s) => s.read(buf),
        };
        let n = match r {
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok(0);
            }
            Ok(0) if matches!(self, LabPort::Tcp(_)) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "the link closed",
                ));
            }
            other => other?,
        };
        #[cfg(unix)]
        if let LabPort::Serial {
            fold: true,
            held_ff,
            ..
        } = self
        {
            return Ok(fold_ff(&mut buf[..n], held_ff));
        }
        Ok(n)
    }

    /// Write everything; gives up after five seconds without progress.
    pub fn write_all(&mut self, mut bytes: &[u8]) -> std::io::Result<()> {
        let mut stalled_since = Instant::now();
        while !bytes.is_empty() {
            let r = match self {
                #[cfg(unix)]
                LabPort::Serial { port, .. } => port.write(bytes),
                LabPort::Tcp(s) => s.write(bytes),
            };
            match r {
                Ok(n) if n > 0 => {
                    bytes = &bytes[n..];
                    stalled_since = Instant::now();
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
            if stalled_since.elapsed() > Duration::from_secs(5) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "the board took no host bytes for 5 s",
                ));
            }
        }
        Ok(())
    }
}

/// Chromium's un-marking in place: `0xFF 0xFF` → `0xFF`. A lone trailing
/// `0xFF` is held for the next read. Returns the new length.
fn fold_ff(buf: &mut [u8], held_ff: &mut bool) -> usize {
    let mut w = 0;
    let mut r = 0;
    // A 0xFF held from the last read pairs with this read's first byte. If
    // it does not pair it began a mark (`0xFF 0x00 x`, a parity or break
    // error USB never produces), and marks are dropped, as Chromium does.
    if *held_ff {
        *held_ff = false;
        if buf.first() == Some(&0xFF) {
            buf[0] = 0xFF;
            w = 1;
            r = 1;
        }
    }
    while r < buf.len() {
        let b = buf[r];
        if b == 0xFF {
            if r + 1 == buf.len() {
                *held_ff = true;
                r += 1;
                continue;
            }
            // `0xFF 0xFF` is one data byte; `0xFF 0x00 x` is a mark (dropped).
            if buf[r + 1] == 0xFF {
                buf[w] = 0xFF;
                w += 1;
                r += 2;
            } else {
                r += 2.min(buf.len() - r);
                if r < buf.len() {
                    r += 1;
                }
            }
            continue;
        }
        buf[w] = b;
        w += 1;
        r += 1;
    }
    w
}

#[cfg(unix)]
fn chrome_termios(port: &serialport::TTYPort) -> Result<()> {
    use std::os::fd::AsRawFd;
    let fd = port.as_raw_fd();
    // SAFETY: `fd` is the open tty `port` owns for the length of this call;
    // `t` is a plain C struct tcgetattr fills in.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) != 0 {
            bail!("tcgetattr: {}", std::io::Error::last_os_error());
        }
        t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ECHOE | libc::ECHONL | libc::ISIG);
        t.c_iflag &= !(libc::IGNBRK
            | libc::BRKINT
            | libc::ISTRIP
            | libc::INLCR
            | libc::IGNCR
            | libc::ICRNL
            | libc::IXON);
        t.c_iflag |= libc::PARMRK | libc::IGNPAR;
        t.c_iflag &= !libc::INPCK;
        t.c_oflag &= !libc::OPOST;
        t.c_cflag &= !(libc::CSIZE | libc::PARENB | libc::PARODD | libc::CSTOPB | libc::CRTSCTS);
        t.c_cflag |= libc::CS8 | libc::CREAD | libc::CLOCAL;
        t.c_cflag &= !libc::HUPCL;
        if libc::tcsetattr(fd, libc::TCSANOW, &t) != 0 {
            bail!("tcsetattr: {}", std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubled_ff_folds_to_one_even_across_reads() {
        let mut held = false;
        let mut a = [1, 0xFF, 0xFF, 2, 0xFF];
        let n = fold_ff(&mut a, &mut held);
        assert_eq!(&a[..n], &[1, 0xFF, 2]);
        assert!(held);
        let mut b = [0xFF, 3];
        let n = fold_ff(&mut b, &mut held);
        assert_eq!(&b[..n], &[0xFF, 3]);
        assert!(!held);
    }
}
