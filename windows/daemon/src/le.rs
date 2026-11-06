//! BLE advertisement watcher: reports every Apple (0x004C) manufacturer record
//! heard over LE as a `Sighting`, so the daemon can spot the AirPods nearby (core
//! `proximity::parse`) and prompt "connect?" before it opens the AAP session.
//!
//! Scanning is PASSIVE (listen only, no scan requests) and only runs while
//! `should_scan` is true — i.e. while disconnected — so the 2.4 GHz radio never
//! contends with the AirPods' A2DP audio while you're listening (which caused
//! static spikes, esp. on combo cards with poor coexistence).
//!
//! A watcher dies with the radio (Bluetooth toggled off, sleep): the loop sees it
//! Aborted, or Stopped without our asking, and recreates it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
    BluetoothLEAdvertisementWatcherStatus as Status,
    BluetoothLEAdvertisementWatcherStoppedEventArgs, BluetoothLEManufacturerData,
    BluetoothLEScanningMode,
};
use windows::Devices::Bluetooth::{BluetoothAddressType, BluetoothError};
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::{Buffer, DataReader};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use crate::log;

const APPLE_COMPANY_ID: u16 = 0x004C;
const POLL: Duration = Duration::from_secs(2);
const RETRY: Duration = Duration::from_secs(3);

/// One Apple manufacturer record; `data` starts at the first type byte.
pub struct Sighting {
    pub addr: u64,
    pub public: bool,
    pub rssi: i16,
    pub data: Vec<u8>,
}

type OnSighting = Arc<dyn Fn(Sighting) + Send + Sync>;

/// Pass every Apple advertisement record to `on_sighting`, scanning only while
/// `should_scan()` is true. Blocks (keeps COM + the watcher alive), so run it on
/// its own thread.
pub fn watch_nearby(
    on_sighting: impl Fn(Sighting) + Send + Sync + 'static,
    should_scan: impl Fn() -> bool + Send + 'static,
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let on_sighting: OnSighting = Arc::new(on_sighting);
    let stop_error = Arc::new(Mutex::new(BluetoothError::Success));
    let mut watcher: Option<BluetoothLEAdvertisementWatcher> = None;
    let mut scanning = false;
    let mut restarts = 0u32;
    loop {
        let w = match &watcher {
            Some(w) => w.clone(),
            None => match setup(on_sighting.clone(), stop_error.clone()) {
                Ok(w) => {
                    scanning = false;
                    watcher = Some(w.clone());
                    w
                }
                Err(e) => {
                    log(&format!("ble: watcher setup failed: {e}"));
                    std::thread::sleep(RETRY);
                    continue;
                }
            },
        };
        let status = w.Status().unwrap_or(Status::Aborted);
        if status == Status::Aborted || (scanning && status == Status::Stopped) {
            // One line per outage: with the radio off every retry aborts again.
            if restarts == 0 {
                let what = if status == Status::Aborted {
                    "aborted"
                } else {
                    "stopped"
                };
                let err = *stop_error.lock().unwrap();
                log(&format!(
                    "ble: watcher {what} ({err:?}), recreating every {}s",
                    RETRY.as_secs()
                ));
            }
            restarts += 1;
            watcher = None;
            std::thread::sleep(RETRY);
            continue;
        }
        if restarts > 0 && status == Status::Started {
            log(&format!(
                "ble: watcher running again after {restarts} restart(s)"
            ));
            restarts = 0;
        }
        let want = should_scan();
        if want && !scanning {
            scanning = w.Start().is_ok();
        } else if !want && scanning {
            let _ = w.Stop();
            scanning = false;
        }
        std::thread::sleep(POLL);
    }
}

fn setup(
    on_sighting: OnSighting,
    stop_error: Arc<Mutex<BluetoothError>>,
) -> windows::core::Result<BluetoothLEAdvertisementWatcher> {
    let watcher = BluetoothLEAdvertisementWatcher::new()?;
    watcher.SetScanningMode(BluetoothLEScanningMode::Passive)?;
    let apple = BluetoothLEManufacturerData::Create(APPLE_COMPANY_ID, &Buffer::Create(0)?)?;
    watcher
        .AdvertisementFilter()?
        .Advertisement()?
        .ManufacturerData()?
        .Append(&apple)?;
    watcher.Received(&TypedEventHandler::new(
        move |_s: &Option<BluetoothLEAdvertisementWatcher>,
              args: &Option<BluetoothLEAdvertisementReceivedEventArgs>| {
            if let Some(args) = args.as_ref() {
                let _ = report(args, &on_sighting);
            }
            Ok(())
        },
    ))?;
    watcher.Stopped(&TypedEventHandler::new(
        move |_s: &Option<BluetoothLEAdvertisementWatcher>,
              args: &Option<BluetoothLEAdvertisementWatcherStoppedEventArgs>| {
            if let Some(args) = args.as_ref() {
                *stop_error.lock().unwrap() = args.Error()?;
            }
            Ok(())
        },
    ))?;
    Ok(watcher)
}

fn report(
    args: &BluetoothLEAdvertisementReceivedEventArgs,
    on_sighting: &OnSighting,
) -> windows::core::Result<()> {
    let addr = args.BluetoothAddress()?;
    let public = args.BluetoothAddressType()? == BluetoothAddressType::Public;
    let rssi = args.RawSignalStrengthInDBm()?;
    for md in args.Advertisement()?.ManufacturerData()? {
        if md.CompanyId()? != APPLE_COMPANY_ID {
            continue;
        }
        let buf = md.Data()?;
        let mut data = vec![0u8; buf.Length()? as usize];
        DataReader::FromBuffer(&buf)?.ReadBytes(&mut data)?;
        on_sighting(Sighting {
            addr,
            public,
            rssi,
            data,
        });
    }
    Ok(())
}
