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

## Status

| Piece | State |
|---|---|
| Protocol core, CLI, HCI | Done, tested (including vectors from an independent implementation). Static, pool and derived key modes |
| Server + dashboard + Docker | Done, tested; reports come in through `POST /api/reports` |
| ESP32-C3 beacon | Builds; not yet tried on hardware. Static and pool only (no on-device derivation) |
| Fetching reports from Apple | Not part of this repo. An external fetcher reads `GET /api/keys/hashed` and pushes to `POST /api/reports` (see `crates/fm-anisette/README.md`) |
| Linux, Windows, nRF52 beacons | Not built yet |

Only track things you own.
