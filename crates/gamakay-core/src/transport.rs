// SPDX-License-Identifier: GPL-3.0-or-later
//! Transport abstraction and the transport-agnostic conversation.
//!
//! `Transport` is the four primitives a backend must provide (the plan's
//! `send_feature`/`get_feature`/`write`/`read_timeout`). `Session` layers the
//! protocol on top: it builds packets through the [`Guard`] and retries reads
//! until the opcode echoes back. A `MockTransport` replays fixtures so the
//! conversation is tested without hardware.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::thread::sleep;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::guard::Guard;
use crate::packet::{Checksum, REPORT_LEN};

/// How long a board is given to settle before its reply is read back, and how
/// many times the read is retried (with a growing gap).
pub const SETTLE_MS: u64 = 10;
pub const REPLY_TRIES: u32 = 5;

/// The raw device operations. The hidraw backend implements this against
/// `/dev/hidraw*`; the mock implements it against a fixture table.
pub trait Transport {
    /// Send one 64-byte feature report (report id 0).
    fn send_feature(&mut self, buf: &[u8; REPORT_LEN]) -> Result<()>;
    /// Read one 64-byte feature report. Returns the byte count filled.
    fn get_feature(&mut self, buf: &mut [u8; REPORT_LEN]) -> Result<usize>;
    /// Write an output report / raw bytes. Unused by the app-mode protocol.
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    /// Read with a timeout. Unused by the app-mode protocol.
    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> Result<usize>;
    /// A short human description of the node, for diagnostics.
    fn describe(&self) -> String {
        "transport".to_string()
    }
}

/// The conversation. Holds a mutable transport and the guard.
pub struct Session<'a> {
    transport: &'a mut dyn Transport,
    guard: Guard,
    settle_ms: u64,
    /// Gap inserted before a write. The firmware needs a pause after a read
    /// reply before it will accept a feature SET; too small and the SET times
    /// out and can stall the control endpoint. **[HW]**
    write_gap_ms: u64,
    /// Pause after a write. The board is busy committing the change and NAKs
    /// any control transfer during that window; a transfer sent then times out
    /// and stalls the endpoint until re-enumeration. Waiting here is what
    /// keeps a following command (in this run or the next) healthy. **[HW]**
    write_commit_ms: u64,
    /// Packets a dry run withheld from the wire, in order.
    withheld: Vec<[u8; REPORT_LEN]>,
}

/// Gap before a write, in milliseconds.
pub const WRITE_GAP_MS: u64 = 20;
/// Pause after a write, in milliseconds.
pub const WRITE_COMMIT_MS: u64 = 1500;

impl<'a> Session<'a> {
    pub fn new(transport: &'a mut dyn Transport, guard: Guard) -> Self {
        Self {
            transport,
            guard,
            settle_ms: SETTLE_MS,
            write_gap_ms: WRITE_GAP_MS,
            write_commit_ms: WRITE_COMMIT_MS,
            withheld: Vec::new(),
        }
    }

    /// Override the settle gap. Tests use 0; the receiver relay needs more.
    pub fn with_settle(mut self, ms: u64) -> Self {
        self.settle_ms = ms;
        self
    }

    /// Override the post-write commit pause. Tests use 0.
    pub fn with_write_commit(mut self, ms: u64) -> Self {
        self.write_commit_ms = ms;
        self
    }

    pub fn guard(&self) -> Guard {
        self.guard
    }

    /// Write packets a dry run withheld from the wire.
    pub fn withheld(&self) -> &[[u8; REPORT_LEN]] {
        &self.withheld
    }

    /// Build through the guard, then hand the packet to the transport.
    ///
    /// Under `--dry-run` a *write* (opcode below `0x80`) is recorded instead
    /// of sent; reads (`0x80..`) still travel so the command can report the
    /// current state. A real write is followed by a commit pause so the next
    /// command does not hit the busy window.
    pub fn send(&mut self, opcode: u8, payload: &[u8], mode: Checksum) -> Result<()> {
        let pkt = self.guard.build(opcode, payload, mode)?;
        if opcode < 0x80 {
            if self.guard.dry_run {
                self.withheld.push(pkt);
                return Ok(());
            }
            sleep(Duration::from_millis(self.write_gap_ms));
            self.transport.send_feature(&pkt)?;
            sleep(Duration::from_millis(self.write_commit_ms));
            return Ok(());
        }
        self.transport.send_feature(&pkt)
    }

    /// Read one report into `buf`.
    pub fn read(&mut self, buf: &mut [u8; REPORT_LEN]) -> Result<usize> {
        self.transport.get_feature(buf)
    }

    /// Send, then poll for a reply whose byte 0 echoes the opcode.
    pub fn roundtrip(
        &mut self,
        opcode: u8,
        payload: &[u8],
        mode: Checksum,
    ) -> Result<[u8; REPORT_LEN]> {
        self.send(opcode, payload, mode)?;
        self.poll_reply(opcode)
    }

