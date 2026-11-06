//! The line format `podatuid --probe` writes and the replay tests read: `<ms> <KIND> <args>`,
//! one event per line. Lines starting with `#` are the probe's inventory header.

use std::fmt;

use crate::proximity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Written as `IDENT` when `data` is a prefix-0x07 identity frame.
    Adv {
        addr: u64,
        public: bool,
        rssi: i16,
        data: Vec<u8>,
    },
    Ep(bool),
    Render(bool),
    Acl(bool),
    Watcher(String),
    Resume,
    Note(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub ms: u64,
    pub event: Event,
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ", self.ms)?;
        let flag = |on: &bool| u8::from(*on);
        match &self.event {
            Event::Adv {
                addr,
                public,
                rssi,
                data,
            } => {
                let kind = if proximity::parse(data).is_none()
                    && proximity::parse_identity(data).is_some()
                {
                    "IDENT"
                } else {
                    "ADV"
                };
                let side = if *public { 'P' } else { 'R' };
                write!(f, "{kind} {addr:012X} {side} {rssi} ")?;
                data.iter().try_for_each(|b| write!(f, "{b:02x}"))
            }
            Event::Ep(on) => write!(f, "EP {}", flag(on)),
            Event::Render(on) => write!(f, "RENDER {}", flag(on)),
            Event::Acl(on) => write!(f, "ACL {}", flag(on)),
            Event::Watcher(status) => write!(f, "WATCHER {status}"),
            Event::Resume => f.write_str("RESUME"),
            Event::Note(label) => write!(f, "NOTE {label}"),
        }
    }
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// None for a header line, a blank line or anything malformed.
pub fn parse(line: &str) -> Option<Line> {
    let (ms, rest) = line.trim_end().split_once(' ')?;
    let ms = ms.parse().ok()?;
    let (kind, arg) = rest.split_once(' ').unwrap_or((rest, ""));
    let flag = || match arg {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    };
    let event = match kind {
        "ADV" | "IDENT" => {
            let mut fields = arg.split(' ');
            let addr = fields.next().filter(|a| a.len() == 12).and_then(hex)?;
            let public = match fields.next()? {
                "P" => true,
                "R" => false,
                _ => return None,
            };
            let rssi = fields.next()?.parse().ok()?;
            let data = hex(fields.next()?)?;
            if fields.next().is_some() || data.is_empty() {
                return None;
            }
            Event::Adv {
                addr: addr.iter().fold(0, |a, &b| a << 8 | u64::from(b)),
                public,
                rssi,
                data,
            }
        }
        "EP" => Event::Ep(flag()?),
        "RENDER" => Event::Render(flag()?),
        "ACL" => Event::Acl(flag()?),
        "WATCHER" if !arg.is_empty() => Event::Watcher(arg.to_string()),
        "RESUME" if arg.is_empty() => Event::Resume,
        "NOTE" if !arg.is_empty() => Event::Note(arg.to_string()),
        _ => return None,
    };
    Some(Line { ms, event })
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "071901272021888f110004b48a83d66c322a4745cb15da3fd6ab2b";
    const IDENTITY: &str = "071907192035dfde4676ce665cf46ebab6ac1b694ca7d809bf9a04";

    fn adv(ms: u64, public: bool, data: &str) -> Line {
        Line {
            ms,
            event: Event::Adv {
                addr: 0x7A3B_51C0_2F9E,
                public,
                rssi: -61,
                data: hex(data).unwrap(),
            },
        }
    }

    #[test]
    fn every_kind_round_trips() {
        let lines = [
            (
                adv(1234, false, STATUS),
                format!("1234 ADV 7A3B51C02F9E R -61 {STATUS}"),
            ),
            (
                adv(1300, true, IDENTITY),
                format!("1300 IDENT 7A3B51C02F9E P -61 {IDENTITY}"),
            ),
            (
                adv(1400, false, "0f05c0"),
                "1400 ADV 7A3B51C02F9E R -61 0f05c0".into(),
            ),
            (
                Line {
                    ms: 0,
                    event: Event::Ep(true),
                },
                "0 EP 1".into(),
            ),
            (
                Line {
                    ms: 5,
                    event: Event::Render(false),
                },
                "5 RENDER 0".into(),
            ),
            (
                Line {
                    ms: 2000,
                    event: Event::Acl(true),
                },
                "2000 ACL 1".into(),
            ),
            (
                Line {
                    ms: 7,
                    event: Event::Watcher("Aborted".into()),
                },
                "7 WATCHER Aborted".into(),
            ),
            (
                Line {
                    ms: 9,
                    event: Event::Resume,
                },
                "9 RESUME".into(),
            ),
            (
                Line {
                    ms: 61000,
                    event: Event::Note("lid open 2 m".into()),
                },
                "61000 NOTE lid open 2 m".into(),
            ),
        ];
        for (line, text) in lines {
            assert_eq!(line.to_string(), text);
            assert_eq!(parse(&text), Some(line.clone()), "{text}");
            assert_eq!(parse(&format!("{text}\r")), Some(line), "{text}");
        }
    }

    #[test]
    fn rejects() {
        for text in [
            "",
            "# mac ECE9D262EBE8",
            "12",
            "x EP 1",
            "12 EP",
            "12 EP 2",
            "12 RESUME now",
            "12 NOTE",
            "12 WATCHER",
            "12 LID 1",
            "12 ADV 7A3B51C02F9E R -61",
            "12 ADV 7A3B51C02F9 R -61 0719",
            "12 ADV 7A3B51C02F9E X -61 0719",
            "12 ADV 7A3B51C02F9E R -61 071",
            "12 ADV 7A3B51C02F9E R -61 07zz",
            "12 ADV 7A3B51C02F9E R loud 0719",
            "12 ADV 7A3B51C02F9E R -61 0719 extra",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }
}
