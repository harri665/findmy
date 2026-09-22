//! SQLite storage. One connection behind a mutex: traffic is a dashboard and
//! a few beacons, so contention isn't a concern.

use std::path::Path;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS devices (
    id               INTEGER PRIMARY KEY,
    name             TEXT    NOT NULL,
    icon             TEXT    NOT NULL DEFAULT 'tag',
    platform         TEXT    NOT NULL,
    key_mode         TEXT    NOT NULL,
    interval_s       INTEGER NOT NULL,
    status           INTEGER NOT NULL DEFAULT 0,
    agent_token_hash TEXT    UNIQUE,
    created          INTEGER NOT NULL,
    seed_enc         BLOB
);
CREATE TABLE IF NOT EXISTS keys (
    device_id   INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    privkey_enc BLOB    NOT NULL,
    adv_key     BLOB    NOT NULL,
    hashed      TEXT    NOT NULL UNIQUE,
    PRIMARY KEY (device_id, idx)
);
CREATE TABLE IF NOT EXISTS reports (
    id         INTEGER PRIMARY KEY,
    device_id  INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    hashed     TEXT    NOT NULL,
    ts         INTEGER NOT NULL,
    lat        REAL    NOT NULL,
    lon        REAL    NOT NULL,
    accuracy   INTEGER NOT NULL,
    confidence INTEGER NOT NULL,
    status     INTEGER NOT NULL,
    received   INTEGER NOT NULL,
    UNIQUE (hashed, ts, lat, lon)
);
CREATE INDEX IF NOT EXISTS reports_device_ts ON reports(device_id, ts);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT    PRIMARY KEY,
    expires    INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    init(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    init(Connection::open_in_memory()?)
}

fn init(conn: Connection) -> Result<Connection> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(SCHEMA)?;
    // Databases created before derived mode lack the seed column.
    let has_seed = conn
        .prepare("SELECT 1 FROM pragma_table_info('devices') WHERE name = 'seed_enc'")?
        .exists([])?;
    if !has_seed {
        conn.execute("ALTER TABLE devices ADD COLUMN seed_enc BLOB", [])?;
    }
    Ok(conn)
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// --- Devices ----------------------------------------------------------------

pub struct NewDevice<'a> {
    pub name: &'a str,
    pub icon: &'a str,
    pub platform: &'a str,
    pub key_mode: &'a str,
    pub interval_s: u32,
    pub status: u8,
    pub agent_token_hash: &'a str,
    /// Derived mode: the sealed seed.
    pub seed_enc: Option<&'a [u8]>,
}

pub struct NewKey {
    pub privkey_enc: Vec<u8>,
    pub adv_key: [u8; 28],
    pub hashed: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Location {
    pub ts: i64,
    pub lat: f64,
    pub lon: f64,
    pub accuracy: u8,
    pub confidence: u8,
    pub status: u8,
}

#[derive(Serialize, Debug)]
pub struct Device {
    pub id: i64,
    pub name: String,
    pub icon: String,
    pub platform: String,
    pub key_mode: String,
    pub interval_s: u32,
    pub status: u8,
    pub created: i64,
    pub key_count: u32,
    pub report_count: u32,
    pub last: Option<Location>,
}

/// Insert a device and its keys atomically. Fails if any hashed key already
/// belongs to another device.
pub fn insert_device(conn: &mut Connection, d: &NewDevice<'_>, keys: &[NewKey]) -> Result<i64> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO devices (name, icon, platform, key_mode, interval_s, status, agent_token_hash, created, seed_enc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![d.name, d.icon, d.platform, d.key_mode, d.interval_s, d.status, d.agent_token_hash, now(), d.seed_enc],
    )?;
    let id = tx.last_insert_rowid();
    {
        let mut stmt =
            tx.prepare("INSERT INTO keys (device_id, idx, privkey_enc, adv_key, hashed) VALUES (?1, ?2, ?3, ?4, ?5)")?;
        for (i, k) in keys.iter().enumerate() {
            stmt.execute(params![id, i as i64, k.privkey_enc, &k.adv_key[..], k.hashed])?;
        }
    }
    tx.commit()?;
    Ok(id)
}

const DEVICE_SELECT: &str = "
SELECT d.id, d.name, d.icon, d.platform, d.key_mode, d.interval_s, d.status, d.created,
       (SELECT COUNT(*) FROM keys k WHERE k.device_id = d.id),
       (SELECT COUNT(*) FROM reports r WHERE r.device_id = d.id)
FROM devices d";

fn device_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Device> {
    Ok(Device {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        platform: row.get(3)?,
        key_mode: row.get(4)?,
        interval_s: row.get(5)?,
        status: row.get(6)?,
        created: row.get(7)?,
        key_count: row.get(8)?,
        report_count: row.get(9)?,
        last: None,
    })
}