    /// Poll for a reply already requested (used after relay release).
    pub fn poll_reply(&mut self, opcode: u8) -> Result<[u8; REPORT_LEN]> {
        let mut buf = [0u8; REPORT_LEN];
        for attempt in 0..REPLY_TRIES {
            sleep(Duration::from_millis(
                self.settle_ms * u64::from(attempt + 1),
            ));
            let n = self.read(&mut buf)?;
            if n < REPORT_LEN {
                // A short report is not the reply we want; keep polling.
                continue;
            }
            if buf[0] == opcode {
                return Ok(buf);
            }
        }
        Err(Error::NoReply { opcode })
    }

    /// Send, settle, and read a reply that carries raw page bytes and does
    /// not echo the opcode (bulk keymatrix/Fn/macro/userpic reads).
    pub fn read_raw_page(
        &mut self,
        opcode: u8,
        payload: &[u8],
        mode: Checksum,
    ) -> Result<[u8; REPORT_LEN]> {
        self.send(opcode, payload, mode)?;
        sleep(Duration::from_millis(self.settle_ms));
        let mut buf = [0u8; REPORT_LEN];
        self.read(&mut buf)?;
        Ok(buf)
    }
}

/// A transport that replays recorded replies, keyed by the request opcode.
/// Used by unit tests and by `--replay` style fixture checks.
#[derive(Default)]
pub struct MockTransport {
    replies: HashMap<u8, VecDeque<[u8; REPORT_LEN]>>,
    /// Every packet that was sent, in order.
    pub sent: Vec<[u8; REPORT_LEN]>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one reply for a request opcode. Replies are consumed FIFO.
    pub fn expect(&mut self, opcode: u8, reply: [u8; REPORT_LEN]) -> &mut Self {
        self.replies.entry(opcode).or_default().push_back(reply);
        self
    }

    /// Register a reply built from an opcode and payload (device replies echo
    /// the opcode, so this is just [`crate::packet::packet`]).
    pub fn expect_payload(&mut self, opcode: u8, payload: &[u8], mode: Checksum) -> &mut Self {
        let reply = crate::packet::packet(opcode, payload, mode).expect("reply fits");
        self.expect(opcode, reply)
    }

    /// The opcode of the last packet sent.
    pub fn last_opcode(&self) -> Option<u8> {
        self.sent.last().map(|p| p[0])
    }
}

impl Transport for MockTransport {
    fn send_feature(&mut self, buf: &[u8; REPORT_LEN]) -> Result<()> {
        self.sent.push(*buf);
        Ok(())
    }

    fn get_feature(&mut self, buf: &mut [u8; REPORT_LEN]) -> Result<usize> {
        let op = self
            .sent
            .last()
            .map(|p| p[0])
            .ok_or(Error::NoReply { opcode: 0 })?;
        match self.replies.get_mut(&op).and_then(|q| q.pop_front()) {
            Some(reply) => {
                *buf = reply;
                Ok(REPORT_LEN)
            }
            None => Err(Error::NoReply { opcode: op }),
        }
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        Ok(buf.len())
    }

    fn read_timeout(&mut self, _buf: &mut [u8], _timeout_ms: i32) -> Result<usize> {
        Err(Error::Transport("mock has no timed read".into()))
    }

    fn describe(&self) -> String {
        "mock".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd;

    #[test]
    fn roundtrip_parses_identify() {
        let mut mock = MockTransport::new();
        // id 149 little-endian.
        mock.expect_payload(cmd::GET_USB_VERSION, &[149, 0, 0, 0], Checksum::Bit7);
        let guard = Guard::new(false);
        let mut session = Session::new(&mut mock, guard).with_settle(0);
        let reply = session
            .roundtrip(cmd::GET_USB_VERSION, &[], Checksum::Bit7)
            .unwrap();
        assert_eq!(
            u32::from_le_bytes([reply[1], reply[2], reply[3], reply[4]]),
            149
        );
        assert_eq!(mock.last_opcode(), Some(0x8F));
    }

    #[test]
    fn missing_reply_times_out() {
        let mut mock = MockTransport::new();
        let guard = Guard::new(false);
        let mut session = Session::new(&mut mock, guard).with_settle(0);
        let err = session
            .roundtrip(cmd::GET_REVISION, &[], Checksum::Bit7)
            .unwrap_err();
        assert!(matches!(err, Error::NoReply { opcode: 0x80 }));
    }

    #[test]
    fn dry_run_withholds_writes_but_sends_reads() {
        let mut mock = MockTransport::new();
        mock.expect_payload(cmd::GET_USB_VERSION, &[149, 0, 0, 0], Checksum::Bit7);
        let mut session = Session::new(&mut mock, Guard::new(true)).with_settle(0);

        // A write is recorded, not sent.
        session
            .send(cmd::SET_LEDPARAM, &[1, 1, 1, 7, 0, 0, 0], Checksum::Bit8)
            .unwrap();
        assert_eq!(session.withheld().len(), 1);
        assert_eq!(session.withheld()[0][0], cmd::SET_LEDPARAM);

        // A read still travels.
        let reply = session
            .roundtrip(cmd::GET_USB_VERSION, &[], Checksum::Bit7)
            .unwrap();
        assert_eq!(reply[0], cmd::GET_USB_VERSION);
        assert_eq!(session.withheld().len(), 1);
    }
}
