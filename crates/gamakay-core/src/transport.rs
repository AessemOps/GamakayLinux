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
}

impl<'a> Session<'a> {
    pub fn new(transport: &'a mut dyn Transport, guard: Guard) -> Self {
        Self {
            transport,
            guard,
            settle_ms: SETTLE_MS,
        }
    }

    /// Override the settle gap. Tests use 0; the receiver relay needs more.
    pub fn with_settle(mut self, ms: u64) -> Self {
        self.settle_ms = ms;
        self
    }

    pub fn guard(&self) -> Guard {
        self.guard
    }

    /// Build through the guard, then hand the packet to the transport.
    pub fn send(&mut self, opcode: u8, payload: &[u8], mode: Checksum) -> Result<()> {
        let pkt = self.guard.build(opcode, payload, mode)?;
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
}
