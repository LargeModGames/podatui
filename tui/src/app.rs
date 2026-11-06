//! Sections, setting rows and key handling. Keys turn into AAP commands.

use crate::device::Device;
use podatui_core::aap;
use ratatui::crossterm::event::KeyCode;

pub const NOISE: u8 = 0x0D;
const CYCLE: u8 = 0x1A;

/// Noise modes in display order: (name, short name, AAP value, icon).
pub const MODES: [(&str, &str, u8, char); 4] = [
    ("Off", "Off", 1, '\u{F1852}'),
    ("Transparency", "Transp.", 3, '\u{F07C5}'),
    ("Adaptive", "Adaptive", 4, '\u{F0674}'),
    ("Noise Cancellation", "Noise C.", 2, '\u{F0A45}'),
];

pub enum Kind {
    /// Control id; 0x01 on, 0x02 off.
    Switch(u8),
    /// Control id and its (label, value) options.
    Choice(u8, &'static [(&'static str, u8)]),
    /// Noise mode value of control 0x0D.
    Radio(u8),
    /// Bit in the press-and-hold cycle mask (control 0x1A).
    Check(u8),
    /// Control id, 0..=100.
    Slider(u8),
    Info(Info),
}

#[derive(Clone, Copy)]
pub enum Info {
    Name,
    Model,
    Firmware,
    Address,
    HeartRate,
}

pub struct Row {
    pub icon: char,
    pub label: &'static str,
    pub kind: Kind,
}

pub struct Group {
    pub title: &'static str,
    pub rows: &'static [Row],
}

pub struct Section {
    pub name: &'static str,
    pub icon: char,
    pub groups: &'static [Group],
}

const fn row(icon: char, label: &'static str, kind: Kind) -> Row {
    Row { icon, label, kind }
}

const NOISE_SHORT: &[(&str, u8)] = &[("Off", 1), ("Transp.", 3), ("Adaptive", 4), ("Noise C.", 2)];
const SPEED: &[(&str, u8)] = &[("Default", 0), ("Slower", 1), ("Slowest", 2)];
const LENGTH: &[(&str, u8)] = &[("Default", 0), ("Longer", 1), ("Longest", 2)];
const MIC: &[(&str, u8)] = &[("Automatic", 0), ("Right", 1), ("Left", 2)];

const CONVERSATION: Row = row('\u{F05CB}', "Conversation Awareness", Kind::Switch(0x28));
const PERSONAL_VOLUME: Row = row('\u{F057E}', "Personalized Volume", Kind::Switch(0x26));
const RADIOS: [Row; 4] = [
    row(MODES[0].3, MODES[0].0, Kind::Radio(MODES[0].2)),
    row(MODES[1].3, MODES[1].0, Kind::Radio(MODES[1].2)),
    row(MODES[2].3, MODES[2].0, Kind::Radio(MODES[2].2)),
    row(MODES[3].3, MODES[3].0, Kind::Radio(MODES[3].2)),
];

