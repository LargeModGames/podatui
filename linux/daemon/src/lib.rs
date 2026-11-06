//! BlueZ side of podatui on Linux: find the AirPods and open their AAP channel.
//!
//! The connect and handshake sequence follows LibrePods' linux-rust client
//! (https://github.com/kavishdevar/librepods, GPL-3.0).

use bluer::l2cap::{SeqPacket, Socket, SocketAddr};
use bluer::{Adapter, Address, AddressType, Uuid};
use podatui_core::aap;
use std::time::Duration;
use tokio::time::{sleep, timeout};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Incoming MTU to advertise. The AirPods only publish RTBuddy services whose descriptor
/// fits it, and the heart-rate ones (~920 bytes) do not fit the 672-byte default.
/// Android and the Windows driver use 1691 too (see windows/drivers/aap/L2cap.c).
pub const RECV_MTU: u16 = 1691;

const AAP_SERVICE: Uuid = Uuid::from_u128(0x74ec2172_0bad_4d01_8f77_997b2be0722a);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SETUP_GAP: Duration = Duration::from_millis(300);

/// Connected AirPods with their AAP channel open and set up.
pub struct Airpods {
    pub addr: Address,
    pub name: String,
    pub link: SeqPacket,
}

/// Open the AAP channel to `addr`, or to the first connected AirPods, and send the setup packets.
pub async fn open(addr: Option<Address>) -> Result<Airpods> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    let addr = match addr {
        Some(a) => a,
        None => find_airpods(&adapter).await?,
    };
    let name = adapter.device(addr)?.alias().await?;
    let link = connect(addr).await?;
    for packet in [
        &aap::HANDSHAKE[..],
        &aap::SET_FEATURES[..],
        &aap::REQUEST_NOTIFS[..],
    ] {
        link.send(packet).await?;
        sleep(SETUP_GAP).await;
    }
    Ok(Airpods { addr, name, link })
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
    socket.bind(SocketAddr::any_br_edr())?;
    socket.set_recv_mtu(RECV_MTU)?;
    Ok(timeout(CONNECT_TIMEOUT, socket.connect(target)).await??)
}

/// Start or stop the 1 Hz heart-rate stream (AirPods Pro 3). Start sends the AACP 1.3
/// session init, switches the sensor on (control 0x30) and sets the interval on both
/// heart-rate services, as the Windows daemon and LibrePods' Android client (PR #702) do.
/// `seq` is a request counter; this uses `seq` and `seq + 1`.
pub async fn heart_rate(link: &SeqPacket, seq: u8, on: bool) -> Result<()> {
    if on {
        for (packet, gap_ms) in [
            (&aap::HR_CONNECT_SERVICE_0[..], 180),
            (&aap::HR_CAPABILITIES_SERVICE_0[..], 220),
            (&aap::HR_CONNECT_SERVICE_4[..], 180),
            (&aap::HR_CAPABILITIES_SERVICE_4[..], 220),
            (&aap::HR_ENABLE[..], 120),
        ] {
            link.send(packet).await?;
            sleep(Duration::from_millis(gap_ms)).await;
        }
    }
    let period = if on { aap::PERIOD_HEART_RATE_US } else { 0 };
    link.send(&aap::hr_stream(seq, aap::STREAM_HEART_RATE, period))
        .await?;
    sleep(Duration::from_millis(120)).await;
    link.send(&aap::hr_stream(
        seq.wrapping_add(1),
        aap::STREAM_HEART_RATE_LEGACY,
        period,
    ))
    .await?;
    Ok(())
}
