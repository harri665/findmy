//! Bluetooth HCI commands and events for findmy beacons.
//!
//! Every beacon talks to its Bluetooth controller through the same handful of
//! HCI commands: set an address, set advertising parameters and data, and
//! enable advertising. This crate builds those commands as H4 packets
//! (`01 | opcode | length | params`) and parses the controller's replies
//! (`04 | event code | length | params`). It is `no_std` and allocation-free,
//! so ESP32 firmware, the Linux daemon and the Windows tools share it.
//!
//! Two command families:
//! - **legacy** advertising (Bluetooth 4.x), which the ESP32 uses: one
//!   advertiser that we own completely;
//! - **extended** advertising (Bluetooth 5), used where we must coexist with
//!   the OS's own advertising (Linux, Windows): our own advertising *set*,
//!   with its own random address, sending legacy non-connectable PDUs so
//!   iPhones see an ordinary 31-byte advert.

#![no_std]
#![forbid(unsafe_code)]

/// H4 packet indicator for commands.
pub const H4_COMMAND: u8 = 0x01;
/// H4 packet indicator for events.
pub const H4_EVENT: u8 = 0x04;

/// HCI opcodes (OGF << 10 | OCF).
pub mod opcode {
    pub const RESET: u16 = 0x0C03;
    pub const LE_SET_RANDOM_ADDRESS: u16 = 0x2005;
    pub const LE_SET_ADV_PARAMS: u16 = 0x2006;
    pub const LE_SET_ADV_DATA: u16 = 0x2008;
    pub const LE_SET_ADV_ENABLE: u16 = 0x200A;
    pub const LE_SET_ADV_SET_RANDOM_ADDRESS: u16 = 0x2035;
    pub const LE_SET_EXT_ADV_PARAMS: u16 = 0x2036;
    pub const LE_SET_EXT_ADV_DATA: u16 = 0x2037;
    pub const LE_SET_EXT_ADV_ENABLE: u16 = 0x2039;
    pub const LE_READ_NUM_SUPPORTED_ADV_SETS: u16 = 0x203B;
    pub const LE_REMOVE_ADV_SET: u16 = 0x203C;
}

/// Convert milliseconds to advertising-interval units (0.625 ms).
pub const fn interval_from_ms(ms: u32) -> u32 {
    ms * 8 / 5
}

/// Largest parameter block we build (extended adv data: 4 + 31).
const MAX_PARAMS: usize = 40;

/// An HCI command as an H4 packet.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Command {
    buf: [u8; 4 + MAX_PARAMS],
    len: usize,
}

impl Command {
    fn new(opcode: u16, params: &[u8]) -> Self {
        debug_assert!(params.len() <= MAX_PARAMS);
        let mut buf = [0u8; 4 + MAX_PARAMS];
        buf[0] = H4_COMMAND;
        buf[1..3].copy_from_slice(&opcode.to_le_bytes());
        buf[3] = params.len() as u8;
        buf[4..4 + params.len()].copy_from_slice(params);
        Command { buf, len: 4 + params.len() }
    }

    /// The full H4 packet, including the `0x01` indicator.
    pub fn h4(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn opcode(&self) -> u16 {
        u16::from_le_bytes([self.buf[1], self.buf[2]])
    }

    pub fn params(&self) -> &[u8] {
        &self.buf[4..self.len]
    }
}

impl core::fmt::Debug for Command {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Command(0x{:04x}, {:02x?})", self.opcode(), self.params())
    }
}

pub fn reset() -> Command {
    Command::new(opcode::RESET, &[])
}

// --- Legacy advertising ------------------------------------------------------

/// `LE Set Random Address`. `addr` is in HCI byte order (LSB first), as
/// `fm_core::advert::Advertisement::address_hci` returns it.
pub fn le_set_random_address(addr: [u8; 6]) -> Command {
    Command::new(opcode::LE_SET_RANDOM_ADDRESS, &addr)
}

