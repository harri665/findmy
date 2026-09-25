//! findmy beacon for Linux.
//!
//! ```text
//! # keys from `fm blob` or the server (curl -H "Authorization: Device $TOKEN" $URL/api/agent/blob -o keys.fmkt)
//! sudo fm-beacon-linux --blob keys.fmkt
//! ```
//!
//! The blob file is re-read when it changes, so refreshing it with curl from a
//! timer is enough to pick up new keys.

// The daemon proper is Linux-only; other hosts just build and test the portable parts.
#![cfg_attr(not(target_os = "linux"), allow(unused))]

mod radio;
#[cfg(target_os = "linux")]
mod hci_socket;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use fm_core::advert;
use fm_core::blob::Blob;
use fm_core::schedule;

use radio::Radio;

#[derive(Parser)]
#[command(version, about = "Advertise findmy keys from a provisioning blob over a raw HCI socket")]
struct Args {
    /// Provisioning blob (from `fm blob` or the server).
    #[arg(long)]
    blob: PathBuf,
    /// HCI adapter number (0 = hci0).
    #[arg(long, default_value_t = 0)]
    hci: u16,
}

/// How often to check the blob file for changes while waiting for a rotation.
const RELOAD_CHECK: Duration = Duration::from_secs(60);

struct Loaded {
    bytes: Vec<u8>,
    mtime: Option<SystemTime>,
}

fn load(path: &Path) -> Result<Loaded, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    Blob::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Loaded { bytes, mtime })
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn run(_: &Args) -> Result<(), String> {
    Err("fm-beacon-linux only runs on Linux (raw HCI sockets)".into())
}

#[cfg(target_os = "linux")]
fn run(args: &Args) -> Result<(), String> {
    let mut loaded = load(&args.blob)?;
    let socket = hci_socket::HciSocket::open(args.hci)
        .map_err(|e| format!("opening hci{} (needs CAP_NET_RAW and the adapter up): {e}", args.hci))?;
    let mut radio = Radio::new(socket);
    hci_socket::install_stop_handler();

    let blob = Blob::parse(&loaded.bytes).expect("validated by load");
    let mut start = (hci_socket::random_u32() % blob.key_count() as u32) as u16;
    let boot = Instant::now();
    eprintln!(
        "{} key(s), mode {:?}, interval {} s, status 0x{:02x}",
        blob.key_count(),
        blob.mode,
        blob.interval_s,
        blob.status
    );

    loop {
        let blob = Blob::parse(&loaded.bytes).expect("validated by load");
        let uptime = boot.elapsed().as_secs();
        let index = schedule::key_index(blob.mode, blob.key_count(), blob.interval_s, start, uptime);

        if radio.current() != Some(index) {
            let adv = advert::encode(&blob.adv_key(index), blob.status);
            match radio.advertise(index, &adv) {
                Ok(()) => eprintln!("advertising key #{index} as {}", fmt_addr(&adv.address)),
                Err(e) => {
                    eprintln!("advertising failed: {e}");
                    radio.reset();
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            }
        }

        let wait = schedule::secs_until_next(blob.mode, blob.key_count(), blob.interval_s, uptime)
            .map(Duration::from_secs)
            .unwrap_or(RELOAD_CHECK)
            .min(RELOAD_CHECK);
        // Sleep in short slices so SIGINT / SIGTERM is noticed promptly.
        let until = Instant::now() + wait;
        while Instant::now() < until && !hci_socket::stop_requested() {
            std::thread::sleep(Duration::from_millis(500).min(until - Instant::now()));
        }
        if hci_socket::stop_requested() {
            // Closing the socket would leave the controller advertising.
            radio.stop();
            eprintln!("stopped");
            return Ok(());
        }

        if let Some(new) = reload_if_changed(&args.blob, &loaded) {
            eprintln!("reloaded {}", args.blob.display());
            loaded = new;
            // Keys may have changed, so start over from the beginning of the new schedule.
            let count = Blob::parse(&loaded.bytes).expect("validated by load").key_count();
            start = (hci_socket::random_u32() % count as u32) as u16;
            radio.reset();
        }
    }
}

/// The new blob if the file changed and still parses; a bad update keeps the old keys.
fn reload_if_changed(path: &Path, current: &Loaded) -> Option<Loaded> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if mtime == current.mtime {
        return None;
    }
    match load(path) {
        Ok(l) if l.bytes != current.bytes => Some(l),
        Ok(_) => None,
        Err(e) => {
            eprintln!("ignoring changed blob: {e}");
            None
        }
    }
}

fn fmt_addr(a: &[u8; 6]) -> String {
    a.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}