pub const SECTIONS: [Section; 5] = [
    Section {
        name: "General",
        icon: '\u{F0493}',
        groups: &[Group {
            title: "",
            rows: &[
                row(
                    '\u{F07C5}',
                    "Noise Control",
                    Kind::Choice(NOISE, NOISE_SHORT),
                ),
                CONVERSATION,
                PERSONAL_VOLUME,
            ],
        }],
    },
    Section {
        name: "Noise Control",
        icon: '\u{F07C5}',
        groups: &[
            Group {
                title: "",
                rows: &RADIOS,
            },
            Group {
                title: "Press and hold cycles between",
                rows: &[
                    row(MODES[0].3, MODES[0].0, Kind::Check(0x01)),
                    row(MODES[1].3, MODES[1].0, Kind::Check(0x04)),
                    row(MODES[2].3, MODES[2].0, Kind::Check(0x08)),
                    row(MODES[3].3, MODES[3].0, Kind::Check(0x02)),
                ],
            },
            Group {
                title: "",
                rows: &[
                    row(
                        '\u{F184F}',
                        "Noise Cancellation with one AirPod",
                        Kind::Switch(0x1B),
                    ),
                    row('\u{F0674}', "Adaptive Audio strength", Kind::Slider(0x2E)),
                    CONVERSATION,
                    PERSONAL_VOLUME,
                ],
            },
        ],
    },
    Section {
        name: "Controls",
        icon: '\u{F0741}',
        groups: &[
            Group {
                title: "Press and hold",
                rows: &[
                    row('\u{F0741}', "Press speed", Kind::Choice(0x17, SPEED)),
                    row('\u{F0D77}', "Hold duration", Kind::Choice(0x18, SPEED)),
                ],
            },
            Group {
                title: "Volume",
                rows: &[
                    row('\u{F057E}', "Swipe to adjust volume", Kind::Switch(0x25)),
                    row('\u{F057E}', "Swipe interval", Kind::Choice(0x23, LENGTH)),
                ],
            },
            Group {
                title: "",
                rows: &[
                    row('\u{F07C5}', "Automatic ear detection", Kind::Switch(0x0A)),
                    row('\u{F036C}', "Microphone", Kind::Choice(0x01, MIC)),
                    row('\u{F009F}', "Chime volume", Kind::Slider(0x1F)),
                ],
            },
        ],
    },
    Section {
        name: "Health",
        icon: '\u{F05F6}',
        groups: &[
            Group {
                title: "",
                rows: &[
                    row('\u{F05F6}', "Heart rate monitoring", Kind::Switch(0x30)),
                    row('\u{F02D5}', "Heart rate", Kind::Info(Info::HeartRate)),
                ],
            },
            Group {
                title: "",
                rows: &[row('\u{F04B2}', "Sleep detection", Kind::Switch(0x35))],
            },
        ],
    },
    Section {
        name: "About",
        icon: '\u{F02FD}',
        groups: &[Group {
            title: "",
            rows: &[
                row('\u{F02CB}', "Name", Kind::Info(Info::Name)),
                row('\u{F02FD}', "Model", Kind::Info(Info::Model)),
                row('\u{F02FD}', "Firmware", Kind::Info(Info::Firmware)),
                row('\u{F00AF}', "Address", Kind::Info(Info::Address)),
            ],
        }],
    },
];

/// The small-terminal card: the noise radios and the two switches.
pub const COMPACT: [Group; 2] = [
    Group {
        title: "Noise Control",
        rows: &RADIOS,
    },
    Group {
        title: "",
        rows: &[CONVERSATION, PERSONAL_VOLUME],
    },
];

pub enum LinkEvent {
    Connected {
        name: String,
        addr: String,
    },
    Packet(Vec<u8>),
    Down(String),
    Bpm(u16),
    /// The heart-rate stream gave no reading after every retry.
    HeartRateFailed,
}

#[derive(PartialEq, Eq, Debug)]
pub enum LinkCommand {
    Send(Vec<u8>),
    HeartRate(bool),
}

/// Live heart-rate reading, started from the Health section.
#[derive(Default, PartialEq, Eq, Debug)]
pub enum Heart {
    #[default]
    Off,
    Starting,
    /// Recent readings, oldest first.
    Live(Vec<u16>),
    Failed,
}

/// Readings kept for the sparkline.
pub const HEART_HISTORY: usize = 12;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Focus {
    Sidebar,
    Content,
}

pub struct App {
    pub device: Device,
    pub heart: Heart,
    /// Why the link is down, shown while disconnected.
    pub status: String,
    pub section: usize,
    pub focus: Focus,
    pub cursor: usize,
    pub compact: bool,
    pub quit: bool,
}

impl Default for App {
    fn default() -> Self {
        App {
            device: Device::default(),
            heart: Heart::Off,
            status: "looking for AirPods".into(),
            section: 0,
            focus: Focus::Content,
            cursor: 0,
            compact: false,
            quit: false,
        }
    }
}

