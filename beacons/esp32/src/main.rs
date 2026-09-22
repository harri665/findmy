//! findmy beacon for ESP32-C3.
//!
//! Reads its key blob (fm-core `blob` format) from the `fmkeys` flash
//! partition, then broadcasts Find My offline-finding advertisements, rotating
//! keys on the blob's schedule. Talks to the BLE controller with raw HCI
//! commands from fm-hci; no BLE host stack is needed for a beacon.
//!
//! Provision with:
//! ```text
//! fm blob device.keys.json -o device.fmkt
//! espflash write-bin --chip esp32c3 0x210000 device.fmkt
//! ```

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::ble::controller::BleConnector;
use esp_storage::FlashStorage;
use fm_core::advert;
use fm_core::blob::{Blob, BlobError, HEADER_LEN};
use fm_core::schedule::{self, KeyMode};
use fm_hci::{self as hci, Command};
use log::{error, info};

esp_bootloader_esp_idf::esp_app_desc!();

/// Start of the `fmkeys` partition. Must match partitions.csv.
const KEYS_OFFSET: u32 = 0x21_0000;
/// Size of the `fmkeys` partition.
const KEYS_SIZE: usize = 0x1_0000;
/// How often finders get a chance to see us. Apple's own tags use about 2 s.
const ADV_INTERVAL_MS: u32 = 2000;
/// How long to wait for the controller to answer a command.
const CMD_TIMEOUT: Duration = Duration::from_millis(1000);
/// Static keys never rotate; still wake up occasionally to re-check the radio.
const STATIC_RECHECK_S: u64 = 3600;

#[allow(clippy::large_stack_frames, reason = "main owns the radio and buffers for the program's lifetime")]
#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 66320);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    info!("findmy beacon {}", env!("CARGO_PKG_VERSION"));

    let mut flash = FlashStorage::new(peripherals.FLASH);
    let blob_bytes = match read_blob(&mut flash) {
        Ok(bytes) => bytes,
        Err(e) => not_provisioned(e).await,
    };
    let blob = Blob::parse(&blob_bytes).expect("validated by read_blob");
    let count = blob.key_count();
    info!(
        "{} key(s), mode {:?}, interval {} s, status 0x{:02x}",
        count, blob.mode, blob.interval_s, blob.status
    );
    // Start at a random key so a reboot doesn't always replay key 0, which
    // would let someone link the device across reboots.
    let start = (Rng::new().random() % count as u32) as u16;

    let connector = BleConnector::new(peripherals.BT, Default::default()).expect("BLE init");
    let mut radio = Radio::new(connector);
    let boot = Instant::now();

    loop {
        let uptime = boot.elapsed().as_secs();
        let index = schedule::key_index(blob.mode, count, blob.interval_s, start, uptime);

        if radio.current != Some(index) {
            let adv = advert::encode(&blob.key(index).expect("stored keys only"), blob.status);
            match radio.advertise(index, &adv).await {
                Ok(()) => info!("advertising key #{} as {}", index, Mac(&adv.address)),
                Err(e) => {
                    error!("radio: {}; retrying in 5 s", e);
                    radio.current = None;
                    Timer::after(Duration::from_secs(5)).await;
                    continue;
                }
            }
        }

        let wait = schedule::secs_until_next(blob.mode, count, blob.interval_s, uptime).unwrap_or(STATIC_RECHECK_S);
        Timer::after(Duration::from_secs(wait)).await;
    }
}

/// Read and validate the key blob from the `fmkeys` partition.
fn read_blob(flash: &mut FlashStorage<'_>) -> Result<Vec<u8>, BlobError> {
    let mut header = [0u8; HEADER_LEN];
    flash.read(KEYS_OFFSET, &mut header).map_err(|_| BlobError::TooShort)?;
    let len = Blob::peek_len(&header)?;
    if len > KEYS_SIZE {
        return Err(BlobError::TooShort);
    }
    let mut bytes = vec![0u8; len];
    flash.read(KEYS_OFFSET, &mut bytes).map_err(|_| BlobError::TooShort)?;
    // Derived blobs hold a seed and need P-224 on the device; this firmware
    // is built without fm-core's crypto feature, so it only takes stored keys.
    if Blob::parse(&bytes)?.mode == KeyMode::Derived {
        return Err(BlobError::UnknownKeyMode(KeyMode::Derived as u8));
    }
    Ok(bytes)
}

