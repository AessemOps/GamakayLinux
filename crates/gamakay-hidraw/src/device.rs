// SPDX-License-Identifier: GPL-3.0-or-later
//! The hidraw node itself: the four `Transport` primitives over ioctls.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use gamakay_core::packet::REPORT_LEN;
use gamakay_core::transport::Transport;
use gamakay_core::{Error, Result};

/// The buffer a hidraw feature ioctl takes: one report-number byte, then the
/// 64 data bytes. The report number is 0 (no report ids on this descriptor).
pub const FEATURE_BUF_LEN: usize = REPORT_LEN + 1;

// Linux `_IOC` encoding, 64-bit.
const IOC_WRITE: u64 = 1;
const IOC_READ: u64 = 2;
const IOC_NRBITS: u64 = 8;
const IOC_TYPEBITS: u64 = 8;
const IOC_SIZEBITS: u64 = 14;
const IOC_NRSHIFT: u64 = 0;
const IOC_TYPESHIFT: u64 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u64 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u64 = IOC_SIZESHIFT + IOC_SIZEBITS;

const fn ioc(dir: u64, ty: u8, nr: u8, size: usize) -> u64 {
    (dir << IOC_DIRSHIFT)
        | ((ty as u64) << IOC_TYPESHIFT)
        | ((nr as u64) << IOC_NRSHIFT)
        | ((size as u64) << IOC_SIZESHIFT)
}

/// `HIDIOCSFEATURE(65)`: send one feature report.
pub const HIDIOCSFEATURE: u64 = ioc(IOC_READ | IOC_WRITE, b'H', 0x06, FEATURE_BUF_LEN);
/// `HIDIOCGFEATURE(65)`: read one feature report.
pub const HIDIOCGFEATURE: u64 = ioc(IOC_READ | IOC_WRITE, b'H', 0x07, FEATURE_BUF_LEN);

/// An open hidraw node. Kept non-blocking so a stalled board cannot hang the
/// process; reads poll with a timeout.
pub struct HidrawDevice {
    file: File,
    path: PathBuf,
}

impl HidrawDevice {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)
            .map_err(|e| Error::Transport(format!("open {}: {e}", path.display())))?;
        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn fd(&self) -> i32 {
        self.file.as_raw_fd()
    }

    /// A feature-report ioctl, copying through the 65-byte kernel buffer.
    fn feature_ioctl(&self, request: u64, buf: &mut [u8; FEATURE_BUF_LEN]) -> Result<()> {
        // SAFETY: fd is a valid open hidraw descriptor; buf is 65 bytes and the
        // request code was built for exactly that size.
        let rc = unsafe { libc::ioctl(self.fd(), request as libc::c_ulong, buf.as_mut_ptr()) };
        if rc < 0 {
            return Err(Error::Transport(format!(
                "ioctl 0x{request:08X} on {}: {}",
                self.path.display(),
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }
}

impl Transport for HidrawDevice {
    fn send_feature(&mut self, buf: &[u8; REPORT_LEN]) -> Result<()> {
        let mut wire = [0u8; FEATURE_BUF_LEN];
        wire[1..].copy_from_slice(buf);
        self.feature_ioctl(HIDIOCSFEATURE, &mut wire)
    }

    fn get_feature(&mut self, buf: &mut [u8; REPORT_LEN]) -> Result<usize> {
        let mut wire = [0u8; FEATURE_BUF_LEN];
        self.feature_ioctl(HIDIOCGFEATURE, &mut wire)?;
        buf.copy_from_slice(&wire[1..]);
        Ok(REPORT_LEN)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.file.write(buf).map_err(io_err)
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<usize> {
        let mut pfd = libc::pollfd {
            fd: self.fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd.
        let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if rc < 0 {
            return Err(io_err(std::io::Error::last_os_error()));
        }
        if rc == 0 {
            return Err(Error::NoReply { opcode: 0 });
        }
        match self.file.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Err(Error::NoReply { opcode: 0 }),
            Err(e) => Err(io_err(e)),
        }
    }

    fn describe(&self) -> String {
        self.path.display().to_string()
    }
}

fn io_err(e: std::io::Error) -> Error {
    Error::Transport(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Values printed by a C snippet against <linux/hidraw.h> with
    // HIDIOCSFEATURE(65) / HIDIOCGFEATURE(65). If the `_IOC` encoding ever
    // drifts, this catches it before the ioctl fails on hardware.
    #[test]
    fn ioctl_requests_match_the_kernel_header() {
        assert_eq!(HIDIOCSFEATURE, 0xC041_4806);
        assert_eq!(HIDIOCGFEATURE, 0xC041_4807);
    }

    #[test]
    fn feature_buffer_is_report_plus_report_id() {
        assert_eq!(FEATURE_BUF_LEN, 65);
    }
}
