//! Heart-rate tracking database — a dedicated SQLite file,
//! `%LOCALAPPDATA%\podatui\heart-rate.sqlite3`, kept apart from everything else.
//!
//! Schema (v1):
//! - `sessions`: one row per monitoring run — the AirPods' address, when the first
//!   reading arrived, and when monitoring stopped (HR switched off or the buds
//!   disconnected; NULL while open or if the daemon died mid-session).
//! - `samples`: every validated reading — its session, Unix time in ms, BPM, and
//!   the sensor's confidence byte (payload[2]; 160..240 once locked).
//!
//! Writes go through a channel to one writer thread, so the AAP receive loop never
//! waits on the disk. If the database can't be opened, tracking is disabled and
//! logged once; heart rate itself keeps working.

use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::hr::HrSample;

const SCHEMA_VERSION: i32 = 1;

enum Event {
    Sample {
        device: u64,
        ts_ms: i64,
        bpm: u16,
        confidence: u8,
    },
    EndSession {
        ts_ms: i64,
    },
}

/// Handle to the writer thread; cheap to clone into `Ctx`.
#[derive(Clone)]
pub struct HrDb {
    tx: Option<Sender<Event>>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl HrDb {
    /// Open (or create) the database and start the writer thread. `log` is the
    /// daemon's logger, so failures land in daemon.log.
    pub fn open(log: fn(&str)) -> HrDb {
        let path = match std::env::var("LOCALAPPDATA") {
            Ok(la) => std::path::Path::new(&la)
                .join("podatui")
                .join("heart-rate.sqlite3"),
            Err(_) => {
                log("hrdb: LOCALAPPDATA not set — heart-rate tracking disabled");
                return HrDb { tx: None };
            }
        };
        let conn = match Connection::open(&path).and_then(|c| init(&c).map(|_| c)) {
            Ok(c) => c,
            Err(e) => {
                log(&format!(
                    "hrdb: cannot open {} ({e}) — heart-rate tracking disabled",
                    path.display()
                ));
                return HrDb { tx: None };
            }
        };
        log(&format!("hrdb: tracking heart rate in {}", path.display()));
        let (tx, rx) = mpsc::channel::<Event>();
        thread::spawn(move || writer(conn, rx, log));
        HrDb { tx: Some(tx) }
    }

    /// Record one validated reading from the AirPods at `device` (their address).
    pub fn sample(&self, device: u64, s: HrSample) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Event::Sample {
                device,
                ts_ms: now_ms(),
                bpm: s.bpm,
                confidence: s.confidence,
            });
        }
    }

    /// Close the open session, if any (HR switched off, or the buds went away).
    pub fn end_session(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Event::EndSession { ts_ms: now_ms() });
        }
    }
}

fn init(c: &Connection) -> rusqlite::Result<()> {
    c.pragma_update(None, "journal_mode", "WAL")?;
    c.pragma_update(None, "synchronous", "NORMAL")?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
             id          INTEGER PRIMARY KEY,
             device      TEXT    NOT NULL,          -- AirPods address, aa:bb:cc:dd:ee:ff
             started_at  INTEGER NOT NULL,          -- Unix ms of the first reading
             ended_at    INTEGER                    -- Unix ms; NULL if open / daemon died
         );
         CREATE TABLE IF NOT EXISTS samples (
             id          INTEGER PRIMARY KEY,
             session_id  INTEGER NOT NULL REFERENCES sessions(id),
             ts          INTEGER NOT NULL,          -- Unix ms
             bpm         INTEGER NOT NULL,
             confidence  INTEGER NOT NULL           -- payload[2]; >= 128 = locked
         );
         CREATE INDEX IF NOT EXISTS samples_ts      ON samples(ts);
         CREATE INDEX IF NOT EXISTS samples_session ON samples(session_id);",
    )?;
    c.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn format_mac(mac: u64) -> String {
    (0..6)
        .rev()
        .map(|i| format!("{:02x}", (mac >> (i * 8)) & 0xff))
        .collect::<Vec<_>>()
        .join(":")
}

