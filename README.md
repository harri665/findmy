# findmy

findmy lets you build your own Bluetooth LE beacons and track them on Apple's
Find My network, with a self-hosted dashboard for the map and location history.

A beacon broadcasts a public key, much like an AirTag. Nearby iPhones use that
key to encrypt their location and send the report to Apple. Your findmy server
fetches those reports and decrypts them with the beacon's private keys.

## What's in the repo

| Path | Purpose |
|---|---|
| `crates/fm-core` | Keys, advertisements, key rotation, provisioning blobs, and report decryption. Supports `no_std`; see [the protocol notes](docs/protocol.md). |
| `crates/fm-hci` | Shared Bluetooth HCI commands for the beacons, also `no_std`. |
| `crates/fm-cli` | The `fm` command-line tool for generating keys, building blobs, and decrypting reports. |
| `crates/fm-server` | The API on port 3001, including device management, encrypted key storage, and location history. |
| `crates/fm-anisette` | HTTP client for an external anisette header provider. |
| `crates/fm-apple` | Fetches encrypted reports from Apple using an authenticated session. |
| `client/` | Dashboard, based on [harrisonwebdefault](https://github.com/harri665/harrisonwebdefault). |
| `beacons/esp32` | ESP32-C3 beacon firmware. |
| `beacons/linux` | Linux beacon daemon using a raw HCI socket. |
| `docker-compose.yml` | Runs the dashboard behind nginx alongside the server. |

## Getting started

For local development, you'll need Rust and Node.js with npm. From the repo
root, run:

```sh
npm run install:all
FM_ADMIN_PASSWORD=change-me npm run dev
```

This starts the API on port 3001 and the dashboard at
<http://localhost:5173>. Choose your own password in place of `change-me`.

To run it with Docker instead:

```sh
FM_ADMIN_PASSWORD=change-me docker compose up -d --build
```

The Docker site is available at <http://localhost:8080> and shows a coming soon
page by default. Set `FINDMY_PORT` to use a different port. To deploy the full
dashboard, set `VITE_COMING_SOON=false` and rebuild with `docker compose up -d
--build`. Local development continues to show the dashboard.

On Windows PowerShell, set the password with `$env:FM_ADMIN_PASSWORD = 'change-me'`
before running `npm run dev` or `docker compose up -d --build`.

With the full dashboard enabled, sign in with that password, add a device, and provision its
beacon using the [ESP32](beacons/esp32/README.md) or
[Linux](beacons/linux/README.md) instructions. Follow the
[Apple setup guide](docs/apple-setup.md) to connect your account and enable
automatic report fetching. The map refreshes every minute.

The server stores its database and master key in `FM_DATA_DIR`, which is
`/data` in Docker. You can supply a base64-encoded, 32-byte key through
`FM_MASTER_KEY` if you want to manage the encryption key separately from the
data volume. Keep that key backed up along with the database.

## Running the checks

From the repo root:

```sh
cargo test --workspace
npm test --prefix client
python tools/apple/test_login.py
```

These cover the Rust workspace, dashboard, and offline Apple login export.
To regenerate the independent protocol test vectors, run
`python tools/vectors/gen_vector.py`.

The ESP32 firmware has its own workspace. Run `cargo check` from
`beacons/esp32` to check it separately.

## Portainer

Create a stack from this repo's `docker-compose.yml` (repository or web editor)
and set `FM_ADMIN_PASSWORD` in the stack's environment. You can also set
`FM_MASTER_KEY` and `FINDMY_PORT` there. Keep the `/data` volume when updating
the stack: it holds the database and, unless you supply `FM_MASTER_KEY`, the
encryption key.

## Current state

The protocol core, CLI, and HCI code have tests, including vectors from an
independent implementation. The core supports static, pool, and derived keys.
The server and dashboard support device setup, location history, encrypted
session storage, and both scheduled and manual report fetching.

Apple report fetching is implemented and tested against local fixtures. Live
retrieval.

The ESP32-C3 firmware supports
static and pool keys, without on-device key derivation. The Linux beacon also
builds and has unit tests for its advertising sequence, but still needs a
hardware test. Windows and nRF52 beacons haven't been built yet.

Only track things you own.
