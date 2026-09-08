"""Independent finder-side implementation (pyca/cryptography) that produces the
fixed test vector in docs/protocol.md section 6. Deterministic: fixed owner and
ephemeral scalars. Run: pip install cryptography && python gen_vector.py"""
import hashlib, struct
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

N = 0xffffffffffffffffffffffffffff16a2e0b8f03e13dd29455c5c2a3d

def scalar(label):
    return int.from_bytes(hashlib.sha256(label).digest(), "big") % N

owner = ec.derive_private_key(scalar(b"findmy test owner"), ec.SECP224R1())
eph = ec.derive_private_key(scalar(b"findmy test ephemeral"), ec.SECP224R1())

owner_priv = owner.private_numbers().private_value.to_bytes(28, "big")
adv_key = owner.public_key().public_numbers().x.to_bytes(28, "big")

# Finder: rebuild point from x with even y (02 prefix), ECDH with ephemeral key.
target = ec.EllipticCurvePublicKey.from_encoded_point(ec.SECP224R1(), b"\x02" + adv_key)
shared = eph.exchange(ec.ECDH(), target)
eph_bytes = eph.public_key().public_bytes(serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint)
k = hashlib.sha256(shared + b"\x00\x00\x00\x01" + eph_bytes).digest()

lat, lon, acc, status = 40.0149856, -105.2705456, 35, 0x40
plain = struct.pack(">ii", round(lat * 1e7), round(lon * 1e7)) + bytes([acc, status])
# Apple uses a 16-byte GCM IV, so use the low-level Cipher API (AESGCM wants 12).
enc = Cipher(algorithms.AES(k[:16]), modes.GCM(k[16:])).encryptor()
ct = enc.update(plain) + enc.finalize()
tag = enc.tag

unix_ts = 1_790_000_000
apple_ts = unix_ts - 978_307_200
report = struct.pack(">I", apple_ts) + bytes([2]) + eph_bytes + ct + tag
assert len(report) == 88

addr = bytearray(adv_key[:6]); addr[0] |= 0xC0
adv = bytes([0x1E, 0xFF, 0x4C, 0x00, 0x12, 0x19, 0x40]) + adv_key[6:] + bytes([adv_key[0] >> 6, 0])

print("owner_priv", owner_priv.hex())
print("adv_key   ", adv_key.hex())
print("hashed    ", hashlib.sha256(adv_key).hexdigest())
print("address   ", bytes(addr).hex())
print("adv_data  ", adv.hex())
print("report    ", report.hex())
