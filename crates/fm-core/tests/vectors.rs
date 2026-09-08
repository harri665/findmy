//! Fixed vectors produced by an independent finder-side implementation
//! (Python, pyca/cryptography). See docs/protocol.md, "Test vectors".

use fm_core::advert;
use fm_core::keys::{hashed_adv_key, KeyPair};
use fm_core::report;

const OWNER_PRIV: &str = "6bea390c3ab9532df5ace692b52d00a82b55145742c45b4e372abf3f";
const ADV_KEY: &str = "8940c0a1e026b24a5a52c6e8bb5c3981231dd6fde710677cf2d0ffd4";
const HASHED: &str = "9b39f0beb3f6d9994b04d2f6a1d70a9e7024245d99e20f8107c7d5772c461fd6";
const ADDRESS: &str = "c940c0a1e026";
const ADV_DATA: &str = "1eff4c00121940b24a5a52c6e8bb5c3981231dd6fde710677cf2d0ffd40200";
const REPORT: &str = "306173000204394a7c1c259b57d1d63fbe71ef7937a9e15ac2b06f38fe79f7d71fb1aa5c488768cf511b199b80a53d4ba60c3dc71b8a6bfae0e6511441ee46207fb94b5aeae6c1ef5fd80eec651d009e64d54d6158ec02cd";

fn hex<const N: usize>(s: &str) -> [u8; N] {
    let v = hex_vec(s);
    v.try_into().expect("vector length")
}

fn hex_vec(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn key_derivation() {
    let kp = KeyPair::from_private_bytes(&hex(OWNER_PRIV)).unwrap();
    assert_eq!(kp.adv_key(), hex::<28>(ADV_KEY));
    assert_eq!(kp.hashed_adv_key(), hex::<32>(HASHED));
    assert_eq!(hashed_adv_key(&hex(ADV_KEY)), hex::<32>(HASHED));
}

#[test]
fn advertisement() {
    let adv = advert::encode(&hex(ADV_KEY), 0x40);
    assert_eq!(adv.address, hex::<6>(ADDRESS));
    assert_eq!(adv.data, hex::<31>(ADV_DATA));
    let decoded = advert::decode(&hex(ADDRESS), &hex_vec(ADV_DATA)).unwrap();
    assert_eq!(decoded.key, hex::<28>(ADV_KEY));
    assert_eq!(decoded.status, 0x40);
}

#[test]
fn report_decryption() {
    let kp = KeyPair::from_private_bytes(&hex(OWNER_PRIV)).unwrap();
    let loc = report::decrypt(&kp, &hex_vec(REPORT)).unwrap();
    assert_eq!(loc.timestamp, 1_790_000_000);
    assert_eq!(loc.confidence, 2);
    assert_eq!(loc.latitude, 40.0149856);
    assert_eq!(loc.longitude, -105.2705456);
    assert_eq!(loc.accuracy, 35);
    assert_eq!(loc.status, 0x40);
}