impl App {
    pub fn groups(&self) -> &'static [Group] {
        if self.compact {
            &COMPACT
        } else {
            SECTIONS[self.section].groups
        }
    }

    pub fn rows(&self) -> impl Iterator<Item = &'static Row> {
        self.groups().iter().flat_map(|g| g.rows.iter())
    }

    pub fn on_link(&mut self, event: LinkEvent) {
        match event {
            LinkEvent::Connected { name, addr } => {
                self.device = Device::default();
                self.device.name = Some(name);
                self.device.addr = addr;
                self.heart = Heart::Off;
            }
            LinkEvent::Packet(p) => self.device.apply(&p),
            LinkEvent::Down(why) => {
                self.device.name = None;
                self.status = why;
                self.heart = Heart::Off;
            }
            LinkEvent::Bpm(bpm) => {
                let mut history = match std::mem::take(&mut self.heart) {
                    Heart::Live(h) => h,
                    _ => Vec::new(),
                };
                history.push(bpm);
                if history.len() > HEART_HISTORY {
                    history.remove(0);
                }
                self.heart = Heart::Live(history);
            }
            LinkEvent::HeartRateFailed => self.heart = Heart::Failed,
        }
    }

    /// Handle a key press; returns what to tell the link, if anything.
    pub fn key(&mut self, code: KeyCode) -> Option<LinkCommand> {
        let content = self.compact || self.focus == Focus::Content;
        let rows = self.rows().count();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::BackTab if !self.compact => {
                self.focus = if content {
                    Focus::Sidebar
                } else {
                    Focus::Content
                };
                self.cursor = 0;
            }
            KeyCode::Up | KeyCode::Char('k') if content => {
                self.cursor = self.cursor.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') if content => {
                self.cursor = (self.cursor + 1).min(rows - 1)
            }
            KeyCode::Up | KeyCode::Char('k') => self.section = self.section.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.section = (self.section + 1).min(SECTIONS.len() - 1)
            }
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right | KeyCode::Char('l')
                if !content =>
            {
                self.focus = Focus::Content;
                self.cursor = 0;
            }
            KeyCode::Char(c @ '1'..='4') => {
                return self.send(aap::anc_command(MODES[c as usize - '1' as usize].2).to_vec());
            }
            KeyCode::Enter | KeyCode::Char(' ') => return self.activate(0),
            KeyCode::Right | KeyCode::Char('l') => return self.activate(1),
            KeyCode::Left | KeyCode::Char('h') => return self.activate(-1),
            _ => {}
        }
        None
    }

    /// Act on the cursor row: `step` 0 is Enter, +1/-1 are right/left.
    fn activate(&mut self, step: i32) -> Option<LinkCommand> {
        let row = self.rows().nth(self.cursor)?;
        let d = &self.device;
        let cmd = match row.kind {
            Kind::Info(Info::HeartRate) if step == 0 && d.connected() => {
                let on = matches!(self.heart, Heart::Off | Heart::Failed);
                self.heart = if on { Heart::Starting } else { Heart::Off };
                return Some(LinkCommand::HeartRate(on));
            }
            Kind::Switch(id) if step == 0 => aap::feature_command(id, d.control(id) != Some(1)),
            Kind::Radio(v) if step == 0 => aap::anc_command(v),
            Kind::Check(bit) if step == 0 => {
                aap::control_command(CYCLE, d.control(CYCLE).unwrap_or(0x0E) ^ bit)
            }
            Kind::Choice(id, opts) => {
                let at = opts
                    .iter()
                    .position(|o| Some(o.1) == d.control(id))
                    .unwrap_or(0) as i32;
                let next = (at + if step == 0 { 1 } else { step }).rem_euclid(opts.len() as i32);
                aap::control_command(id, opts[next as usize].1)
            }
            Kind::Slider(id) if step != 0 => {
                let v = d.control(id).unwrap_or(50) as i32 + step * 5;
                aap::control_command(id, v.clamp(0, 100) as u8)
            }
            _ => return None,
        };
        self.send(cmd.to_vec())
    }

    /// The AirPods do not echo control changes, so apply the command locally as it is sent.
    fn send(&mut self, cmd: Vec<u8>) -> Option<LinkCommand> {
        if !self.device.connected() {
            return None;
        }
        self.device.apply(&cmd);
        Some(LinkCommand::Send(cmd))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> App {
        let mut app = App::default();
        app.on_link(LinkEvent::Connected {
            name: "AirPods".into(),
            addr: "00".into(),
        });
        app
    }

    fn sent(cmd: &[u8]) -> Option<LinkCommand> {
        Some(LinkCommand::Send(cmd.to_vec()))
    }

    fn report(app: &mut App, id: u8, value: u8) {
        app.on_link(LinkEvent::Packet(vec![
            0x04, 0x00, 0x04, 0x00, 0x09, 0x00, id, value, 0, 0, 0,
        ]));
    }

    #[test]
    fn switch_sends_the_opposite_of_the_reported_value() {
        let mut app = connected();
        report(&mut app, 0x28, 0x01);
        app.cursor = 1;
        assert_eq!(
            app.key(KeyCode::Enter),
            sent(&aap::feature_command(0x28, false))
        );
        assert_eq!(app.device.control(0x28), Some(0x02));
    }

    #[test]
    fn number_keys_set_the_noise_mode() {
        let mut app = connected();
        assert_eq!(app.key(KeyCode::Char('4')), sent(&aap::anc_command(2)));
        assert_eq!(app.key(KeyCode::Char('2')), sent(&aap::anc_command(3)));
    }

    #[test]
    fn choice_steps_through_its_options_both_ways() {
        let mut app = connected();
        report(&mut app, NOISE, 4);
        assert_eq!(app.key(KeyCode::Right), sent(&aap::anc_command(2)));
        assert_eq!(app.key(KeyCode::Left), sent(&aap::anc_command(4)));
    }

    #[test]
    fn check_flips_one_bit_of_the_cycle_mask() {
        let mut app = connected();
        app.section = 1;
        report(&mut app, CYCLE, 0x0E);
        app.cursor = 4;
        assert_eq!(
            app.key(KeyCode::Enter),
            sent(&aap::control_command(CYCLE, 0x0F))
        );
    }

    #[test]
    fn slider_moves_in_fives_and_stops_at_100() {
        let mut app = connected();
        app.section = 1;
        report(&mut app, 0x2E, 98);
        app.cursor = 9;
        assert_eq!(
            app.key(KeyCode::Right),
            sent(&aap::control_command(0x2E, 100))
        );
        assert_eq!(
            app.key(KeyCode::Left),
            sent(&aap::control_command(0x2E, 95))
        );
    }

    #[test]
    fn sidebar_moves_sections_and_enter_goes_to_settings() {
        let mut app = connected();
        app.key(KeyCode::Tab);
        app.key(KeyCode::Down);
        app.key(KeyCode::Down);
        assert_eq!((app.focus, app.section), (Focus::Sidebar, 2));
        assert_eq!(app.key(KeyCode::Enter), None);
        assert_eq!(app.focus, Focus::Content);
    }

    #[test]
    fn nothing_is_sent_while_disconnected() {
        let mut app = App::default();
        assert_eq!(app.key(KeyCode::Char('1')), None);
        app.key(KeyCode::Char('q'));
        assert!(app.quit);
    }

    #[test]
    fn heart_rate_row_starts_and_stops_the_stream() {
        let mut app = connected();
        app.section = 3;
        app.cursor = 1;
        assert_eq!(app.key(KeyCode::Enter), Some(LinkCommand::HeartRate(true)));
        assert_eq!(app.heart, Heart::Starting);
        assert_eq!(app.key(KeyCode::Enter), Some(LinkCommand::HeartRate(false)));
        assert_eq!(app.heart, Heart::Off);
    }

    #[test]
    fn readings_keep_a_short_history() {
        let mut app = connected();
        for bpm in 60..60 + HEART_HISTORY as u16 + 3 {
            app.on_link(LinkEvent::Bpm(bpm));
        }
        let Heart::Live(h) = &app.heart else {
            panic!("not live: {:?}", app.heart)
        };
        assert_eq!(h.len(), HEART_HISTORY);
        assert_eq!(h.last(), Some(&(60 + HEART_HISTORY as u16 + 2)));
        app.on_link(LinkEvent::Down("gone".into()));
        assert_eq!(app.heart, Heart::Off);
    }
}
