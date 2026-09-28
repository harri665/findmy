# findmy

Put your own devices on Apple's Find My network over Bluetooth LE, and see
where they are on a self-hosted map.

A beacon broadcasts a public key the way an AirTag does. Nearby iPhones encrypt
their location to that key and upload it to Apple. findmy holds the private
keys, so only you can decrypt the reports.

## Layout

| Path | What |
|---|---|
| `crates/fm-core` | Protocol core (`no_std`): keys, advertisements, key rotation, provisioning blobs, report decryption. See `docs/protocol.md`. |
| `crates/fm-hci` | Bluetooth HCI commands shared by the beacons (`no_std`). |
| `crates/fm-cli` | `fm`: generate keys, build blobs, decrypt reports. |
| `crates/fm-server` | API on :3001. Devices, encrypted keys, report ingest, location history. |
| `crates/fm-anisette` | Placeholder, not implemented. See its README. |
| `client/` | Dashboard, based on [harrisonwebdefault](https://github.com/harri665/harrisonwebdefault). |
| `beacons/esp32` | ESP32-C3 beacon firmware. |
| `beacons/linux` | Linux beacon daemon (raw HCI socket). |
| `docker-compose.yml` | Dashboard (nginx) + server. |

## Run it

```sh
# Development: server on :3001 and Vite on :5173
npm run install:all
FM_ADMIN_PASSWORD=change-me npm run dev

# Docker (e.g. a Portainer stack): dashboard on :8080
FM_ADMIN_PASSWORD=change-me docker compose up -d --build
```

The server keeps its database and master key in `/data` (`FM_DATA_DIR`). Set
`FM_MASTER_KEY` (base64, 32 bytes) to keep the key that encrypts private keys
outside the data volume.

## Tests

```sh
cargo test --workspace            # protocol core, CLI, HCI, server, Linux beacon logic
(cd client && npm test)           # dashboard smoke tests
python tools/vectors/gen_vector.py   # regenerate the independent test vectors
```

The ESP32 firmware has its own workspace: `cd beacons/esp32 && cargo check`.

## Portainer

Create a stack from this repo's `docker-compose.yml` (repository or web editor)
and set `FM_ADMIN_PASSWORD` (required) and optionally `FM_MASTER_KEY` and
`FINDMY_PORT` in the stack's environment. Keep the `/data` volume: it holds the
database and, unless `FM_MASTER_KEY` is set, the key that encrypts private keys.

## Status

| Piece | State |
|---|---|
| Protocol core, CLI, HCI | Done, tested (including vectors from an independent implementation). Static, pool and derived key modes |
| Server + dashboard + Docker | Done, tested; reports come in through `POST /api/reports` |
| ESP32-C3 beacon | Builds; not yet tried on hardware. Static and pool only (no on-device derivation) |
| Fetching reports from Apple | Not part of this repo. An external fetcher reads `GET /api/keys/hashed` and pushes to `POST /api/reports` (see `crates/fm-anisette/README.md`) |
| Linux beacon | Builds and type-checks for Linux; sequence unit-tested; not yet tried on hardware (`beacons/linux`) |
| Windows, nRF52 beacons | Not built yet |

Only track things you own.