/// `LE Set Advertising Parameters`: non-connectable undirected advertising
/// (`ADV_NONCONN_IND`) from our random address on all three channels.
/// `interval` is in 0.625 ms units, 0x00A0..=0x4000 for this PDU type.
pub fn le_set_adv_params_nonconn(interval: u16) -> Command {
    let mut p = [0u8; 15];
    p[0..2].copy_from_slice(&interval.to_le_bytes()); // min
    p[2..4].copy_from_slice(&interval.to_le_bytes()); // max
    p[4] = 0x03; // ADV_NONCONN_IND
    p[5] = 0x01; // own address: random
    p[6] = 0x00; // peer address type (unused)
    // p[7..13]: peer address (unused)
    p[13] = 0x07; // channels 37, 38, 39
    p[14] = 0x00; // no filter
    Command::new(opcode::LE_SET_ADV_PARAMS, &p)
}

/// `LE Set Advertising Data`. Always sends the full 31-byte field.
pub fn le_set_adv_data(data: &[u8]) -> Command {
    let len = data.len().min(31);
    let mut p = [0u8; 32];
    p[0] = len as u8;
    p[1..1 + len].copy_from_slice(&data[..len]);
    Command::new(opcode::LE_SET_ADV_DATA, &p)
}

pub fn le_set_adv_enable(enable: bool) -> Command {
    Command::new(opcode::LE_SET_ADV_ENABLE, &[enable as u8])
}

// --- Extended advertising (Bluetooth 5) ---------------------------------------

/// `LE Set Extended Advertising Parameters` for set `handle`: legacy
/// `ADV_NONCONN_IND` PDUs from the set's own random address on all channels.
/// `interval` is in 0.625 ms units (24-bit).
pub fn le_set_ext_adv_params_nonconn(handle: u8, interval: u32) -> Command {
    let iv = interval.to_le_bytes();
    let mut p = [0u8; 25];
    p[0] = handle;
    p[1..3].copy_from_slice(&0x0010u16.to_le_bytes()); // properties: legacy PDU, non-connectable, non-scannable
    p[3..6].copy_from_slice(&iv[..3]); // primary interval min
    p[6..9].copy_from_slice(&iv[..3]); // primary interval max
    p[9] = 0x07; // channels 37, 38, 39
    p[10] = 0x01; // own address: random (the set's address)
    p[11] = 0x00; // peer address type
    // p[12..18]: peer address (unused)
    p[18] = 0x00; // no filter
    p[19] = 0x7F; // TX power: no preference
    p[20] = 0x01; // primary PHY: LE 1M
    p[21] = 0x00; // secondary max skip
    p[22] = 0x01; // secondary PHY: LE 1M
    p[23] = 0x00; // advertising SID
    p[24] = 0x00; // no scan request notifications
    Command::new(opcode::LE_SET_EXT_ADV_PARAMS, &p)
}

/// `LE Set Advertising Set Random Address`. `addr` in HCI byte order.
pub fn le_set_adv_set_random_address(handle: u8, addr: [u8; 6]) -> Command {
    let mut p = [0u8; 7];
    p[0] = handle;
    p[1..].copy_from_slice(&addr);
    Command::new(opcode::LE_SET_ADV_SET_RANDOM_ADDRESS, &p)
}

/// `LE Set Extended Advertising Data`, complete in one fragment (legacy PDUs
/// allow at most 31 bytes).
pub fn le_set_ext_adv_data(handle: u8, data: &[u8]) -> Command {
    let len = data.len().min(31);
    let mut p = [0u8; 4 + 31];
    p[0] = handle;
    p[1] = 0x03; // operation: complete data
    p[2] = 0x01; // fragment preference: controller should not fragment
    p[3] = len as u8;
    p[4..4 + len].copy_from_slice(&data[..len]);
    Command::new(opcode::LE_SET_EXT_ADV_DATA, &p[..4 + len])
}

