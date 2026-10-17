// SPDX-License-Identifier: GPL-3.0-or-later
//! Enumerate hidraw nodes from sysfs and pick the settings node.

use std::fs;
use std::path::{Path, PathBuf};

use gamakay_core::model::Connection;

/// One hidraw node and what sysfs says about it.
#[derive(Clone, Debug)]
pub struct HidrawInfo {
    pub path: PathBuf,
    /// `HID_NAME`, e.g. "GamaKay LK67 Keyboard".
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub connection: Connection,
    /// Bytes of the largest feature report in the descriptor.
    pub feature_bytes: usize,
    /// Usage page / usage of the first application collection.
    pub usage_page: u16,
    pub usage: u16,
}

impl HidrawInfo {
    /// True when the node carries a settings feature report of at least the
    /// size the protocol uses.
    pub fn has_settings_report(&self) -> bool {
        self.feature_bytes >= 64
    }

    /// Whether this node belongs to a board this project drives rather than to
    /// some other HID device that merely looks similar. Front-ends must not
    /// write to anything this rejects: on this machine the Corsair mouse also
    /// carries a 64-byte feature report, so "has a settings report" alone picks
    /// the wrong device the moment the keyboard is away.
    pub fn is_gamakay(&self) -> bool {
        gamakay_core::device::is_gamakay(self.vendor_id, self.product_id)
    }
}

/// Map a Linux input bus id to a connection label. Only USB is supported; a
/// Bluetooth HOGP node arrives on bus `0x05` and is reported as `Unknown`.
pub fn connection_of(bus: u16) -> Connection {
    match bus {
        0x03 => Connection::Usb,
        _ => Connection::Unknown,
    }
}