fn writer(conn: Connection, rx: mpsc::Receiver<Event>, log: fn(&str)) {
    // (session id, device) of the open session.
    let mut open: Option<(i64, u64)> = None;
    for ev in rx {
        let res: rusqlite::Result<()> = (|| {
            match ev {
                Event::Sample {
                    device,
                    ts_ms,
                    bpm,
                    confidence,
                } => {
                    // A different pair of buds starts its own session.
                    if let Some((id, dev)) = open {
                        if dev != device {
                            conn.execute(
                                "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                                params![ts_ms, id],
                            )?;
                            open = None;
                        }
                    }
                    let session = match open {
                        Some((id, _)) => id,
                        None => {
                            conn.execute(
                                "INSERT INTO sessions (device, started_at) VALUES (?1, ?2)",
                                params![format_mac(device), ts_ms],
                            )?;
                            let id = conn.last_insert_rowid();
                            open = Some((id, device));
                            id
                        }
                    };
                    conn.execute(
                        "INSERT INTO samples (session_id, ts, bpm, confidence) VALUES (?1, ?2, ?3, ?4)",
                        params![session, ts_ms, bpm, confidence],
                    )?;
                }
                Event::EndSession { ts_ms } => {
                    if let Some((id, _)) = open.take() {
                        conn.execute(
                            "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                            params![ts_ms, id],
                        )?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = res {
            log(&format!("hrdb: write failed: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{format_mac, init, writer, Event};
    use rusqlite::{Connection, OpenFlags};
    use std::sync::mpsc;

    #[test]
    fn format_mac_is_lowercase_colon_separated_48_bits() {
        assert_eq!(format_mac(0), "00:00:00:00:00:00");
        assert_eq!(format_mac(0x0011_2233_4455), "00:11:22:33:44:55");
        assert_eq!(format_mac(0xAABB_CCDD_EEFF), "aa:bb:cc:dd:ee:ff");
        assert_eq!(format_mac(0xFFFF_0000_0000_0001), "00:00:00:00:00:01");
    }

    #[test]
    fn writer_opens_switches_and_ends_sessions() {
        let uri = "file:hrdb_writer_test?mode=memory&cache=shared";
        let flags = OpenFlags::default() | OpenFlags::SQLITE_OPEN_URI;
        let keep = Connection::open_with_flags(uri, flags).unwrap();
        let conn = Connection::open_with_flags(uri, flags).unwrap();
        init(&conn).unwrap();

        let (a, b) = (0xAABB_CCDD_EEFF, 0x0011_2233_4455);
        let sample = |device, ts_ms, bpm| Event::Sample {
            device,
            ts_ms,
            bpm,
            confidence: 200,
        };
        let (tx, rx) = mpsc::channel();
        for ev in [
            sample(a, 1000, 60),
            sample(a, 2000, 61),
            sample(b, 3000, 70),
            Event::EndSession { ts_ms: 4000 },
            Event::EndSession { ts_ms: 4500 },
            sample(a, 5000, 80),
        ] {
            tx.send(ev).unwrap();
        }
        drop(tx);
        writer(conn, rx, |m| panic!("{m}"));

        let sessions: Vec<(i64, String, i64, Option<i64>)> = keep
            .prepare("SELECT id, device, started_at, ended_at FROM sessions ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            sessions,
            [
                (1, "aa:bb:cc:dd:ee:ff".into(), 1000, Some(3000)),
                (2, "00:11:22:33:44:55".into(), 3000, Some(4000)),
                (3, "aa:bb:cc:dd:ee:ff".into(), 5000, None),
            ]
        );
        let samples: Vec<(i64, i64, i64, i64)> = keep
            .prepare("SELECT session_id, ts, bpm, confidence FROM samples ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            samples,
            [
                (1, 1000, 60, 200),
                (1, 2000, 61, 200),
                (2, 3000, 70, 200),
                (3, 5000, 80, 200),
            ]
        );
    }
}
