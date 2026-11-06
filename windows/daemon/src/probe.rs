//! `podatuid --probe`: logs proximity adverts, watcher status, render and link
//! edges, and typed NOTE lines in the replay-fixture format (core `fixture`), to
//! stdout and `%LOCALAPPDATA%\podatui\probe-<utc>.txt`. Runs next to the daemon.

use std::fs::File;
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use podatui_core::fixture::{Event, Line};
use podatui_core::proximity;
use windows_sys::Win32::Storage::FileSystem::{GetFileType, FILE_TYPE_DISK, FILE_TYPE_PIPE};
use windows_sys::Win32::System::Console::{AllocConsole, GetStdHandle, STD_OUTPUT_HANDLE};

use crate::{bt, le, utc_timestamp, volume};

/// Proximity pairing and nearby action records.
const LOGGED: [u8; 2] = [0x07, 0x0F];
const POLL: Duration = Duration::from_millis(500);
const ACL_EVERY: u32 = 4;

struct Out {
    start: Instant,
    file: Option<File>,
}

impl Out {
    fn text(&mut self, s: &str) {
        let _ = writeln!(std::io::stdout(), "{s}");
        if let Some(f) = &mut self.file {
            let _ = f.write_all(format!("{s}\r\n").as_bytes());
        }
    }

    fn event(&mut self, event: Event) {
        let ms = self.start.elapsed().as_millis() as u64;
        self.text(&Line { ms, event }.to_string());
    }
}

fn has_record(mut data: &[u8]) -> bool {
    while let [kind, len, rest @ ..] = data {
        if LOGGED.contains(kind) {
            return true;
        }
        data = rest.get(usize::from(*len)..).unwrap_or_default();
    }
    false
}

fn guid(v: u128) -> String {
    let h = format!("{v:032X}");
    format!(
        "{{{}-{}-{}-{}-{}}}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

fn header(o: &mut Out, started: &str, path: Option<&str>) -> u64 {
    o.text(&format!(
        "# podatuid {} --probe started {started}",
        env!("CARGO_PKG_VERSION")
    ));
    o.text(&format!("# file {}", path.unwrap_or("none")));
    let Some((mac, name)) = bt::find_airpods() else {
        o.text("# mac none");
        return 0;
    };
    let p = bt::device_props(mac);
    let none = || "none".to_string();
    o.text(&format!("# mac {mac:012X} name '{name}'"));
    o.text(&format!(
        "# vid {} pid {} model {} container {} hfp_battery {} aap {}",
        p.vid.map_or_else(none, |v| format!("{v:04X}")),
        p.pid.map_or_else(none, |v| format!("{v:04X}")),
        p.pid.and_then(proximity::model_name).unwrap_or("unknown"),
        p.container.map_or_else(none, guid),
        p.hfp_battery.map_or_else(none, |b| format!("{b}%")),
        if bt::aap_service_addresses().contains(&mac) {
            "yes"
        } else {
            "no"
        },
    ));
    mac
}

pub fn run() {
    unsafe {
        let kind = GetFileType(GetStdHandle(STD_OUTPUT_HANDLE));
        if kind != FILE_TYPE_DISK && kind != FILE_TYPE_PIPE {
            AllocConsole();
        }
    }
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(utc_timestamp)
        .unwrap_or_default();
    let path = std::env::var("LOCALAPPDATA").ok().map(|la| {
        let _ = std::fs::create_dir_all(format!("{la}\\podatui"));
        let stamp: String = started
            .chars()
            .take(19)
            .filter(|c| c.is_ascii_digit() || *c == 'T')
            .collect();
        format!("{la}\\podatui\\probe-{stamp}Z.txt")
    });
    let file = path.as_ref().and_then(|p| File::create(p).ok());
    let path = path.filter(|_| file.is_some());
    let mut o = Out {
        start: Instant::now(),
        file,
    };
    let mac = header(&mut o, &started, path.as_deref());
    let out = Arc::new(Mutex::new(o));

    {
        let (adv, status) = (out.clone(), out.clone());
        thread::spawn(move || {
            le::watch_nearby(
                move |s: le::Sighting| {
                    if has_record(&s.data) {
                        adv.lock().unwrap().event(Event::Adv {
                            addr: s.addr,
                            public: s.public,
                            rssi: s.rssi,
                            data: s.data,
                        });
                    }
                },
                || true,
                move |s| status.lock().unwrap().event(Event::Watcher(s.into())),
            );
        });
    }
    {
        let out = out.clone();
        thread::spawn(move || {
            volume::init();
            let (mut render, mut acl) = (None, None);
            for tick in (0..ACL_EVERY).cycle() {
                let playing = volume::any_render_active();
                if render != Some(playing) {
                    render = Some(playing);
                    out.lock().unwrap().event(Event::Render(playing));
                }
                if mac != 0 && tick == 0 {
                    let up = bt::is_connected(mac);
                    if acl != Some(up) {
                        acl = Some(up);
                        out.lock().unwrap().event(Event::Acl(up));
                    }
                }
                thread::sleep(POLL);
            }
        });
    }

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let label = line.trim();
        if !label.is_empty() {
            out.lock().unwrap().event(Event::Note(label.to_string()));
        }
    }
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

#[cfg(test)]
mod tests {
    use super::{guid, has_record};

    #[test]
    fn logs_proximity_and_nearby_action_records_only() {
        assert!(has_record(&[0x07, 0x02, 0x01, 0x02]));
        assert!(has_record(&[0x10, 0x01, 0x00, 0x0F, 0x01, 0x00]));
        assert!(has_record(&[0x07, 0x19, 0x01]));
        assert!(!has_record(&[0x10, 0x05, 0x01, 0x18, 0x00, 0x00, 0x00]));
        assert!(!has_record(&[0x12, 0x02, 0x07, 0x07]));
        assert!(!has_record(&[0x07]));
        assert!(!has_record(&[]));
    }

    #[test]
    fn guid_text() {
        assert_eq!(
            guid(0x0065cbfd_f3fc_53ff_8fa5_91bf2c35671f),
            "{0065CBFD-F3FC-53FF-8FA5-91BF2C35671F}"
        );
    }
}
