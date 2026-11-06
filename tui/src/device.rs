//! What the AirPods last reported, built from AAP packets.

use podatui_core::aap::{self, Battery, EarStatus};

#[derive(Default)]
pub struct Device {
    /// BlueZ name while the AAP link is up, `None` when disconnected.
    pub name: Option<String>,
    pub addr: String,
    pub battery: Option<Battery>,
    pub ear: Option<(EarStatus, EarStatus)>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    controls: Vec<Option<u8>>,
}

impl Device {
    pub fn connected(&self) -> bool {
        self.name.is_some()
    }

    pub fn apply(&mut self, p: &[u8]) {
        if let Some(b) = aap::parse_battery(p) {
            self.battery = Some(b);
        } else if let Some((a, b)) = aap::parse_ear_detection(p) {
            // A bud in motion reports 0x04; keep its previous state.
            let old = self.ear.unwrap_or((a, b));
            let keep = |new: EarStatus, prev| if new.is_transitional() { prev } else { new };
            self.ear = Some((keep(a, old.0), keep(b, old.1)));
        } else if let Some((model, firmware, _serial)) = aap::parse_metadata(p) {
            self.model = Some(model);
            self.firmware = Some(firmware);
        } else if p.len() >= 8 && p[..4] == [0x04, 0x00, 0x04, 0x00] && p[4] == 0x09 {
            if self.controls.is_empty() {
                self.controls = vec![None; 256];
            }
            self.controls[p[6] as usize] = Some(p[7]);
        }
    }

    /// Last reported value of control `id` (the first data byte of opcode 0x09).
    pub fn control(&self, id: u8) -> Option<u8> {
        self.controls.get(id as usize).copied().flatten()
    }

    /// Ear state text for the left and right bud. The ear packet does not say which bud
    /// is which, so a charging bud is taken to be in the case and the other bud gets the
    /// remaining status; anything still ambiguous is `None`.
    pub fn bud_states(&self) -> [Option<&'static str>; 2] {
        let b = self.battery.unwrap_or_default();
        let in_case = [b.left_charging, b.right_charging];
        let text = |s: EarStatus| match s {
            EarStatus::InEar => Some("in ear"),
            EarStatus::OutOfEar => Some("out"),
            EarStatus::InCase => Some("in case"),
            EarStatus::Disconnected | EarStatus::Transitional => None,
        };
        let pick = |i: usize| {
            if in_case[i] {
                return Some("in case");
            }
            let (a, b) = self.ear?;
            if a == b {
                return text(a);
            }
            let out: Vec<EarStatus> = [a, b]
                .into_iter()
                .filter(|s| *s != EarStatus::InCase)
                .collect();
            if out.len() == 1 && in_case[1 - i] {
                text(out[0])
            } else {
                None
            }
        };
        [pick(0), pick(1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HDR: [u8; 4] = [0x04, 0x00, 0x04, 0x00];

    fn packet(body: &[u8]) -> Vec<u8> {
        [&HDR[..], body].concat()
    }

    /// Right 85 %, left 80 % charging in the case, case 32 %.
    fn battery_packet() -> Vec<u8> {
        packet(&[
            0x04, 0x00, 0x03, 0x02, 0x01, 0x55, 0x02, 0x01, 0x04, 0x01, 0x50, 0x05, 0x01, 0x08,
            0x01, 0x20, 0x02, 0x01,
        ])
    }

    #[test]
    fn stores_control_values() {
        let mut d = Device::default();
        assert_eq!(d.control(0x28), None);
        d.apply(&packet(&[0x09, 0x00, 0x28, 0x01, 0x00, 0x00, 0x00]));
        d.apply(&packet(&[0x09, 0x00, 0x0D, 0x04, 0x00, 0x00, 0x00]));
        assert_eq!(d.control(0x28), Some(1));
        assert_eq!(d.control(0x0D), Some(4));
    }

    #[test]
    fn transitional_ear_state_keeps_the_old_one() {
        let mut d = Device::default();
        d.apply(&packet(&[0x06, 0x00, 0x00, 0x00]));
        d.apply(&packet(&[0x06, 0x00, 0x04, 0x01]));
        assert_eq!(d.ear, Some((EarStatus::InEar, EarStatus::OutOfEar)));
    }

    #[test]
    fn both_buds_share_a_matching_ear_state() {
        let mut d = Device::default();
        d.apply(&packet(&[0x06, 0x00, 0x00, 0x00]));
        assert_eq!(d.bud_states(), [Some("in ear"), Some("in ear")]);
    }

    #[test]
    fn charging_bud_is_in_case_and_the_other_gets_the_rest() {
        let mut d = Device::default();
        d.apply(&battery_packet());
        d.apply(&packet(&[0x06, 0x00, 0x02, 0x00]));
        assert_eq!(d.bud_states(), [Some("in case"), Some("in ear")]);
    }

    #[test]
    fn differing_states_without_a_case_hint_stay_unknown() {
        let mut d = Device::default();
        d.apply(&packet(&[0x06, 0x00, 0x00, 0x01]));
        assert_eq!(d.bud_states(), [None, None]);
    }
}
