# findmy beacon: ESP32-C3

Bare-metal Rust firmware (`esp-hal` + `esp-radio`, no ESP-IDF). It reads a key
blob from flash and broadcasts Find My offline-finding advertisements,
rotating keys on the blob's schedule. It talks to the BLE controller with raw
HCI commands from `fm-hci`, since a beacon needs no BLE host stack.

## One-time setup

```sh
rustup target add riscv32imc-unknown-none-elf
cargo install espflash --locked
```

## Build, flash, provision

```sh
# 1. Make keys for the device (from the repo root). Keep the .keys.json safe.
cargo run -p fm-cli -- keygen my-tag --mode pool --count 96 -o my-tag.keys.json
cargo run -p fm-cli -- blob my-tag.keys.json -o my-tag.fmkt

# 2. Flash the firmware with its partition table (from beacons/esp32).
cargo run --release            # = espflash flash --monitor --partition-table partitions.csv

# 3. Write the keys into the fmkeys partition and reset.
espflash write-bin --chip esp32c3 0x210000 ../../my-tag.fmkt
```

Re-flashing the firmware leaves the `fmkeys` partition alone, so keys survive
updates. To give the board new keys, write a new blob to `0x210000`.

## What you should see

Serial log:

```text
findmy beacon 0.1.0
96 key(s), mode Pool, interval 900 s, status 0x00
advertising key #37 as C9:40:C0:A1:E0:26
```

In a BLE scanner (e.g. nRF Connect), a non-connectable device with that
address advertising manufacturer data `4C 00 12 19 ...`. To check it against
the key file:

```sh
cargo run -p fm-cli -- show my-tag.keys.json      # address + data per key
cargo run -p fm-cli -- decode-adv <address> <raw data hex>
```

## Behaviour

| | |
|---|---|
| Advertising | `ADV_NONCONN_IND`, 2 s interval, all 3 channels, random static address |
| Rotation | `fm-core::schedule`. Pool mode starts at a random key each boot, so reboots don't replay key 0. |
| No / bad blob | Never advertises; logs the provisioning command every 10 s. |
| Radio error | Resets the controller and retries after 5 s. |

## Layout

| Partition | Offset | Size | Contents |
|---|---|---|---|
| factory | `0x10000` | 2 MB | firmware |
| fmkeys | `0x210000` | 64 KB | key blob (`KEYS_OFFSET` in `src/main.rs`) |

## Not yet done

- **Not run on hardware yet.** It builds and the image validates, but the
  first flash still needs a check that the advert appears in a scanner.
- Power saving (light sleep between adverts) for battery tags.
- Battery level in the status byte (it's currently fixed by the blob).
