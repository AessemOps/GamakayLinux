// SPDX-License-Identifier: GPL-3.0-or-later
//! The device table. Data-driven, modelled on the vendor's table and the
//! GPL-3.0 `sharkfin` registry, but only entries we can identify are listed.
//!
//! Identity is resolved from the `0x8F` handshake reply (the vendor's numeric
//! id), never from VID/PID: hundreds of boards share `1121:4002` and
//! `3151:4002`, so the USB ids pick a *candidate*, the handshake picks the
//! record.

/// The two colliding command sets. Opcodes overlap between them, so the
/// family decides what every write means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// The LK67's family (older, 0x8F identify, single-slot key writes).
    Yc500,
    /// Newer lineage; present in the table only to refuse it loudly.
    Gen2,
}

impl Family {
    pub const fn name(self) -> &'static str {
        match self {
            Family::Yc500 => "yc500",
            Family::Gen2 => "gen2",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DeviceRecord {
    /// Vendor table id, the value the `0x8F` handshake returns.
    pub id: u32,
    pub name: &'static str,
    pub display_name: &'static str,
    pub company: &'static str,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    /// Length reported by WebHID/hidapi: 64 data bytes, +1 for the report id.
    pub feature_report_byte_length: u16,
    pub family: Family,
    /// Number of onboard layers (the vendor's `layer`).
    pub layer: u8,
    pub profiles: u8,
    /// Vendor `support_onboard`: 2 means every setting is written to keyboard
    /// memory, so no resident host process is needed.
    pub support_onboard: u8,
    pub key_layout: &'static str,
    pub light_layout: &'static str,
    /// Whether the board has an edge/side light. Presence comes from the
    /// record, not from a GET reply (the firmware answers one anyway).
    pub side_light: bool,
    pub keys: u16,
}

/// The GamaKay LK67, wired USB. The only board this project targets and tests.
pub const LK67: DeviceRecord = DeviceRecord {
    id: 149,
    name: "yzw_lk67",
    display_name: "LK67",
    company: "gamakay2",
    vendor_id: 0x0461,
    product_id: 0x4002,
    usage_page: 0x0001,
    usage: 0x0006,
    feature_report_byte_length: 65,
    family: Family::Yc500,
    layer: 8,
    profiles: 3,
    support_onboard: 2,
    key_layout: "LK67",
    light_layout: "LK67",
    side_light: false,
    keys: 67,
};

/// A known sibling that shares the LK67's USB ids but is a different board.
/// Present so a mis-identified handshake is caught rather than assumed.
pub const YZ68: DeviceRecord = DeviceRecord {
    id: 244,
    name: "yzw_yz68_dm",
    display_name: "YZ68",
    company: "YUNZII",
    vendor_id: 1121,
    product_id: 16386,
    usage_page: 0xFFFF,
    usage: 0x0002,
    feature_report_byte_length: 65,
    family: Family::Yc500,
    layer: 8,
    profiles: 2,
    support_onboard: 2,
    key_layout: "Common68_RT68~k1",
    light_layout: "se",
    side_light: false,
    keys: 68,
};

pub const DEVICES: &[DeviceRecord] = &[LK67, YZ68];

/// The vendor-table id a board with mismatched (Banggood/original) firmware
/// reports for the LK67. Seen in the wild as 71 instead of 149.
pub const LK67_VARIANT_IDS: &[u32] = &[71];

/// Look up a record by its numeric id.
pub fn by_id(id: u32) -> Option<&'static DeviceRecord> {
    DEVICES.iter().find(|d| d.id == id)
}

/// Candidate records sharing a USB id. Ambiguous by design.
pub fn by_usb(vendor_id: u16, product_id: u16) -> Vec<&'static DeviceRecord> {
    DEVICES
        .iter()
        .filter(|d| d.vendor_id == vendor_id && d.product_id == product_id)
        .collect()
}

/// True when the id is the LK67 under its documented mismatched firmware.
pub fn is_lk67_variant(id: u32) -> bool {
    LK67_VARIANT_IDS.contains(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lk67_is_the_only_supported_target() {
        assert_eq!(by_id(149).unwrap().display_name, "LK67");
    }

    #[test]
    fn variant_id_is_flagged_not_confused() {
        assert!(by_id(71).is_none());
        assert!(is_lk67_variant(71));
        assert!(!is_lk67_variant(149));
    }

    #[test]
    fn usb_ids_are_ambiguous() {
        // 0x0461:0x4002 is the same pair as 1121:16386 in decimal, and the
        // LK67 and YZ68 share it: USB ids are only a hint, never an identity.
        assert_eq!(by_usb(0x0461, 0x4002).len(), 2);
        assert_eq!(by_usb(1121, 16386).len(), 2);
    }
}
