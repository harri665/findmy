//! `fm`: the findmy command-line tool.
//!
//! ```text
//! fm keygen my-bike --mode pool --count 96 -o bike.keys.json
//! fm blob bike.keys.json -o bike.fmkt          # provisioning blob for a beacon
//! fm show bike.keys.json                       # address + advert per key
//! fm decrypt bike.keys.json <hashed> <report>  # decrypt one fetched report
//! fm decode-adv <address> <adv data hex>       # what a finder would rebuild
//! ```

mod keyfile;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use clap::{Parser, Subcommand};
use fm_core::schedule::{KeyMode, DEFAULT_INTERVAL_S};
use fm_core::{advert, blob, keys::KeyPair, report};
use keyfile::{hex, parse_mode, unhex, KeyEntry, KeyFile};
use rand_core::OsRng;

#[derive(Parser)]
#[command(name = "fm", version, about = "findmy: keys, provisioning blobs and reports")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate keys for a new device and write a key file (contains private keys).
    Keygen {
        /// Device name.
        name: String,
        /// "static" (one key) or "pool" (rotating keys).
        #[arg(long, default_value = "pool")]
        mode: String,
        /// Number of keys in the pool (ignored for static).
        #[arg(long, default_value_t = 96)]
        count: u16,
        /// Seconds between key rotations.
        #[arg(long, default_value_t = DEFAULT_INTERVAL_S)]
        interval: u32,
        /// Status byte to advertise (battery bits 6-7).
        #[arg(long, default_value_t = 0)]
        status: u8,
        /// Output key file.
        #[arg(short, long)]
        out: String,
        /// Overwrite an existing key file.
        #[arg(long)]
        force: bool,
    },
    /// Build the FMKT provisioning blob (public keys only) for a beacon.
    Blob {
        keyfile: String,
        #[arg(short, long)]
        out: String,
    },
    /// Print the BLE address and advertising data for each key.
    Show {
        keyfile: String,
        /// Only show the first N keys.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Decrypt a report fetched from Apple.
    Decrypt {
        keyfile: String,
        /// Hashed key (base64) the report was fetched under.
        hashed: String,
        /// Report payload, base64.
        report: String,
    },
    /// Rebuild the public key from an observed advertisement, like a finder does.
    DecodeAdv {
        /// Address as shown by scanners, e.g. C9:40:C0:A1:E0:26.
        address: String,
        /// Raw advertising data, hex.
        data: String,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Keygen { name, mode, count, interval, status, out, force } => {
            keygen(name, &mode, count, interval, status, &out, force)
        }
        Cmd::Blob { keyfile, out } => write_blob(&keyfile, &out),
        Cmd::Show { keyfile, limit } => show(&keyfile, limit),
        Cmd::Decrypt { keyfile, hashed, report } => decrypt(&keyfile, &hashed, &report),
        Cmd::DecodeAdv { address, data } => decode_adv(&address, &data),
    }
}

fn keygen(name: String, mode: &str, count: u16, interval: u32, status: u8, out: &str, force: bool) -> Result<()> {
    let key_mode = parse_mode(mode)?;
    let count = match key_mode {
        KeyMode::Static => 1,
        _ => count,
    };
    if count == 0 || count > blob::MAX_KEYS {
        bail!("--count must be 1..={}", blob::MAX_KEYS);
    }
    if key_mode == KeyMode::Pool && interval == 0 {
        bail!("--interval must be at least 1 second");
    }
    if !force && std::path::Path::new(out).exists() {
        bail!("{out} already exists (use --force to overwrite; the old keys would be lost)");
    }

    let keys = (0..count).map(|_| KeyEntry::from_pair(&KeyPair::generate(&mut OsRng))).collect();
    let kf = KeyFile { name, mode: mode.to_string(), interval_s: interval, status, keys };
    kf.save(out)?;

    let cycle_h = count as f64 * interval as f64 / 3600.0;
    println!("wrote {out}: {count} key(s), mode {mode}");
    if key_mode == KeyMode::Pool {
        println!("rotates every {interval}s, full cycle {cycle_h:.1} h");
    }
    println!("this file contains PRIVATE keys; keep it safe and out of git");
    Ok(())
}

fn write_blob(keyfile: &str, out: &str) -> Result<()> {
    let kf = KeyFile::load(keyfile)?;
    let advs = kf.keys.iter().map(KeyEntry::adv_key).collect::<Result<Vec<_>>>()?;
    let bytes = blob::build(kf.key_mode()?, kf.interval_s, kf.status, &advs)?;
    std::fs::write(out, &bytes).with_context(|| format!("writing {out}"))?;
    println!("wrote {out}: {} bytes, {} key(s)", bytes.len(), advs.len());
    Ok(())
}

fn show(keyfile: &str, limit: Option<usize>) -> Result<()> {
    let kf = KeyFile::load(keyfile)?;
    println!("{} ({} mode, {} keys, interval {}s)", kf.name, kf.mode, kf.keys.len(), kf.interval_s);
    for (i, k) in kf.keys.iter().take(limit.unwrap_or(usize::MAX)).enumerate() {
        let adv = advert::encode(&k.adv_key()?, kf.status);
        println!("#{i:<4} addr {}  hashed {}", fmt_addr(&adv.address), k.hashed);
        println!("      data {}", hex(&adv.data));
    }
    Ok(())
}

fn decrypt(keyfile: &str, hashed: &str, report_b64: &str) -> Result<()> {
    let kf = KeyFile::load(keyfile)?;
    let entry = kf.by_hashed(hashed).with_context(|| format!("no key in {keyfile} has hashed id {hashed}"))?;
    let payload = B64.decode(report_b64.trim()).context("report is not valid base64")?;
    let loc = report::decrypt(&entry.pair()?, &payload)?;
    println!("time      {} (unix {})", fmt_time(loc.timestamp), loc.timestamp);
    println!("location  {:.7}, {:.7}  (±{} m)", loc.latitude, loc.longitude, loc.accuracy);
    println!("map       https://www.openstreetmap.org/?mlat={0:.7}&mlon={1:.7}#map=17/{0:.7}/{1:.7}", loc.latitude, loc.longitude);
    println!("confidence {}  status 0x{:02x}", loc.confidence, loc.status);
    Ok(())
}

fn decode_adv(address: &str, data: &str) -> Result<()> {
    let addr_bytes = unhex(&address.replace([':', '-'], ""))?;
    let addr: [u8; 6] = addr_bytes.try_into().map_err(|_| anyhow::anyhow!("address must be 6 bytes"))?;
    let decoded = advert::decode(&addr, &unhex(data)?)?;
    println!("adv key   {}", B64.encode(decoded.key));
    println!("hashed    {}", B64.encode(fm_core::keys::hashed_adv_key(&decoded.key)));
    println!("status    0x{:02x}", decoded.status);
    Ok(())
}

fn fmt_addr(a: &[u8; 6]) -> String {
    a.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// UTC timestamp without pulling in a date crate.
fn fmt_time(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", secs / 3600, secs % 3600 / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::fmt_time;

    #[test]
    fn time_formatting() {
        assert_eq!(fmt_time(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(fmt_time(1_790_000_000), "2026-09-21 14:13:20 UTC");
        assert_eq!(fmt_time(951_782_400), "2000-02-29 00:00:00 UTC");
    }
}
