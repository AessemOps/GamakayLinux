// SPDX-License-Identifier: GPL-3.0-or-later
//! GamaKay (ROYUAN) keyboard core: wire packets, the device table, the
//! command allow-list and the transport-agnostic conversation.
//!
//! Clean-room note: this crate is written from `docs/protocol.md`, which is
//! our own notes. Wire facts were confirmed against hardware and against the
//! GPL-3.0 `sharkfin` protocol document (see `CREDITS.md`). No vendor code
//! is linked or bundled.
//!
//! The transport is abstract so the same conversation is exercised against a
//! real device (hidraw) and against recorded fixtures ([`MockTransport`]).

pub mod cmd;
pub mod device;
pub mod error;
pub mod guard;
pub mod keycode;
pub mod keymap;
pub mod light;
pub mod macros;
pub mod model;
pub mod ops;
pub mod packet;
pub mod transport;

pub use device::{DeviceRecord, Family};
pub use error::{Error, Result};
pub use guard::Guard;
pub use keymap::{Keymap, Slot, SlotValue};
pub use light::{EdgeMode, FlagsPair, LK67_FLAGS, LedParam, Mode};
pub use macros::{Macro, MacroEvent};
pub use model::{Connection, DeviceInfo};
pub use packet::{Checksum, REPORT_LEN, apply_checksum};
pub use transport::{MockTransport, Session, Transport};
