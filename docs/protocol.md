# findmy protocol notes

Our own write-up of the parts of Apple's offline-finding protocol that findmy
implements, plus findmy's own formats. Code lives in `crates/fm-core`.

## 1. Keys

- Curve: NIST P-224 (secp224r1).
- Private key: 28-byte scalar, big-endian. Generated and kept on the server only.
- **Advertisement key**: the 28-byte x-coordinate of the public point.
- **Hashed key** (the report ID Apple indexes by): `SHA256(advertisement key)`,
  sent to Apple base64-encoded.

A finder only sees the x-coordinate, so it rebuilds the point with an
arbitrary y (we use SEC1 `02 || x`). That's fine: ECDH's shared secret is the
x-coordinate of `k·P`, which is identical for `P` and `-P`.

Code: `fm_core::keys`.

## 2. Advertisement

The 28-byte key is split between the BLE address and the payload:

| Where | Bytes |
|---|---|
| BLE random static address (MSB first) | `key[0] \| 0xC0`, `key[1..6]` |
| Advertising data | see below |

```text
1E        AD length (30)
FF        AD type: manufacturer specific
4C 00     company id: Apple (little-endian)
12        offline-finding type
19        offline-finding length (25)
SS        status (battery level in bits 6-7)
.. x22    key[6..28]
TT        key[0] >> 6  (the two bits the address overwrote)
00        hint
```

That's 31 bytes, the whole legacy advertising payload. There's no room for a
Flags AD structure, so beacons must advertise **non-connectable**
(`ADV_NONCONN_IND`, or the legacy non-connectable PDU through extended
advertising commands).

HCI commands take the address least-significant byte first:
`Advertisement::address_hci()`.

Code: `fm_core::advert`.

## 3. Location reports

Finders encrypt to the advertised key. Reports fetched from Apple are 88 bytes
(newer ones are 89, with an extra byte at offset 4 that we drop):

```text
offset size  field
     0    4  timestamp: seconds since 2001-01-01T00:00:00Z (u32 BE)
     4    1  confidence
     5   57  finder ephemeral public key, SEC1 uncompressed (04 || x || y)
    62   10  ciphertext
    72   16  AES-GCM tag
```

Decryption:

```text
shared = ECDH(private key, ephemeral key)                  -- 28-byte x-coordinate
k      = SHA256(shared || 00 00 00 01 || ephemeral key)    -- ANSI X9.63 KDF, 1 block
plain  = AES-128-GCM(key = k[0..16], iv = k[16..32]).open(ciphertext || tag)
```

Note that the IV is **16 bytes**, not the usual 12.

Plaintext (10 bytes):

```text
0  4  latitude,  i32 BE, units of 1e-7 degrees
4  4  longitude, i32 BE, units of 1e-7 degrees
8  1  horizontal accuracy, metres
9  1  status byte the beacon was advertising
```

Code: `fm_core::report`.

## 4. Key schedules (findmy)

Chosen per device:

| Mode | Value | Behaviour |
|---|---|---|
| Static | 0 | One key forever. |
| Pool | 1 | N keys. Index = `(start + uptime / interval) mod N`. |
| Derived | 2 | Keys derived from one seed (below). Same rotation as Pool, wrapping at the window size N. Needs P-224 on the beacon, so not ESP32. |

Beacons only need a monotonic uptime counter. The server asks Apple for
reports on every key a device owns, so it never needs to know which key was
active at which moment.

Code: `fm_core::schedule`.

### Derived keys

A 28-byte seed yields key `i` as the private scalar
`SHA256("findmy/derive/v1" || seed || i as u32 BE || ctr as u8)[..28]`, with
`ctr` starting at 0 and incremented if the value is not a valid P-224 scalar
(about 2^-96). The server stores every key of the window, so reports for any
of them can be decrypted. The seed is a secret: anyone holding it can compute
every private key. Code: `fm_core::derive`; vectors in section 6.

## 5. Provisioning blob v1 (findmy)

How every beacon receives its keys. Public keys only, except Derived mode, which carries the seed.

```text
offset size  field
     0    4  magic "FMKT"
     4    1  version = 1
     5    1  key mode (section 4)
     6    4  rotation interval, seconds (u32 LE)
    10    2  key count N (u16 LE, 1..=2048)
    12    1  status byte to advertise
    13    1  reserved = 0
    14  28M  payload: M = N advertisement keys; Derived mode: M = 1, the seed
14+28M    4  CRC-32/IEEE of all preceding bytes (u32 LE)
```

In Derived mode N is the window of derived keys the beacon cycles through, not
the number of payload entries.

Firmware can read the 14-byte header first and call `Blob::peek_len` to learn
how much more to read.

Code: `fm_core::blob`.

## 6. Test vectors

These were produced by an independent finder-side implementation in Python
(pyca/cryptography), `tools/vectors/gen_vector.py`. Owner and ephemeral scalars are
`SHA256("findmy test owner")` and `SHA256("findmy test ephemeral")`, each
reduced mod n. They're checked in `crates/fm-core/tests/vectors.rs`.

| Field | Hex |
|---|---|
| owner private key | `6bea390c3ab9532df5ace692b52d00a82b55145742c45b4e372abf3f` |
| advertisement key | `8940c0a1e026b24a5a52c6e8bb5c3981231dd6fde710677cf2d0ffd4` |
| hashed key | `9b39f0beb3f6d9994b04d2f6a1d70a9e7024245d99e20f8107c7d5772c461fd6` |
| address (status `0x40`) | `c940c0a1e026` |
| advertising data | `1eff4c00121940b24a5a52c6e8bb5c3981231dd6fde710677cf2d0ffd40200` |
| report | `306173000204394a7c1c259b57d1d63fbe71ef7937a9e15ac2b06f38fe79f7d71fb1aa5c488768cf511b199b80a53d4ba60c3dc71b8a6bfae0e6511441ee46207fb94b5aeae6c1ef5fd80eec651d009e64d54d6158ec02cd` |

Expected report contents: timestamp `1790000000` (Unix), confidence 2,
latitude `40.0149856`, longitude `-105.2705456`, accuracy 35 m, status `0x40`.

Live verification still needed: add an anonymized vector from a real Apple
report to confirm the layout against live data. The fetch path (`fm-apple`)
is implemented and tested with local HTTP fixtures.
