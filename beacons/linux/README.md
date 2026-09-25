# fm-beacon-linux

Advertises findmy keys from a Linux machine's Bluetooth adapter. It takes a
provisioning blob (see `docs/protocol.md`) and uses Bluetooth 5 extended
advertising to get its own advertising set, so bluetoothd's address is not touched.

```sh
cargo build --release -p fm-beacon-linux
sudo setcap cap_net_raw+ep target/release/fm-beacon-linux   # or run as root
sudo hciconfig hci0 up                                       # adapter must be up

# Keys: from the CLI...
fm blob bike.keys.json -o keys.fmkt
# ...or from the server (device page -> agent token), refreshed by cron/systemd timer:
curl -fsS -H "Authorization: Device $TOKEN" https://findmy.example.com/api/agent/blob -o keys.fmkt

fm-beacon-linux --blob keys.fmkt            # --hci 1 for hci1
```

- Static, pool and derived key modes work. The blob file is re-read when it
  changes, so a curl timer keeps keys current.
- The adapter must support extended advertising (Bluetooth 5). Most USB
  adapters from the last few years do.
- SIGINT / SIGTERM stop advertising before exiting.
- **Status: type-checked for Linux and the advertising sequence is unit-tested
  against a mock transport, but not yet run against real hardware.**
