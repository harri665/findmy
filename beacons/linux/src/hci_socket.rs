//! Raw HCI socket (Linux only): talk to a Bluetooth controller directly.
//!
//! Needs `CAP_NET_RAW` (run as root, or `setcap cap_net_raw+ep` on the binary)
//! and an adapter that is up (`hciconfig hci0 up` or `btmgmt`).

use std::io;
use std::time::{Duration, Instant};

use fm_hci::{self as hci, Command};

use crate::radio::Transport;

const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_HCI: libc::c_int = 1;
const SOL_HCI: libc::c_int = 0;
const HCI_FILTER: libc::c_int = 2;
const HCI_CHANNEL_RAW: u16 = 0;
const CMD_TIMEOUT: Duration = Duration::from_secs(2);

#[repr(C)]
struct SockaddrHci {
    family: libc::sa_family_t,
    dev: u16,
    channel: u16,
}

#[repr(C)]
struct HciFilter {
    type_mask: u32,
    event_mask: [u32; 2],
    opcode: u16,
}

pub struct HciSocket {
    fd: libc::c_int,
}

impl HciSocket {
    pub fn open(dev: u16) -> io::Result<Self> {
        // SAFETY: plain syscalls with correctly sized, initialised arguments;
        // the fd is closed in Drop (or right here on error).
        unsafe {
            let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_RAW | libc::SOCK_CLOEXEC, BTPROTO_HCI);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let sock = HciSocket { fd };

            // Only deliver Command Complete (0x0E) and Command Status (0x0F) events.
            let filter = HciFilter {
                type_mask: 1 << hci::H4_EVENT,
                event_mask: [(1 << 0x0E) | (1 << 0x0F), 0],
                opcode: 0,
            };
            if libc::setsockopt(
                fd,
                SOL_HCI,
                HCI_FILTER,
                &filter as *const _ as *const libc::c_void,
                std::mem::size_of::<HciFilter>() as libc::socklen_t,
            ) < 0
            {
                return Err(io::Error::last_os_error());
            }

            let addr = SockaddrHci { family: AF_BLUETOOTH as libc::sa_family_t, dev, channel: HCI_CHANNEL_RAW };
            if libc::bind(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<SockaddrHci>() as libc::socklen_t,
            ) < 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(sock)
        }
    }

    fn wait_readable(&self, deadline: Instant) -> io::Result<bool> {
        let left = deadline.saturating_duration_since(Instant::now());
        let mut pfd = libc::pollfd { fd: self.fd, events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd.
        let n = unsafe { libc::poll(&mut pfd, 1, left.as_millis().min(i32::MAX as u128) as i32) };
        match n {
            n if n < 0 => Err(io::Error::last_os_error()),
            0 => Ok(false),
            _ => Ok(true),
        }
    }
}

impl Transport for HciSocket {
    fn exec(&mut self, cmd: Command) -> Result<(), String> {
        let packet = cmd.h4();
        // SAFETY: writing a valid buffer of the stated length.
        let n = unsafe { libc::write(self.fd, packet.as_ptr() as *const libc::c_void, packet.len()) };
        if n != packet.len() as isize {
            return Err(format!("writing HCI command {:#06x}: {}", cmd.opcode(), io::Error::last_os_error()));
        }

        let deadline = Instant::now() + CMD_TIMEOUT;
        let mut buf = [0u8; 260];
        loop {
            if !self.wait_readable(deadline).map_err(|e| e.to_string())? {
                return Err(format!("timed out waiting for HCI command {:#06x}", cmd.opcode()));
            }
            // SAFETY: reading into a valid buffer of the stated length.
            let n = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n < 0 {
                return Err(format!("reading HCI event: {}", io::Error::last_os_error()));
            }
            let status = hci::parse_event(&buf[..n as usize]).ok().and_then(|e| e.status_for(cmd.opcode()));
            if let Some(status) = status {
                return if status == hci::STATUS_SUCCESS {
                    Ok(())
                } else {
                    Err(format!("HCI command {:#06x} failed: {}", cmd.opcode(), hci::status_name(status)))
                };
            }
        }
    }
}

impl Drop for HciSocket {
    fn drop(&mut self) {
        // SAFETY: we own the fd.
        unsafe { libc::close(self.fd) };
    }
}

/// Random start index so a restart doesn't always replay key 0.
pub fn random_u32() -> u32 {
    let mut v = [0u8; 4];
    // SAFETY: getrandom into a valid 4-byte buffer.
    let n = unsafe { libc::getrandom(v.as_mut_ptr() as *mut libc::c_void, v.len(), 0) };
    if n == 4 {
        u32::from_le_bytes(v)
    } else {
        0
    }
}

static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Ask to be told (via [`stop_requested`]) on SIGINT / SIGTERM.
pub fn install_stop_handler() {
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
    }
}

pub fn stop_requested() -> bool {
    STOP.load(std::sync::atomic::Ordering::SeqCst)
}
