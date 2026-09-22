//! Core of findmy: everything about Apple's offline-finding protocol that is
//! shared between the beacons, the server and the CLI.
//!
//! - [`advert`]: turn a 28-byte public key into a BLE address + advertisement and back.
//! - [`schedule`]: which key a beacon should broadcast right now.
//! - [`blob`]: the provisioning format every beacon reads its keys from.
//! - [`keys`] / [`derive`] / [`report`] (feature `crypto`): key generation, seed-derived keys and report decryption.
//!
//! See `docs/protocol.md` for the wire formats.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod advert;
pub mod blob;
mod crc32;
pub mod schedule;

#[cfg(feature = "crypto")]
pub mod derive;
#[cfg(feature = "crypto")]
pub mod keys;
#[cfg(feature = "crypto")]
pub mod report;

/// Length of an advertised public key: the x-coordinate of a P-224 point.
pub const KEY_LEN: usize = 28;

/// The 28-byte public key a beacon advertises.
pub type AdvKey = [u8; KEY_LEN];

/// Seconds between the Unix epoch and Apple's reference date (2001-01-01T00:00:00Z).
pub const APPLE_EPOCH_OFFSET: i64 = 978_307_200;
