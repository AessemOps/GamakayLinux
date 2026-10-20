// SPDX-License-Identifier: GPL-3.0-or-later
//! A layer has to be read twice and agree before anything acts on it.
//!
//! This is the regression behind the "wrong layer" bug: the board answers a raw
//! page read with whatever report is ready, so a slow reply returns the
//! *previous* request's page and the layer is assembled from a mixture of the
//! base and Fn keymaps. `keymap set` writes back the whole layer it read, so an
//! unconfirmed read must not be handed to a caller — it becomes an error.

use lk67_core::cmd;
use lk67_core::error::Error;
use lk67_core::guard::Guard;
use lk67_core::keymap::{LAYER_BYTES, PAGES, Slot, SlotValue, TX_SLOTS};
use lk67_core::transport::{MockTransport, Session};

/// Register one full pass of page replies; `fill` decides what each page holds.
///
/// A page reply is the raw report: the board does not echo the opcode on a raw
/// page read (unlike a feature GET, which does), so these are registered as
/// literal 64-byte reports.
fn one_pass(mock: &mut MockTransport, fill: u8, opcode: u8) {
    for _ in 0..PAGES {
        mock.expect(opcode, [fill; 64]);
    }
}

#[test]
fn two_agreeing_passes_are_returned() {
    let mut mock = MockTransport::new();
    one_pass(&mut mock, 0x11, cmd::GET_KEYMATRIX);
    one_pass(&mut mock, 0x11, cmd::GET_KEYMATRIX);
    let mut session = Session::new(&mut mock, Guard::new(true)).with_settle(0);

    let layer = session.read_layer(0, false).expect("both passes agree");
    // 0x11 as a raw slot word is not a valid key, so it decodes to something
    // harmless — what matters is that a layer came back at all.
    assert_eq!(layer.slots[0], Slot([0x11; 4]));
}

#[test]
fn disagreeing_passes_are_an_error_not_a_mixture() {
    let mut mock = MockTransport::new();
    one_pass(&mut mock, 0x11, cmd::GET_KEYMATRIX);
    one_pass(&mut mock, 0x22, cmd::GET_KEYMATRIX);
    one_pass(&mut mock, 0x11, cmd::GET_KEYMATRIX);
    let mut session = Session::new(&mut mock, Guard::new(true)).with_settle(0);

    match session.read_layer(0, false) {
        Err(Error::UnstableLayer { .. }) => {}
        Err(e) => panic!("expected an unconfirmed-layer error, got {e}"),
        Ok(_) => panic!("an unconfirmed layer was returned as if it were good"),
    }
}

/// The Fn layer is read through its own opcode, so the two layers cannot be
/// confused for one another by the retry logic either.
#[test]
fn the_fn_layer_is_read_through_its_own_opcode() {
    let mut mock = MockTransport::new();
    one_pass(&mut mock, 0x33, cmd::GET_FN);
    one_pass(&mut mock, 0x33, cmd::GET_FN);
    let mut session = Session::new(&mut mock, Guard::new(true)).with_settle(0);

    session.read_layer(0, true).expect("the Fn layer reads");
    assert!(mock.sent.iter().all(|p| p[0] == cmd::GET_FN));
}

/// A bulk write transmits nine pages of 56 bytes, so the last two slots never
/// reach the board: front-ends refuse them rather than report a write that
/// cannot land.
#[test]
fn the_write_limit_is_below_the_layer_size() {
    const { assert!(TX_SLOTS == 126) };
    const { assert!(TX_SLOTS < lk67_core::keymap::SLOTS) };
    // Slots past the limit are still addressable in memory (reads return them),
    // which is why the check has to be explicit rather than an array bound.
    let layer = lk67_core::keymap::Keymap::from_bytes(&[0u8; LAYER_BYTES]);
    assert_eq!(layer.slots[TX_SLOTS], Slot::EMPTY);
    let _ = SlotValue::Hid(0x04);
    let _ = PAGES;
}
