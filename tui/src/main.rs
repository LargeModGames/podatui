//! podatui: terminal UI for AirPods on Linux.

mod app;
mod device;
mod ui;

use app::{App, LinkCommand, LinkEvent};
use podatui_core::hr::RtBuddyHeartRateDecoder;
use podatuid::Airpods;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::{sleep_until, Instant};

const RETRY: Duration = Duration::from_secs(3);
/// How long each heart-rate attempt waits for a first reading, and how many attempts
/// to make; the stream rarely starts on the first try (same as the Windows daemon).
const HR_WAIT: Duration = Duration::from_secs(12);
const HR_ATTEMPTS: u32 = 4;

fn main() -> std::io::Result<()> {
    let (events, from_link) = mpsc::channel();
    let (to_link, commands) = unbounded_channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(link(events, commands));
    });

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, from_link, to_link);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    from_link: mpsc::Receiver<LinkEvent>,
    to_link: UnboundedSender<LinkCommand>,
) -> std::io::Result<()> {
    let mut app = App::default();
    while !app.quit {
        while let Ok(e) = from_link.try_recv() {
            app.on_link(e);
        }
        let size = terminal.size()?;
        app.compact = ui::is_compact(size.width, size.height);
        terminal.draw(|f| ui::draw(f, &app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if let Some(cmd) = app.key(key.code) {
                        let _ = to_link.send(cmd);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Keep the AAP link open and reconnect after a drop. Returns when the UI is gone.
async fn link(events: mpsc::Sender<LinkEvent>, mut commands: UnboundedReceiver<LinkCommand>) {
    loop {
        let why = match podatuid::open(None).await {
            Ok(airpods) => {
                let connected = LinkEvent::Connected {
                    name: airpods.name.clone(),
                    addr: airpods.addr.to_string(),
                };
                if events.send(connected).is_err() {
                    return;
                }
                match session(&airpods, &events, &mut commands).await {
                    Some(why) => why,
                    None => return,
                }
            }
            Err(e) => e.to_string(),
        };
        if events.send(LinkEvent::Down(why)).is_err() {
            return;
        }
        while commands.try_recv().is_ok() {}
        tokio::time::sleep(RETRY).await;
    }
}

/// Serve one open link: packets and heart-rate readings to the UI, commands to the
/// AirPods. Returns why the link dropped, or `None` once the UI is gone.
async fn session(
    airpods: &Airpods,
    events: &mpsc::Sender<LinkEvent>,
    commands: &mut UnboundedReceiver<LinkCommand>,
) -> Option<String> {
    let link = &airpods.link;
    let mut buf = [0u8; podatuid::RECV_MTU as usize];
    let mut decoder = RtBuddyHeartRateDecoder::new();
    let mut seq: u8 = 0;
    let mut hr_on = false;
    let mut attempts = 0;
    // Set while waiting for the first reading of a heart-rate attempt.
    let mut deadline: Option<Instant> = None;
    loop {
        let wait = async move {
            match deadline {
                Some(d) => sleep_until(d).await,
                None => std::future::pending().await,
            }
        };
        let result: podatuid::Result<()> = tokio::select! {
            r = link.recv(&mut buf) => match r {
                Ok(0) => return Some("AirPods closed the link".into()),
                Ok(n) => {
                    let p = &buf[..n];
                    if hr_on {
                        for sample in decoder.feed(p) {
                            deadline = None;
                            events.send(LinkEvent::Bpm(sample.bpm)).ok()?;
                        }
                    }
                    events.send(LinkEvent::Packet(p.to_vec())).ok()?;
                    Ok(())
                }
                Err(e) => return Some(e.to_string()),
            },
            Some(cmd) = commands.recv() => match cmd {
                LinkCommand::Send(c) => link.send(&c).await.map(drop).map_err(Into::into),
                LinkCommand::HeartRate(on) => {
                    hr_on = on;
                    attempts = 0;
                    decoder.reset();
                    seq = seq.wrapping_add(2);
                    let r = podatuid::heart_rate(link, seq, on).await;
                    deadline = on.then(|| Instant::now() + HR_WAIT);
                    r
                }
            },
            _ = wait => {
                attempts += 1;
                seq = seq.wrapping_add(2);
                if attempts >= HR_ATTEMPTS {
                    hr_on = false;
                    deadline = None;
                    events.send(LinkEvent::HeartRateFailed).ok()?;
                    podatuid::heart_rate(link, seq, false).await
                } else {
                    let r = podatuid::heart_rate(link, seq, true).await;
                    deadline = Some(Instant::now() + HR_WAIT);
                    r
                }
            }
        };
        if let Err(e) = result {
            return Some(e.to_string());
        }
    }
}
