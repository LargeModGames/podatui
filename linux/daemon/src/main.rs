//! podatuid for Linux: opens the AirPods' AAP channel over BlueZ and prints what they report.
//!
//! The connect and handshake sequence follows LibrePods' linux-rust client
//! (https://github.com/kavishdevar/librepods, GPL-3.0).

use bluer::l2cap::{SeqPacket, Socket, SocketAddr};
use bluer::{Adapter, Address, AddressType, Uuid};
use podatui_core::aap::{self, Battery, EarStatus};
use std::error::Error;
use std::time::Duration;
use tokio::time::{sleep, timeout};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const AAP_SERVICE: Uuid = Uuid::from_u128(0x74ec2172_0bad_4d01_8f77_997b2be0722a);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SETUP_GAP: Duration = Duration::from_millis(300);

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    let addr = match std::env::args().nth(1) {
        Some(mac) => mac.parse()?,
        None => find_airpods(&adapter).await?,
    };

    println!("connecting to {addr} on PSM {:#06x}", aap::PSM_AACP);
    let link = connect(addr).await?;
    for packet in [
        &aap::HANDSHAKE[..],
        &aap::SET_FEATURES[..],
        &aap::REQUEST_NOTIFS[..],
    ] {
        link.send(packet).await?;
        sleep(SETUP_GAP).await;
    }
    println!("handshake sent");

    let mut buf = [0u8; 1024];
    loop {
        let n = link.recv(&mut buf).await?;
        if n == 0 {
            println!("AirPods closed the link");
            return Ok(());
        }
        match describe(&buf[..n]) {
            Some(line) => println!("{line}"),
            None => println!("? {}", hex(&buf[..n])),
        }
    }
}

/// The first connected device that offers the AAP service.
async fn find_airpods(adapter: &Adapter) -> Result<Address> {
    for addr in adapter.device_addresses().await? {
        let device = adapter.device(addr)?;
        if device.is_connected().await?
            && device
                .uuids()
                .await?
                .unwrap_or_default()
                .contains(&AAP_SERVICE)
        {
            return Ok(addr);
        }
    }
    Err("no connected AirPods found; connect them in bluetoothctl first".into())
}

async fn connect(addr: Address) -> Result<SeqPacket> {
    let target = SocketAddr::new(addr, AddressType::BrEdr, aap::PSM_AACP);
    let socket = Socket::new_seq_packet()?;
    Ok(timeout(CONNECT_TIMEOUT, socket.connect(target)).await??)
}

/// One human-readable line for the packets this slice understands.
fn describe(data: &[u8]) -> Option<String> {
    if let Some(b) = aap::parse_battery(data) {
        return Some(battery_line(&b));
    }
    if let Some((a, b)) = aap::parse_ear_detection(data) {
        return Some(format!("ear: {} / {}", ear_name(a), ear_name(b)));
    }
    if let Some(mode) = aap::parse_anc_mode(data) {
        return Some(format!("listening mode: {}", aap::anc_name(mode)));
    }
    if let Some(status) = aap::parse_conversational_awareness(data) {
        return Some(format!("conversation: {status}"));
    }
    if let Some((model, firmware, _serial)) = aap::parse_metadata(data) {
        return Some(format!("model {model}, firmware {firmware}"));
    }
    None
}

fn battery_line(b: &Battery) -> String {
    let pct = |level: Option<u8>, charging: bool| match level {
        Some(l) if charging => format!("{l}%+"),
        Some(l) => format!("{l}%"),
        None => "-".into(),
    };
    format!(
        "battery: L {}  R {}  case {}",
        pct(b.left, b.left_charging),
        pct(b.right, b.right_charging),
        pct(b.case, b.case_charging)
    )
}

fn ear_name(s: EarStatus) -> &'static str {
    match s {
        EarStatus::InEar => "in ear",
        EarStatus::OutOfEar => "out",
        EarStatus::InCase => "in case",
        EarStatus::Disconnected => "gone",
        EarStatus::Transitional => "moving",
    }
}

fn hex(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_packet() {
        let p = [
            0x04, 0x00, 0x04, 0x00, 0x04, 0x00, 0x03, 0x02, 0x01, 0x55, 0x02, 0x01, 0x04, 0x01,
            0x50, 0x01, 0x01, 0x08, 0x01, 0x20, 0x04, 0x01,
        ];
        assert_eq!(describe(&p).unwrap(), "battery: L 80%+  R 85%  case -");
    }

    #[test]
    fn ear_packet() {
        let p = [0x04, 0x00, 0x04, 0x00, 0x06, 0x00, 0x00, 0x02];
        assert_eq!(describe(&p).unwrap(), "ear: in ear / in case");
    }

    #[test]
    fn listening_mode_packet() {
        let p = [
            0x04, 0x00, 0x04, 0x00, 0x09, 0x00, 0x0D, 0x04, 0x00, 0x00, 0x00,
        ];
        assert_eq!(describe(&p).unwrap(), "listening mode: Adaptive");
    }

    #[test]
    fn unknown_packet() {
        assert_eq!(describe(&[0x04, 0x00, 0x04, 0x00, 0x2B, 0x00]), None);
        assert_eq!(hex(&[0x04, 0xab]), "04 ab");
    }
}
