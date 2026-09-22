//! Which key a beacon broadcasts at a given moment.
//!
//! Beacons only need a monotonic uptime counter, not a real clock: the server
//! asks Apple for reports on *every* key a device owns, so it never has to
//! know which key was active when.

/// How a device's keys are used. Stored in the provisioning blob as a `u8`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyMode {
    /// One key forever. Simplest, but anyone who records the beacon can follow it.
    Static = 0,
    /// A pool of keys, switched every `interval_s` seconds and wrapping around.
    Pool = 1,
    /// Keys derived from one seed (see `derive`), switched every `interval_s`
    /// seconds and wrapping at the blob's window size. The beacon needs crypto support.
    Derived = 2,
}

impl KeyMode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(KeyMode::Static),
            1 => Some(KeyMode::Pool),
            2 => Some(KeyMode::Derived),
            _ => None,
        }
    }
}

/// Default rotation interval: 15 minutes, close to what Apple's own accessories use.
pub const DEFAULT_INTERVAL_S: u32 = 15 * 60;

/// Index of the key to broadcast after `uptime_s` seconds.
///
/// `start_index` lets a beacon resume where it left off after a reboot (it can
/// persist the last index) so it does not restart from key 0 every time.
pub fn key_index(mode: KeyMode, key_count: u16, interval_s: u32, start_index: u16, uptime_s: u64) -> u16 {
    if key_count == 0 {
        return 0;
    }
    match mode {
        KeyMode::Static => 0,
        KeyMode::Pool | KeyMode::Derived => {
            let interval = interval_s.max(1) as u64;
            let steps = uptime_s / interval;
            ((start_index as u64 + steps) % key_count as u64) as u16
        }
    }
}

/// Seconds until the next rotation, so firmware can sleep exactly that long.
/// `None` means the key never changes.
pub fn secs_until_next(mode: KeyMode, key_count: u16, interval_s: u32, uptime_s: u64) -> Option<u64> {
    match mode {
        KeyMode::Pool | KeyMode::Derived if key_count > 1 => {
            let interval = interval_s.max(1) as u64;
            Some(interval - uptime_s % interval)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_never_moves() {
        assert_eq!(key_index(KeyMode::Static, 10, 60, 0, 0), 0);
        assert_eq!(key_index(KeyMode::Static, 10, 60, 0, 1_000_000), 0);
        assert_eq!(secs_until_next(KeyMode::Static, 10, 60, 5), None);
    }

    #[test]
    fn pool_rotates_and_wraps() {
        let idx = |t| key_index(KeyMode::Pool, 4, 900, 0, t);
        assert_eq!(idx(0), 0);
        assert_eq!(idx(899), 0);
        assert_eq!(idx(900), 1);
        assert_eq!(idx(900 * 3), 3);
        assert_eq!(idx(900 * 4), 0);
        assert_eq!(key_index(KeyMode::Pool, 4, 900, 3, 900), 0, "resumes from start_index");
        assert_eq!(secs_until_next(KeyMode::Pool, 4, 900, 100), Some(800));
    }

    #[test]
    fn derived_rotates_like_pool() {
        for t in [0, 899, 900, 2700, 3600] {
            assert_eq!(
                key_index(KeyMode::Derived, 4, 900, 0, t),
                key_index(KeyMode::Pool, 4, 900, 0, t)
            );
        }
        assert_eq!(secs_until_next(KeyMode::Derived, 4, 900, 100), Some(800));
    }

    #[test]
    fn degenerate_inputs() {
        assert_eq!(key_index(KeyMode::Pool, 0, 900, 0, 5000), 0);
        assert_eq!(key_index(KeyMode::Pool, 3, 0, 0, 5), 2, "interval 0 treated as 1s");
        assert_eq!(secs_until_next(KeyMode::Pool, 1, 900, 5), None, "single-key pool never rotates");
    }
}
