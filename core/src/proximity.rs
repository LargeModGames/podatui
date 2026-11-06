//! Apple proximity-pairing adverts: the cleartext status AirPods broadcast over BLE,
//! read from the manufacturer data (company 0x004C) without any key.

const PROXIMITY_PAIRING: u8 = 0x07;
const PAYLOAD_LEN: usize = 0x19;
const STATUS_PREFIX: u8 = 0x01;
const IDENTITY_PREFIX: u8 = 0x07;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// The counter is the low three bits of the lid byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lid {
    Open(u8),
    Closed(u8),
    Unknown,
    NotInCase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bud {
    pub battery: Option<u8>,
    pub charging: bool,
    pub in_ear: bool,
    pub in_case: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Advert {
    pub model: u16,
    pub status: u8,
    pub sender: Side,
    pub primary: Side,
    pub left: Bud,
    pub right: Bud,
    pub case: Option<u8>,
    pub case_charging: bool,
    pub lid: Lid,
    pub color: u8,
    pub audio: u8,
    pub tail: [u8; 16],
}

fn bit(byte: u8, n: u8) -> bool {
    (byte >> n) & 1 == 1
}

fn level(nibble: u8) -> Option<u8> {
    match nibble {
        0xF => None,
        n => Some(n.min(10) * 10),
    }
}

fn payload(data: &[u8], prefix: u8) -> Option<&[u8]> {
    let mut rest = data;
    while let [kind, len, tail @ ..] = rest {
        let len = usize::from(*len);
        let body = tail.get(..len)?;
        if *kind == PROXIMITY_PAIRING && len == PAYLOAD_LEN && body[0] == prefix {
            return Some(body);
        }
        rest = &tail[len..];
    }
    None
}

/// Walks the Apple records (from the first type byte, no company ID) for a status frame.
pub fn parse(data: &[u8]) -> Option<Advert> {
    let p = payload(data, STATUS_PREFIX)?;
    let status = p[3];
    let sender_in_case = bit(status, 6);
    let one_in_case = bit(status, 4);
    let both_in_case = bit(status, 2);
    let (own_ear, other_ear) = if sender_in_case { (3, 1) } else { (1, 3) };
    let own_charging = bit(p[5], 4);
    let other_charging = bit(p[5], 5);
    let own = Bud {
        battery: level(p[4] & 0xF),
        charging: own_charging,
        in_ear: bit(status, own_ear) && !own_charging,
        in_case: sender_in_case || both_in_case,
    };
    let other = Bud {
        battery: level(p[4] >> 4),
        charging: other_charging,
        in_ear: bit(status, other_ear) && !other_charging,
        in_case: both_in_case || (one_in_case && !sender_in_case),
    };
    let left_sends = bit(status, 5);
    let (sender, left, right) = if left_sends {
        (Side::Left, own, other)
    } else {
        (Side::Right, other, own)
    };
    let primary = if left_sends ^ sender_in_case {
        Side::Left
    } else {
        Side::Right
    };
    let lid = if !(sender_in_case || one_in_case || both_in_case) {
        Lid::NotInCase
    } else if !(sender_in_case || both_in_case) {
        Lid::Unknown
    } else if bit(p[6], 3) {
        Lid::Closed(p[6] & 0x07)
    } else {
        Lid::Open(p[6] & 0x07)
    };
    let mut tail = [0; 16];
    tail.copy_from_slice(&p[9..]);
    Some(Advert {
        model: u16::from_le_bytes([p[1], p[2]]),
        status,
        sender,
        primary,
        left,
        right,
        case: level(p[5] & 0xF),
        case_charging: bit(p[5], 6),
        lid,
        color: p[7],
        audio: p[8],
        tail,
    })
}

/// The model of a prefix-0x07 frame, which a pair sends from its identity address.
pub fn parse_identity(data: &[u8]) -> Option<u16> {
    let p = payload(data, IDENTITY_PREFIX)?;
    Some(u16::from_le_bytes([p[1], p[2]]))
}

pub fn model_name(pid: u16) -> Option<&'static str> {
    Some(match pid {
        0x2002 => "AirPods",
        0x200F => "AirPods 2",
        0x2013 => "AirPods 3",
        0x2019 => "AirPods 4",
        0x201B => "AirPods 4 (ANC)",
        0x200E => "AirPods Pro",
        0x2014 => "AirPods Pro 2",
        0x2024 => "AirPods Pro 2 (USB-C)",
        0x2027 => "AirPods Pro 3",
        0x200A => "AirPods Max",
        0x201F => "AirPods Max (USB-C)",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from AirPods Pro 3 (OpenPods tst_blemanager.cpp).
    const PRO3: &str =
        "07 19 01 27 20 21 88 8F 11 00 04 B4 8A 83 D6 6C 32 2A 47 45 CB 15 DA 3F D6 AB 2B";
    // CAPod #164, AirPods Pro 2 (USB-C), sent by the right bud from the open case.
    const PRO2_USBC_IN_CASE: &str =
        "07 19 01 24 20 53 AA 98 32 00 05 49 0A B8 BF 8E 29 D8 70 12 0D A7 0C CE 77 56 00";
    const NEARBY_INFO: &str = "10 06 0C 19 0B 3D 57 68";

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    fn advert(s: &str) -> Advert {
        parse(&hex(s)).unwrap()
    }

    fn batteries(a: &Advert) -> [Option<u8>; 3] {
        [a.left.battery, a.right.battery, a.case]
    }

    fn charging(a: &Advert) -> [bool; 3] {
        [a.left.charging, a.right.charging, a.case_charging]
    }

    fn ears(a: &Advert) -> [bool; 2] {
        [a.left.in_ear, a.right.in_ear]
    }

    fn with(s: &str, i: usize, b: u8) -> Vec<u8> {
        let mut v = hex(s);
        v[i] = b;
        v
    }

    #[test]
    fn pro3_capture_left_sends_out_of_case() {
        let a = advert(PRO3);
        assert_eq!(a.model, 0x2027);
        assert_eq!(a.status, 0x21);
        assert_eq!((a.sender, a.primary), (Side::Left, Side::Left));
        assert_eq!(batteries(&a), [Some(80), Some(80), None]);
        assert_eq!(charging(&a), [false; 3]);
        assert_eq!(ears(&a), [false, false]);
        assert!(!a.left.in_case && !a.right.in_case);
        assert_eq!(a.lid, Lid::NotInCase);
        assert_eq!((a.color, a.audio), (0x00, 0x04));
        assert_eq!(a.tail[0], 0xB4);
        assert_eq!(a.tail[15], 0x2B);
    }

    #[test]
    fn sender_owns_the_low_nibble() {
        let unequal = with(PRO3, 6, 0x82);
        let a = parse(&unequal).unwrap();
        assert_eq!(batteries(&a)[..2], [Some(20), Some(80)]);
        let mut right_sends = unequal;
        right_sends[5] &= !0x20;
        let a = parse(&right_sends).unwrap();
        assert_eq!(a.sender, Side::Right);
        assert_eq!(batteries(&a)[..2], [Some(80), Some(20)]);
    }

    #[test]
    fn earshot_pro3_both_in_ear() {
        let a = advert(
            "07 19 01 27 20 2B 99 8F 11 00 09 09 5A 5C FF 46 52 E7 00 00 00 76 1C B2 37 22 24",
        );
        assert_eq!(a.model, 0x2027);
        assert_eq!(a.sender, Side::Left);
        assert_eq!(batteries(&a), [Some(90), Some(90), None]);
        assert_eq!(ears(&a), [true, true]);
        assert_eq!(a.lid, Lid::NotInCase);
        assert_eq!(a.audio, 0x09);
    }

    #[test]
    fn macmix_pro3_on_mac_and_on_phone() {
        let mac = hex("4C 00 07 19 01 27 20 0B AA 8F 10 00 08 48 64 64 FF 36 EB EA 00 00 00 66 1A 7E DA 3D 3F");
        let phone = hex("4C 00 07 19 01 27 20 2B AA 8F 11 00 08 08 64 62 FF 67 23 6D 00 00 00 26 10 88 18 3E 5D");
        let a = parse(&mac[2..]).unwrap();
        assert_eq!(
            (a.status, a.sender, a.primary),
            (0x0B, Side::Right, Side::Right)
        );
        assert_eq!(batteries(&a), [Some(100), Some(100), None]);
        assert_eq!(ears(&a), [true, true]);
        assert_eq!(a.audio, 0x08);
        let a = parse(&phone[2..]).unwrap();
        assert_eq!(
            (a.status, a.sender, a.primary),
            (0x2B, Side::Left, Side::Left)
        );
        assert_eq!(ears(&a), [true, true]);
        assert_eq!(parse(&mac[2..20]), None);
    }

    #[test]
    fn capod31_pro2_all_charging_lid_open() {
        let s = "07 19 01 14 20 55 88 F9 51 00 04 20 50 03 CA D5 C9 AC 0F FA 84 78 94 5A 4D DF F5";
        let a = advert(s);
        assert_eq!(a.model, 0x2014);
        assert_eq!(batteries(&a), [Some(80), Some(80), Some(90)]);
        assert_eq!(charging(&a), [true; 3]);
        assert_eq!(ears(&a), [false, false]);
        assert!(a.left.in_case && a.right.in_case);
        assert_eq!(a.lid, Lid::Open(1));
        assert_eq!(parse(&with(s, 8, 0x59)).unwrap().lid, Lid::Closed(1));
    }

    #[test]
    fn capod164_right_charging_in_open_case() {
        let a = advert(PRO2_USBC_IN_CASE);
        assert_eq!(a.model, 0x2024);
        assert_eq!((a.sender, a.primary), (Side::Right, Side::Left));
        assert_eq!(batteries(&a), [Some(100), Some(100), Some(80)]);
        assert_eq!(charging(&a), [false, true, false]);
        assert_eq!(ears(&a), [true, false]);
        assert_eq!((a.left.in_case, a.right.in_case), (false, true));
        assert_eq!(a.lid, Lid::Open(2));
        let pro3 = parse(&with(PRO2_USBC_IN_CASE, 3, 0x27)).unwrap();
        assert_eq!(pro3, Advert { model: 0x2027, ..a });
    }

    #[test]
    fn capod164_both_in_ear() {
        let a = advert(
            "07 19 01 24 20 0B 99 8F 11 00 04 BD A7 3B FF 2D 8A 3C AF 9B 1A 7C 74 B7 A9 D1 C3",
        );
        assert_eq!((a.sender, a.primary), (Side::Right, Side::Right));
        assert_eq!(batteries(&a), [Some(90), Some(90), None]);
        assert_eq!(ears(&a), [true, true]);
        assert_eq!(a.lid, Lid::NotInCase);
    }

    #[test]
    fn reddit_pro2_unequal_buds() {
        let a = advert(
            "07 19 01 14 20 2B 9A 8F 01 00 04 0F 26 1A C4 2B FA 2F B9 B6 08 CD 60 CB DF 75 AB",
        );
        assert_eq!(batteries(&a), [Some(100), Some(90), None]);
        assert_eq!(ears(&a), [true, true]);
    }

    #[test]
    fn apple_log_pro1_right_primary_in_case() {
        let a = advert(
            "07 19 01 0E 20 04 F9 99 31 00 00 00 DF FF 61 CA BF D7 70 FF 0B 00 10 B1 13 A9 E3",
        );
        assert_eq!(a.model, 0x200E);
        assert_eq!((a.sender, a.primary), (Side::Right, Side::Right));
        assert_eq!(batteries(&a), [None, Some(90), Some(90)]);
        assert_eq!(charging(&a), [false, true, false]);
        assert!(a.left.in_case && a.right.in_case);
        assert_eq!(a.lid, Lid::Open(1));
    }

    #[test]
    fn bit4_only_lid_is_unknown() {
        let removed = advert(
            "07 19 01 0E 20 31 AA A2 01 00 04 46 FB 26 B8 D7 7A E0 4E 8C 3E A8 A3 3A 8A AB 9E",
        );
        assert_eq!(removed.lid, Lid::Unknown);
        assert_eq!((removed.left.in_case, removed.right.in_case), (false, true));
        let worn = advert(
            "07 19 01 0E 20 13 A9 AA 02 00 09 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
        );
        assert_eq!(worn.lid, Lid::Unknown);
        let in_case = advert(
            "07 19 01 0E 20 73 9A 9A 33 00 09 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
        );
        assert_eq!(in_case.lid, Lid::Open(3));
        assert_eq!((in_case.left.in_case, in_case.right.in_case), (true, false));
    }

    #[test]
    fn charging_bud_is_not_in_ear() {
        let mut both_worn = with(PRO3, 5, 0x2B);
        assert_eq!(ears(&parse(&both_worn).unwrap()), [true, true]);
        both_worn[7] = 0x1F;
        let a = parse(&both_worn).unwrap();
        assert_eq!(charging(&a), [true, false, false]);
        assert_eq!(ears(&a), [false, true]);
    }

    #[test]
    fn nibbles_above_ten_clamp_to_100() {
        let a = parse(&with(PRO3, 6, 0xEB)).unwrap();
        assert_eq!(batteries(&a)[..2], [Some(100), Some(100)]);
        assert_eq!(level(0xA), Some(100));
        assert_eq!(level(0xF), None);
    }

    #[test]
    fn earbud_vectors() {
        type Row = (&'static str, u16, [Option<u8>; 3], [bool; 3]);
        let rows: [Row; 18] = [
            (
                "07 19 01 0E 20 0B A9 89 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
                0x200E,
                [Some(100), Some(90), Some(90)],
                [false; 3],
            ),
            (
                "07 19 01 0E 20 13 A9 AA 02 00 09 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
                0x200E,
                [Some(100), Some(90), Some(100)],
                [true, false, false],
            ),
            (
                "07 19 01 0E 20 73 9A 9A 33 00 09 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
                0x200E,
                [Some(100), Some(90), Some(100)],
                [true, false, false],
            ),
            (
                "07 19 01 0E 20 51 9A 98 33 00 04 0C 14 E0 EB 43 3F 4B 22 C0 A9 ED CB 33 E7 09 71",
                0x200E,
                [Some(90), Some(100), Some(80)],
                [false, true, false],
            ),
            (
                "07 19 01 0E 20 31 AA A2 01 00 04 46 FB 26 B8 D7 7A E0 4E 8C 3E A8 A3 3A 8A AB 9E",
                0x200E,
                [Some(100), Some(100), Some(20)],
                [false, true, false],
            ),
            (
                "07 19 01 0E 20 13 AA EA 02 00 09 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
                0x200E,
                [Some(100), Some(100), Some(100)],
                [true, false, true],
            ),
            (
                "07 19 01 0E 20 13 AA CA 02 00 09 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
                0x200E,
                [Some(100), Some(100), Some(100)],
                [false, false, true],
            ),
            (
                "07 19 01 0F 20 01 A9 8F 01 00 04 C0 D1 B9 25 94 25 F3 48 1F 56 89 E3 4E 40 39 EF",
                0x200F,
                [Some(100), Some(90), None],
                [false; 3],
            ),
            (
                "07 19 01 0F 20 2B 43 8F 00 00 05 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
                0x200F,
                [Some(30), Some(40), None],
                [false; 3],
            ),
            (
                "07 19 01 13 20 2B 88 8F 01 00 08 E2 0E 84 37 C5 98 16 D4 B7 37 ED 23 8B 08 EA A1",
                0x2013,
                [Some(80), Some(80), None],
                [false; 3],
            ),
            (
                "07 19 01 19 20 2B 33 8F 11 00 04 59 D4 57 20 0F 1C 13 38 B2 00 74 E9 DD 70 D7 A5",
                0x2019,
                [Some(30), Some(30), None],
                [false; 3],
            ),
            (
                "07 19 01 1B 20 0B 9A 8F 10 00 04 43 DF EC 1D D3 F1 C3 F4 A1 9B 29 26 B9 E7 3A A0",
                0x201B,
                [Some(90), Some(100), None],
                [false; 3],
            ),
            (
                "07 19 01 30 20 25 9A B9 53 00 00 6D FD 2E D1 14 79 67 58 A8 D7 2C 26 2E B0 16 21",
                0x2030,
                [Some(100), Some(90), Some(90)],
                [true, true, false],
            ),
            (
                "07 19 01 30 20 13 A9 A9 13 00 04 46 84 C6 81 98 F3 9B 4D EA 95 8C CA 1D 6B E7 F5",
                0x2030,
                [Some(100), Some(90), Some(90)],
                [true, false, false],
            ),
            (
                "07 19 01 36 20 15 99 F9 51 00 04 45 C5 C4 F9 1E 09 A6 66 64 52 15 DE FF 5E 1F 27",
                0x2036,
                [Some(90), Some(90), Some(90)],
                [true; 3],
            ),
            (
                "07 19 01 0B 20 21 AA 8F 02 44 24 6D 3E CD 38 A6 F9 6C 6A EC 95 65 AF 97 08 95 49",
                0x200B,
                [Some(100), Some(100), None],
                [false; 3],
            ),
            (
                "07 19 01 12 20 20 FA 8F 01 11 24 9B 9B 23 52 60 5A 8C 32 1C A5 C2 81 51 82 AF C8",
                0x2012,
                [Some(100), None, None],
                [false; 3],
            ),
            (
                "07 19 01 11 20 2B 88 0F 00 11 05 77 7B 0E E5 B8 AD 03 57 2F FA AA 2C F5 65 0F 52",
                0x2011,
                [Some(80), Some(80), None],
                [false; 3],
            ),
        ];
        for (s, model, batt, chg) in rows {
            let a = advert(s);
            assert_eq!(
                (a.model, batteries(&a), charging(&a)),
                (model, batt, chg),
                "{s}"
            );
        }
    }

    #[test]
    fn non_status_frames_are_rejected() {
        let rejects = [
            "07 19 07 19 20 35 DF DE 46 76 CE 66 5C F4 6E BA B6 AC 1B 69 4C A7 D8 09 BF 9A 04",
            "07 19 00 0E 20 51 9A 98 33 00 04 0C 14 E0 EB 43 3F 4B 22 C0 A9 ED CB 33 E7 09 71",
            "07 19 06 0E 20 51 9A 98 33 00 04 0C 14 E0 EB 43 3F 4B 22 C0 A9 ED CB 33 E7 09 71",
            "07 0F 00 0F 20 D0 DA D7 0F 14 48 15 E2 E2 23 02 00",
            "07 11 06 29 20 0B 28 FF FF 51 0B 00 00 00 00 7D F2 02 00",
            "07 11 06 29 20 0F 5B E4 E3 5B 02 00 00 00 00 19 AE 02 00",
            "07 13 01 14 20 75 AA 72 39 00 00 6F E4 E4 93 30 00 30 30 30 30",
            "12 19 00 3D 99 67 E0 D6 7B F5 56 17 93 90 43 E4 8F D6 76 21 44 DA 3E 35 16 03 00",
            "10 06 5B 1E 02 71 1C 91",
            "0C 0E 00 A7 58 2C D6 4F FF 2F E8 30 46 C9 9F 5B 10 06 5A 19 E9 66 70 D8",
            NEARBY_INFO,
            "07 19 01 0E 20 0B A9",
            "07 19 01 0E 20 51 9A 98 33 00 04 0C 14 E0 EB 43 3F 4B 22 C0 A9 ED CB 33 E7 09",
            "",
            "07",
        ];
        for s in rejects {
            assert_eq!(parse(&hex(s)), None, "{s}");
        }
    }

    #[test]
    fn identity_frame_gives_its_model() {
        let identity =
            hex("07 19 07 19 20 35 DF DE 46 76 CE 66 5C F4 6E BA B6 AC 1B 69 4C A7 D8 09 BF 9A 04");
        assert_eq!(parse_identity(&identity), Some(0x2019));
        assert_eq!(parse_identity(&hex(PRO3)), None);
        assert_eq!(parse_identity(&identity[..20]), None);
    }

    #[test]
    fn walks_multiple_records() {
        let mut data = hex(NEARBY_INFO);
        data.extend(hex(PRO3));
        assert_eq!(parse(&data), Some(advert(PRO3)));
        data.extend([0x10, 0x06, 0x0C]);
        assert_eq!(parse(&data), Some(advert(PRO3)));
        let mut overrun = hex(NEARBY_INFO);
        overrun.extend(&hex(PRO3)[..20]);
        assert_eq!(parse(&overrun), None);
    }

    #[test]
    fn model_names() {
        assert_eq!(model_name(0x2027), Some("AirPods Pro 3"));
        assert_eq!(model_name(0x2014), Some("AirPods Pro 2"));
        assert_eq!(model_name(0x2024), Some("AirPods Pro 2 (USB-C)"));
        assert_eq!(model_name(0x200E), Some("AirPods Pro"));
        assert_eq!(model_name(0x2019), Some("AirPods 4"));
        assert_eq!(model_name(0x2012), None);
    }
}