/// `LE Set Extended Advertising Enable` for one set, no duration limit.
pub fn le_set_ext_adv_enable(handle: u8, enable: bool) -> Command {
    // enable, number of sets, then per set: handle, duration (2), max events.
    Command::new(opcode::LE_SET_EXT_ADV_ENABLE, &[enable as u8, 1, handle, 0, 0, 0])
}

pub fn le_read_num_supported_adv_sets() -> Command {
    Command::new(opcode::LE_READ_NUM_SUPPORTED_ADV_SETS, &[])
}

pub fn le_remove_adv_set(handle: u8) -> Command {
    Command::new(opcode::LE_REMOVE_ADV_SET, &[handle])
}

// --- Events -----------------------------------------------------------------------

const EVT_COMMAND_COMPLETE: u8 = 0x0E;
const EVT_COMMAND_STATUS: u8 = 0x0F;

/// HCI status 0x00.
pub const STATUS_SUCCESS: u8 = 0x00;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// `Command Complete`. `status` is the first return parameter (every
    /// command we send returns one); `rest` is what follows it.
    CommandComplete { opcode: u16, status: u8, rest: &'a [u8] },
    /// `Command Status`, sent instead of Command Complete by some commands.
    CommandStatus { opcode: u16, status: u8 },
    /// Anything else (advertising reports, disconnections, ...).
    Other { code: u8, params: &'a [u8] },
}

impl Event<'_> {
    /// If this event finishes `opcode`, its status.
    pub fn status_for(&self, opcode: u16) -> Option<u8> {
        match *self {
            Event::CommandComplete { opcode: o, status, .. } if o == opcode => Some(status),
            Event::CommandStatus { opcode: o, status } if o == opcode => Some(status),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    Truncated,
}

/// Parse one event. `packet` may start with the H4 indicator `0x04` or with
/// the event code directly (Linux HCI sockets deliver both forms).
pub fn parse_event(packet: &[u8]) -> Result<Event<'_>, ParseError> {
    let body = match packet.first() {
        Some(&H4_EVENT) => &packet[1..],
        Some(_) => packet,
        None => return Err(ParseError::Truncated),
    };
    if body.len() < 2 {
        return Err(ParseError::Truncated);
    }
    let (code, len) = (body[0], body[1] as usize);
    let params = body.get(2..2 + len).ok_or(ParseError::Truncated)?;
    Ok(match code {
        EVT_COMMAND_COMPLETE => {
            // num_hci_command_packets (1), opcode (2), return params...
            if params.len() < 4 {
                return Err(ParseError::Truncated);
            }
            Event::CommandComplete {
                opcode: u16::from_le_bytes([params[1], params[2]]),
                status: params[3],
                rest: &params[4..],
            }
        }
        EVT_COMMAND_STATUS => {
            // status (1), num_hci_command_packets (1), opcode (2)
            if params.len() < 4 {
                return Err(ParseError::Truncated);
            }
            Event::CommandStatus { opcode: u16::from_le_bytes([params[2], params[3]]), status: params[0] }
        }
        code => Event::Other { code, params },
    })
}

/// Human-readable name for common HCI status codes.
pub fn status_name(status: u8) -> &'static str {
    match status {
        0x00 => "success",
        0x01 => "unknown HCI command",
        0x07 => "memory capacity exceeded",
        0x0C => "command disallowed",
        0x11 => "unsupported feature or parameter",
        0x12 => "invalid HCI command parameters",
        0x42 => "unknown advertising identifier",
        0x43 => "limit reached",
        _ => "other error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_units() {
        assert_eq!(interval_from_ms(2000), 3200);
        assert_eq!(interval_from_ms(100), 160);
    }

    #[test]
    fn reset_packet() {
        assert_eq!(reset().h4(), &[0x01, 0x03, 0x0C, 0x00]);
    }

    #[test]
    fn legacy_commands() {
        let addr = [0x26, 0xE0, 0xA1, 0xC0, 0x40, 0xC9];
        assert_eq!(le_set_random_address(addr).h4(), &[0x01, 0x05, 0x20, 6, 0x26, 0xE0, 0xA1, 0xC0, 0x40, 0xC9]);

        let p = le_set_adv_params_nonconn(3200);
        assert_eq!(&p.h4()[..4], &[0x01, 0x06, 0x20, 15]);
        assert_eq!(p.params(), &[0x80, 0x0C, 0x80, 0x0C, 0x03, 0x01, 0, 0, 0, 0, 0, 0, 0, 0x07, 0]);

        let data = [0xAB; 31];
        let d = le_set_adv_data(&data);
        assert_eq!(&d.h4()[..5], &[0x01, 0x08, 0x20, 32, 31]);
        assert_eq!(&d.params()[1..], &data);

        // Short data is zero-padded to the fixed 31-byte field.
        let short = le_set_adv_data(&[1, 2, 3]);
        assert_eq!(short.params().len(), 32);
        assert_eq!(&short.params()[..4], &[3, 1, 2, 3]);

        assert_eq!(le_set_adv_enable(true).h4(), &[0x01, 0x0A, 0x20, 1, 1]);
    }

    #[test]
    fn extended_commands() {
        let p = le_set_ext_adv_params_nonconn(3, 3200);
        assert_eq!(p.opcode(), opcode::LE_SET_EXT_ADV_PARAMS);
        assert_eq!(p.params().len(), 25);
        assert_eq!(&p.params()[..10], &[3, 0x10, 0x00, 0x80, 0x0C, 0x00, 0x80, 0x0C, 0x00, 0x07]);
        assert_eq!(p.params()[10], 0x01);
        assert_eq!(&p.params()[19..], &[0x7F, 1, 0, 1, 0, 0]);

        let a = le_set_adv_set_random_address(3, [1, 2, 3, 4, 5, 0xC6]);
        assert_eq!(a.h4(), &[0x01, 0x35, 0x20, 7, 3, 1, 2, 3, 4, 5, 0xC6]);

        let d = le_set_ext_adv_data(3, &[0x1E; 31]);
        assert_eq!(&d.h4()[..8], &[0x01, 0x37, 0x20, 35, 3, 0x03, 0x01, 31]);
        assert_eq!(d.h4().len(), 4 + 35);

        assert_eq!(le_set_ext_adv_enable(3, true).h4(), &[0x01, 0x39, 0x20, 6, 1, 1, 3, 0, 0, 0]);
        assert_eq!(le_remove_adv_set(3).h4(), &[0x01, 0x3C, 0x20, 1, 3]);
    }

    #[test]
    fn events() {
        // Command Complete for LE Set Adv Enable, success.
        let cc = [0x04, 0x0E, 0x04, 0x01, 0x0A, 0x20, 0x00];
        let e = parse_event(&cc).unwrap();
        assert_eq!(e, Event::CommandComplete { opcode: 0x200A, status: 0, rest: &[] });
        assert_eq!(e.status_for(opcode::LE_SET_ADV_ENABLE), Some(0));
        assert_eq!(e.status_for(opcode::RESET), None);

        // Without the H4 indicator, with a return parameter (num sets = 16).
        let e = parse_event(&[0x0E, 0x05, 0x01, 0x3B, 0x20, 0x00, 0x10]).unwrap();
        assert_eq!(e, Event::CommandComplete { opcode: 0x203B, status: 0, rest: &[0x10] });

        // Command Status, disallowed.
        let e = parse_event(&[0x04, 0x0F, 0x04, 0x0C, 0x01, 0x36, 0x20]).unwrap();
        assert_eq!(e.status_for(opcode::LE_SET_EXT_ADV_PARAMS), Some(0x0C));
        assert_eq!(status_name(0x0C), "command disallowed");

        assert!(matches!(parse_event(&[0x04, 0x3E, 0x01, 0x02]), Ok(Event::Other { code: 0x3E, .. })));
        assert_eq!(parse_event(&[0x04, 0x0E, 0x09, 0x01]), Err(ParseError::Truncated));
        assert_eq!(parse_event(&[]), Err(ParseError::Truncated));
    }
}