/// Walk `/sys/class/hidraw` and describe every node that can be read.
pub fn enumerate() -> Vec<HidrawInfo> {
    let root = Path::new("/sys/class/hidraw");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if let Some(info) = read_node(&entry.path()) {
            out.push(info);
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The node that carries the 64-byte settings feature report, preferring a
/// known GamaKay USB id. `None` when no board is attached.
pub fn find_supported() -> Option<HidrawInfo> {
    let nodes = enumerate();
    // A known keyboard id with the settings report wins outright.
    let known = nodes.iter().find(|n| {
        n.has_settings_report()
            && gamakay_core::device::by_usb(n.vendor_id, n.product_id)
                .iter()
                .any(|d| d.family == gamakay_core::device::Family::Yc500)
    });
    known
        .or_else(|| nodes.iter().find(|n| n.has_settings_report()))
        .cloned()
}

/// A specific node by path. Accepts a `/dev/hidrawN` node or its
/// `/sys/class/hidraw/hidrawN` link.
pub fn find(path: impl AsRef<Path>) -> Option<HidrawInfo> {
    let p = path.as_ref();
    if p.starts_with("/dev") {
        let name = p.file_name()?;
        return read_node(&Path::new("/sys/class/hidraw").join(name));
    }
    read_node(p)
}

fn read_node(class_link: &Path) -> Option<HidrawInfo> {
    let device = class_link.join("device");
    let uevent = fs::read_to_string(device.join("uevent")).ok()?;
    let (vendor_id, product_id, bus) = parse_hid_id(&uevent)?;
    let name = parse_hid_name(&uevent).unwrap_or_default();
    let desc = fs::read(device.join("report_descriptor")).unwrap_or_default();
    let summary = parse_descriptor(&desc);
    Some(HidrawInfo {
        path: PathBuf::from("/dev").join(class_link.file_name()?),
        name,
        vendor_id,
        product_id,
        connection: connection_of(bus),
        feature_bytes: summary.feature_bytes,
        usage_page: summary.app_usage_page,
        usage: summary.app_usage,
    })
}

/// `HID_ID=0003:00000461:00004002` → (0x0461, 0x4002, 0x0003).
fn parse_hid_id(uevent: &str) -> Option<(u16, u16, u16)> {
    let line = uevent.lines().find(|l| l.starts_with("HID_ID="))?;
    let mut parts = line[7..].split(':');
    let bus = u16::from_str_radix(parts.next()?.trim(), 16).ok()?;
    let vid = u16::from_str_radix(parts.next()?.trim(), 16).ok()?;
    let pid = u16::from_str_radix(parts.next()?.trim(), 16).ok()?;
    Some((vid, pid, bus))
}

fn parse_hid_name(uevent: &str) -> Option<String> {
    uevent
        .lines()
        .find(|l| l.starts_with("HID_NAME="))
        .map(|l| l[9..].to_string())
}

/// What we need out of a HID report descriptor.
#[derive(Default, Debug, PartialEq, Eq)]
struct Summary {
    /// Bytes of the largest Feature report.
    feature_bytes: usize,
    /// Usage page / usage of the first application collection.
    app_usage_page: u16,
    app_usage: u16,
}

/// Minimal HID report-descriptor walk. We only need the main items (Feature
/// size, the first collection's usage) and the globals that precede them.
fn parse_descriptor(d: &[u8]) -> Summary {
    let mut s = Summary::default();
    let mut usage_page: u16 = 0;
    let mut report_size_bits: u32 = 0;
    let mut report_count: u32 = 0;
    let mut last_usage: u16 = 0;
    let mut got_app = false;

    let mut i = 0usize;
    while i < d.len() {
        let prefix = d[i];
        if prefix == 0xFE {
            // Long item: data size at [i+1], skip 3 + size.
            if i + 2 >= d.len() {
                break;
            }
            i += 3 + d[i + 1] as usize;
            continue;
        }
        let size = (prefix & 0x03) as usize;
        let ty = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        if i + 1 + size > d.len() {
            break;
        }
        let val = read_le(&d[i + 1..i + 1 + size]);
        match ty {
            // Main items.
            0 => match tag {
                0x0B => {
                    let bytes = (report_size_bits as usize * report_count as usize) / 8;
                    s.feature_bytes = s.feature_bytes.max(bytes);
                }
                0x0A if !got_app => {
                    s.app_usage_page = usage_page;
                    s.app_usage = last_usage;
                    got_app = true;
                }
                _ => {}
            },
            // Global items.
            1 => match tag {
                0x00 => usage_page = val as u16,
                0x07 => report_size_bits = val,
                0x09 => report_count = val,
                _ => {}
            },
            // Local items.
            2 if tag == 0x00 => last_usage = val as u16,
            _ => {}
        }
        i += 1 + size;
    }
    s
}

fn read_le(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    for (n, &b) in bytes.iter().enumerate().take(4) {
        v |= u32::from(b) << (8 * n);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    // The LK67's interface 0 descriptor, read from
    // /sys/class/hidraw/hidraw0/device/report_descriptor (Phase 0).
    const LK67_IF0: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25,
        0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x05,
        0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91,
        0x01, 0x05, 0x07, 0x19, 0x00, 0x2a, 0xff, 0x00, 0x95, 0x05, 0x75, 0x08, 0x15, 0x00, 0x26,
        0xff, 0x00, 0x81, 0x00, 0x05, 0xff, 0x09, 0x03, 0x75, 0x08, 0x95, 0x01, 0x81, 0x02, 0x05,
        0x0c, 0x09, 0x00, 0x15, 0x80, 0x25, 0x7f, 0x95, 0x40, 0x75, 0x08, 0xb1, 0x02, 0xc0,
    ];

    // Interface 1: consumer/context/system collections, a 3-byte vendor
    // input report, and no feature report.
    const LK67_IF1: &[u8] = &[
        0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x03, 0x19, 0x00, 0x2a, 0x3c, 0x03, 0x15, 0x00,
        0x26, 0x3c, 0x03, 0x95, 0x01, 0x75, 0x10, 0x81, 0x00, 0xc0,
    ];

    #[test]
    fn if0_carries_the_64_byte_feature_report() {
        let s = parse_descriptor(LK67_IF0);
        assert_eq!(s.feature_bytes, 64);
        assert_eq!(s.app_usage_page, 0x0001);
        assert_eq!(s.app_usage, 0x0006);
    }

    #[test]
    fn if1_has_no_settings_report() {
        let s = parse_descriptor(LK67_IF1);
        assert!(s.feature_bytes < 64);
    }

    #[test]
    fn hid_id_parses() {
        let u =
            "DRIVER=hid-generic\nHID_ID=0003:00000461:00004002\nHID_NAME=GamaKay LK67 Keyboard\n";
        assert_eq!(parse_hid_id(u), Some((0x0461, 0x4002, 0x0003)));
        assert_eq!(parse_hid_name(u).as_deref(), Some("GamaKay LK67 Keyboard"));
    }

    #[test]
    fn bus_maps_to_connection() {
        assert_eq!(connection_of(0x03), Connection::Usb);
        // Bluetooth HOGP arrives on bus 0x05 and is out of scope: it must be
        // reported as unknown rather than as something we claim to support.
        assert_eq!(connection_of(0x05), Connection::Unknown);
        assert_eq!(connection_of(0x00), Connection::Unknown);
    }
}
