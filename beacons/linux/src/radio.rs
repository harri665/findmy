//! Putting a key on air through an HCI transport.
//!
//! Uses Bluetooth 5 extended advertising so we get our own advertising set
//! with its own random address and never touch the address bluetoothd uses.

use fm_core::advert::Advertisement;
use fm_hci::{self as hci, Command};

/// Advertising set handle we own.
const HANDLE: u8 = 0;
/// Close to what AirTags use; iPhones scan far more often than this.
const ADV_INTERVAL_MS: u32 = 2000;

pub trait Transport {
    /// Send one command and wait for its Command Complete / Command Status.
    fn exec(&mut self, cmd: Command) -> Result<(), String>;
}

pub struct Radio<T: Transport> {
    transport: T,
    configured: bool,
    on_air: Option<u16>,
}

impl<T: Transport> Radio<T> {
    pub fn new(transport: T) -> Self {
        Radio { transport, configured: false, on_air: None }
    }

    /// Key index currently advertised.
    pub fn current(&self) -> Option<u16> {
        self.on_air
    }

    /// Put `adv` on air as key `index`. The set's address can only change while
    /// it is disabled, so every rotation is disable → address → data → enable.
    pub fn advertise(&mut self, index: u16, adv: &Advertisement) -> Result<(), String> {
        if !self.configured {
            self.transport.exec(hci::le_set_ext_adv_params_nonconn(HANDLE, hci::interval_from_ms(ADV_INTERVAL_MS)))?;
            self.configured = true;
        } else if self.on_air.is_some() {
            self.transport.exec(hci::le_set_ext_adv_enable(HANDLE, false))?;
        }
        self.on_air = None;
        self.transport.exec(hci::le_set_adv_set_random_address(HANDLE, adv.address_hci()))?;
        self.transport.exec(hci::le_set_ext_adv_data(HANDLE, &adv.data))?;
        self.transport.exec(hci::le_set_ext_adv_enable(HANDLE, true))?;
        self.on_air = Some(index);
        Ok(())
    }

    /// Stop advertising and drop the set.
    pub fn stop(&mut self) {
        if self.on_air.take().is_some() {
            let _ = self.transport.exec(hci::le_set_ext_adv_enable(HANDLE, false));
        }
        if self.configured {
            let _ = self.transport.exec(hci::le_remove_adv_set(HANDLE));
            self.configured = false;
        }
    }

    /// After a transport error, force a full reconfigure on the next attempt.
    pub fn reset(&mut self) {
        self.configured = false;
        self.on_air = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::advert;
    use hci::opcode::*;

    #[derive(Default)]
    struct Mock {
        sent: Vec<(u16, Vec<u8>)>,
        fail_on: Option<u16>,
    }

    impl Transport for &mut Mock {
        fn exec(&mut self, cmd: Command) -> Result<(), String> {
            if self.fail_on == Some(cmd.opcode()) {
                return Err("boom".into());
            }
            self.sent.push((cmd.opcode(), cmd.params().to_vec()));
            Ok(())
        }
    }

    fn ops(m: &Mock) -> Vec<u16> {
        m.sent.iter().map(|(o, _)| *o).collect()
    }

    #[test]
    fn first_advertise_configures_then_rotation_disables_first() {
        let mut m = Mock::default();
        let mut radio = Radio::new(&mut m);
        let a = advert::encode(&[1; 28], 0x40);
        let b = advert::encode(&[2; 28], 0x40);

        radio.advertise(0, &a).unwrap();
        assert_eq!(radio.current(), Some(0));
        radio.advertise(1, &b).unwrap();
        assert_eq!(radio.current(), Some(1));
        drop(radio);

        assert_eq!(
            ops(&m),
            [
                LE_SET_EXT_ADV_PARAMS,
                LE_SET_ADV_SET_RANDOM_ADDRESS,
                LE_SET_EXT_ADV_DATA,
                LE_SET_EXT_ADV_ENABLE,
                LE_SET_EXT_ADV_ENABLE, // disable before changing the address
                LE_SET_ADV_SET_RANDOM_ADDRESS,
                LE_SET_EXT_ADV_DATA,
                LE_SET_EXT_ADV_ENABLE,
            ]
        );
        // The rotation's disable comes before the address change and says "off".
        assert_eq!(m.sent[4].1[0], 0, "disable");
        assert_eq!(m.sent[7].1[0], 1, "enable");
        // Address bytes sent are the second key's address in HCI order.
        assert_eq!(&m.sent[5].1[1..], &b.address_hci());
    }

    #[test]
    fn failure_leaves_nothing_on_air_and_reset_reconfigures() {
        let mut m = Mock { fail_on: Some(LE_SET_EXT_ADV_DATA), ..Default::default() };
        let a = advert::encode(&[1; 28], 0);
        {
            let mut radio = Radio::new(&mut m);
            assert!(radio.advertise(0, &a).is_err());
            assert_eq!(radio.current(), None);
            radio.reset();
        }
        m.fail_on = None;
        m.sent.clear();
        let mut radio = Radio::new(&mut m);
        radio.reset();
        radio.advertise(0, &a).unwrap();
        drop(radio);
        assert_eq!(ops(&m)[0], LE_SET_EXT_ADV_PARAMS);
    }
}
