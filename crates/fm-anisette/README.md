# fm-anisette

HTTP client for a configured anisette provider. It validates the provider URL,
reads a bounded JSON response, and selects the Apple headers needed for report
requests. Account tokens are sent only to Apple, never to the provider.

## Provider contract

The configured URL must respond to GET with JSON containing `X-Apple-I-MD`
and `X-Apple-I-MD-M`. `X-Apple-I-MD-RINFO` is optional. Other headers are
discarded. Persistent user/device IDs come from the authenticated Apple
session; time, timezone, and locale are supplied by the client.

TLS verification remains enabled, including Apple's bundled public root CA.
Redirects are rejected. Providers are operated separately; this crate does
not generate anisette data. See [Apple setup](../../docs/apple-setup.md).

## Contents

- `src/lib.rs`: provider client and bounded HTTP transport helpers.
- `certs/apple-root-ca.cer`: Apple's public Root CA certificate
  (SHA-256 `B0:B1:73:0E:CB:C7:FF:45:05:14:2C:49:F1:29:5E:6E:DA:6B:CA:ED:7E:2C:68:C5:BE:91:B5:A1:10:01:F0:24`,
  from apple.com/certificateauthority). Some Apple endpoints chain to this
  private root, so `fm-apple` will reuse it to verify TLS instead of turning
  verification off.
