//! podatui: terminal UI for AirPods on Linux.

mod app;
mod device;
mod ui;

use app::{App, LinkEvent};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

const RETRY: Duration = Duration::from_secs(3);

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
    to_link: UnboundedSender<Vec<u8>>,
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

/// Keep the AAP link open: forward packets to the UI and commands to the AirPods,
/// and reconnect after a drop. Returns when the UI is gone.
async fn link(events: mpsc::Sender<LinkEvent>, mut commands: UnboundedReceiver<Vec<u8>>) {
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
                let mut buf = [0u8; podatuid::RECV_MTU as usize];
                loop {
                    tokio::select! {
                        r = airpods.link.recv(&mut buf) => match r {
                            Ok(0) => break "AirPods closed the link".to_string(),
                            Ok(n) => {
                                if events.send(LinkEvent::Packet(buf[..n].to_vec())).is_err() {
                                    return;
                                }
                            }
                            Err(e) => break e.to_string(),
                        },
                        Some(cmd) = commands.recv() => {
                            if let Err(e) = airpods.link.send(&cmd).await {
                                break e.to_string();
                            }
                        }
                    }
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
