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
    Ok(timeout(CONNECT_TIMEOUT, socket.connect(target)).await??)
}