/// No usable keys: say so periodically and never advertise.
async fn not_provisioned(e: BlobError) -> ! {
    loop {
        error!("no valid key blob at 0x{:x}: {}", KEYS_OFFSET, e);
        error!("provision with: espflash write-bin --chip esp32c3 0x{:x} <device>.fmkt", KEYS_OFFSET);
        Timer::after(Duration::from_secs(10)).await;
    }
}

/// The BLE controller, driven with raw HCI commands.
struct Radio<'d> {
    connector: BleConnector<'d>,
    /// Key index currently on air, `None` if advertising isn't set up.
    current: Option<u16>,
    configured: bool,
    rx: [u8; 260],
}

#[derive(Debug)]
enum RadioError {
    Status { opcode: u16, status: u8 },
    Timeout { opcode: u16 },
    Write,
}

impl core::fmt::Display for RadioError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            RadioError::Status { opcode, status } => {
                write!(f, "command 0x{opcode:04x} failed: 0x{status:02x} ({})", hci::status_name(status))
            }
            RadioError::Timeout { opcode } => write!(f, "command 0x{opcode:04x} timed out"),
            RadioError::Write => f.write_str("HCI write failed"),
        }
    }
}

impl<'d> Radio<'d> {
    fn new(connector: BleConnector<'d>) -> Self {
        Radio { connector, current: None, configured: false, rx: [0; 260] }
    }

    /// Put `adv` on air. The address can only change while advertising is
    /// off, so every rotation is disable → address → data → enable.
    async fn advertise(&mut self, index: u16, adv: &advert::Advertisement) -> Result<(), RadioError> {
        if !self.configured {
            self.exec(hci::reset()).await?;
            self.exec(hci::le_set_adv_params_nonconn(hci::interval_from_ms(ADV_INTERVAL_MS) as u16)).await?;
            self.configured = true;
        } else if self.current.is_some() {
            self.exec(hci::le_set_adv_enable(false)).await?;
        }
        self.current = None;
        self.exec(hci::le_set_random_address(adv.address_hci())).await?;
        self.exec(hci::le_set_adv_data(&adv.data)).await?;
        self.exec(hci::le_set_adv_enable(true)).await?;
        self.current = Some(index);
        Ok(())
    }

    /// Send a command and wait for its Command Complete / Command Status.
    async fn exec(&mut self, cmd: Command) -> Result<(), RadioError> {
        let opcode = cmd.opcode();
        let mut sent = 0;
        let packet = cmd.h4();
        while sent < packet.len() {
            match self.connector.write_async(&packet[sent..]).await {
                Ok(0) | Err(_) => {
                    self.configured = false;
                    return Err(RadioError::Write);
                }
                Ok(n) => sent += n,
            }
        }

        let deadline = Instant::now() + CMD_TIMEOUT;
        while Instant::now() < deadline {
            let n = self.connector.next(&mut self.rx).unwrap_or(0);
            if n == 0 {
                Timer::after(Duration::from_millis(2)).await;
                continue;
            }
            // Other events (there shouldn't be any for a beacon) are skipped.
            if let Ok(event) = hci::parse_event(&self.rx[..n])
                && let Some(status) = event.status_for(opcode)
            {
                if status != hci::STATUS_SUCCESS {
                    return Err(RadioError::Status { opcode, status });
                }
                return Ok(());
            }
        }
        self.configured = false;
        Err(RadioError::Timeout { opcode })
    }
}

/// Formats an address MSB-first, as scanner apps show it.
struct Mac<'a>(&'a [u8; 6]);

impl core::fmt::Display for Mac<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let a = self.0;
        write!(f, "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", a[0], a[1], a[2], a[3], a[4], a[5])
    }
}