pub fn list_devices(conn: &Connection) -> Result<Vec<Device>> {
    let mut stmt = conn.prepare(&format!("{DEVICE_SELECT} ORDER BY d.name COLLATE NOCASE"))?;
    let mut devices = stmt.query_map([], device_from_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    for d in &mut devices {
        d.last = last_location(conn, d.id)?;
    }
    Ok(devices)
}

pub fn get_device(conn: &Connection, id: i64) -> Result<Option<Device>> {
    let mut d = conn.query_row(&format!("{DEVICE_SELECT} WHERE d.id = ?1"), [id], device_from_row).optional()?;
    if let Some(d) = &mut d {
        d.last = last_location(conn, d.id)?;
    }
    Ok(d)
}

pub fn update_device(conn: &Connection, id: i64, name: Option<&str>, icon: Option<&str>) -> Result<bool> {
    let n = conn.execute(
        "UPDATE devices SET name = COALESCE(?2, name), icon = COALESCE(?3, icon) WHERE id = ?1",
        params![id, name, icon],
    )?;
    Ok(n > 0)
}

pub fn delete_device(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM devices WHERE id = ?1", [id])? > 0)
}

pub fn set_agent_token(conn: &Connection, id: i64, token_hash: &str) -> Result<bool> {
    Ok(conn.execute("UPDATE devices SET agent_token_hash = ?2 WHERE id = ?1", params![id, token_hash])? > 0)
}

pub fn device_by_agent_token(conn: &Connection, token_hash: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT id FROM devices WHERE agent_token_hash = ?1", [token_hash], |r| r.get(0))
        .optional()?)
}

/// Public advertisement keys in index order, for building a blob.
pub fn adv_keys(conn: &Connection, device_id: i64) -> Result<Vec<[u8; 28]>> {
    let mut stmt = conn.prepare("SELECT adv_key FROM keys WHERE device_id = ?1 ORDER BY idx")?;
    let rows = stmt.query_map([device_id], |r| r.get::<_, Vec<u8>>(0))?;
    let mut out = Vec::new();
    for blob in rows {
        out.push(blob?.try_into().map_err(|_| anyhow::anyhow!("corrupt adv key"))?);
    }
    Ok(out)
}

/// Blob parameters: (key_mode, interval_s, status).
pub fn blob_params(conn: &Connection, device_id: i64) -> Result<Option<(String, u32, u8)>> {
    Ok(conn
        .query_row("SELECT key_mode, interval_s, status FROM devices WHERE id = ?1", [device_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?)
}

/// The sealed seed of a derived-mode device.
pub fn seed_enc(conn: &Connection, device_id: i64) -> Result<Option<Vec<u8>>> {
    Ok(conn
        .query_row("SELECT seed_enc FROM devices WHERE id = ?1", [device_id], |r| r.get(0))
        .optional()?
        .flatten())
}

/// Every hashed key with its device, for fetching reports from Apple.
pub fn all_hashed_keys(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare("SELECT device_id, hashed FROM keys ORDER BY device_id, idx")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The encrypted private key and device for a hashed key.
pub fn key_by_hashed(conn: &Connection, hashed: &str) -> Result<Option<(i64, Vec<u8>)>> {
    Ok(conn
        .query_row("SELECT device_id, privkey_enc FROM keys WHERE hashed = ?1", [hashed], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?)
}

// --- Reports ----------------------------------------------------------------

/// Store a decrypted report. Returns false if it was already stored.
pub fn insert_report(conn: &Connection, device_id: i64, hashed: &str, loc: &Location) -> Result<bool> {
    let n = conn.execute(
        "INSERT OR IGNORE INTO reports (device_id, hashed, ts, lat, lon, accuracy, confidence, status, received)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![device_id, hashed, loc.ts, loc.lat, loc.lon, loc.accuracy, loc.confidence, loc.status, now()],
    )?;
    Ok(n > 0)
}

fn location_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Location> {
    Ok(Location {
        ts: r.get(0)?,
        lat: r.get(1)?,
        lon: r.get(2)?,
        accuracy: r.get(3)?,
        confidence: r.get(4)?,
        status: r.get(5)?,
    })
}

pub fn last_location(conn: &Connection, device_id: i64) -> Result<Option<Location>> {
    Ok(conn
        .query_row(
            "SELECT ts, lat, lon, accuracy, confidence, status FROM reports
             WHERE device_id = ?1 ORDER BY ts DESC LIMIT 1",
            [device_id],
            location_from_row,
        )
        .optional()?)
}

/// Locations since `since` (Unix seconds), oldest first, at most `limit`
/// (the most recent ones if there are more).
pub fn locations(conn: &Connection, device_id: i64, since: i64, limit: u32) -> Result<Vec<Location>> {
    let mut stmt = conn.prepare(
        "SELECT ts, lat, lon, accuracy, confidence, status FROM (
             SELECT * FROM reports WHERE device_id = ?1 AND ts >= ?2 ORDER BY ts DESC LIMIT ?3
         ) ORDER BY ts ASC",
    )?;
    let rows = stmt.query_map(params![device_id, since, limit], location_from_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// --- Sessions and settings --------------------------------------------------------

pub fn insert_session(conn: &Connection, token_hash: &str, expires: i64) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE expires < ?1", [now()])?;
    conn.execute("INSERT INTO sessions (token_hash, expires) VALUES (?1, ?2)", params![token_hash, expires])?;
    Ok(())
}

pub fn session_valid(conn: &Connection, token_hash: &str) -> Result<bool> {
    let expires: Option<i64> = conn
        .query_row("SELECT expires FROM sessions WHERE token_hash = ?1", [token_hash], |r| r.get(0))
        .optional()?;
    Ok(expires.is_some_and(|e| e > now()))
}

pub fn delete_session(conn: &Connection, token_hash: &str) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE token_hash = ?1", [token_hash])?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(v) => conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, v],
        )?,
        None => conn.execute("DELETE FROM settings WHERE key = ?1", [key])?,
    };
    Ok(())
}
