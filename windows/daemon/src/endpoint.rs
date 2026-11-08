//! Core Audio endpoints that belong to the AirPods, joined on the Bluetooth container ID.

use windows::core::GUID;
use windows::Win32::Media::Audio::{
    eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE, DEVICE_STATEMASK_ALL,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

const PKEY_DEVICE_CONTAINER_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x8c7ed206_3f8a_4827_b3ab_ae9e1faefc6c),
    pid: 2,
};
const VT_CLSID: u16 = 72;

/// True when a render endpoint of our container is ACTIVE. False on any COM failure.
pub fn render_active(container: u128) -> bool {
    unsafe {
        let Ok(enumerator) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return false;
        };
        let Ok(devices) =
            enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE(DEVICE_STATEMASK_ALL))
        else {
            return false;
        };
        for i in 0..devices.GetCount().unwrap_or(0) {
            let Ok(device) = devices.Item(i) else {
                continue;
            };
            let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
                continue;
            };
            let Ok(value) = store.GetValue(&PKEY_DEVICE_CONTAINER_ID) else {
                continue;
            };
            let raw = &value.as_raw().Anonymous.Anonymous;
            if raw.vt != VT_CLSID || raw.Anonymous.puuid.is_null() {
                continue;
            }
            let g = &*raw.Anonymous.puuid;
            let id = GUID::from_values(g.data1, g.data2, g.data3, g.data4);
            if id.to_u128() == container
                && device.GetState().is_ok_and(|s| s == DEVICE_STATE_ACTIVE)
            {
                return true;
            }
        }
        false
    }
}
