use crate::device::dword;
use serde_json::{json, Value};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::{
    core::GUID,
    Win32::{Foundation::HANDLE, NetworkManagement::WiFi::*},
};

// This opcode follows the two secondary-STA queries in newer Windows SDKs.
const REALTIME_CONNECTION_QUALITY: i32 = 19;

struct Wlan(HANDLE);
impl Drop for Wlan {
    fn drop(&mut self) {
        unsafe {
            WlanCloseHandle(self.0, null());
        }
    }
}

impl Wlan {
    fn query(&self, guid: &GUID, opcode: i32) -> Result<Vec<u8>, u32> {
        let mut data = null_mut();
        let mut length = 0;
        let code = unsafe {
            WlanQueryInterface(
                self.0,
                guid,
                opcode,
                null(),
                &mut length,
                &mut data,
                null_mut(),
            )
        };
        if code != 0 {
            return Err(code);
        }
        if data.is_null() {
            return Ok(Vec::new());
        }
        let bytes = if length <= 65536 {
            // The returned buffer remains owned by WLAN until it is freed below.
            unsafe { std::slice::from_raw_parts(data.cast(), length as usize).to_vec() }
        } else {
            Vec::new()
        };
        unsafe {
            WlanFreeMemory(data);
        }
        Ok(bytes)
    }
}

fn phy(mode: u32) -> Option<&'static str> {
    match mode {
        1 => Some("802.11 FHSS"),
        2 => Some("802.11 DSSS"),
        4 => Some("802.11a"),
        5 => Some("802.11b"),
        6 => Some("802.11g"),
        7 => Some("802.11n"),
        8 => Some("802.11ac"),
        9 => Some("802.11ad"),
        10 => Some("802.11ax"),
        11 => Some("802.11be"),
        _ => None,
    }
}
fn frequency(raw: u32) -> Option<u32> {
    let mhz = if raw > 100000 { raw / 1000 } else { raw };
    (2000..=7100).contains(&mhz).then_some(mhz)
}
fn describe(code: u32) -> String {
    match code {
        5 => "Windows denied access to Wi-Fi details. Check Location and desktop-app access in Windows settings.".into(),
        1062 => "Windows WLAN AutoConfig is unavailable or stopped.".into(),
        1168 | 5023 => "The Wi-Fi interface is disconnected or a connection is no longer active.".into(),
        _ => format!("Windows WLAN API error {code}. The Wi-Fi details could not be read.")
    }
}

#[allow(non_upper_case_globals)] // Match the Windows SDK WLAN state constants.
pub fn connections() -> Result<Vec<Value>, String> {
    let mut handle = null_mut();
    let mut version = 0;
    let code = unsafe { WlanOpenHandle(WLAN_API_VERSION, null(), &mut version, &mut handle) };
    if code != 0 {
        return Err(describe(code));
    }
    let wlan = Wlan(handle);
    let mut raw = null_mut();
    let code = unsafe { WlanEnumInterfaces(wlan.0, null(), &mut raw) };
    if code != 0 {
        return Err(describe(code));
    }
    if raw.is_null() {
        return Ok(Vec::new());
    }
    // The API returns a variable-length array preceded by a count.
    let interfaces = unsafe {
        let count = (*raw).dwNumberOfItems.min(128) as usize;
        let first = std::ptr::addr_of!((*raw).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>();
        let values = std::slice::from_raw_parts(first, count).to_vec();
        WlanFreeMemory(raw.cast());
        values
    };

    let mut rows = Vec::with_capacity(interfaces.len());
    for interface in interfaces {
        let name = String::from_utf16_lossy(
            &interface.strInterfaceDescription[..interface
                .strInterfaceDescription
                .iter()
                .position(|&ch| ch == 0)
                .unwrap_or(interface.strInterfaceDescription.len())],
        );
        let connected = matches!(
            interface.isState,
            wlan_interface_state_connected | wlan_interface_state_ad_hoc_network_formed
        );
        let state = match interface.isState {
            wlan_interface_state_connected => "Connected",
            wlan_interface_state_ad_hoc_network_formed => "Ad hoc network",
            wlan_interface_state_not_ready => "Not ready",
            wlan_interface_state_discovering => "Discovering",
            wlan_interface_state_associating | wlan_interface_state_authenticating => "Connecting",
            wlan_interface_state_disconnecting => "Disconnecting",
            _ => "Disconnected",
        };
        let mut row = json!({"name":name,"state":state,"source":"Windows WLAN API"});
        if connected {
            let guid = &interface.InterfaceGuid;
            let channel = wlan
                .query(guid, wlan_intf_opcode_channel_number)
                .ok()
                .and_then(|data| dword(&data, 0))
                .filter(|&v| v > 0);
            row["channel"] = json!(channel);
            let realtime = wlan
                .query(guid, REALTIME_CONNECTION_QUALITY)
                .ok()
                .filter(|data| data.len() >= 24);
            if let Some(data) = realtime {
                row["phy"] = json!(dword(&data, 0).and_then(phy));
                row["signal"] = json!(dword(&data, 4).filter(|&v| v <= 100));
                row["rxMbps"] = json!(dword(&data, 8).map(|v| f64::from(v) / 1000.));
                row["txMbps"] = json!(dword(&data, 12).map(|v| f64::from(v) / 1000.));
                if dword(&data, 20).is_some_and(|v| v > 0) {
                    row["frequencyMHz"] = json!(dword(&data, 28).and_then(frequency));
                    row["rssi"] = json!(dword(&data, 36)
                        .map(|v| v as i32)
                        .filter(|v| (-128..=0).contains(v)));
                }
            } else {
                match wlan.query(guid, wlan_intf_opcode_current_connection) {
                    Ok(data) if data.len() >= size_of::<WLAN_CONNECTION_ATTRIBUTES>() => {
                        // WLAN_CONNECTION_ATTRIBUTES consists of scalar integers and fixed arrays.
                        let connection = unsafe {
                            data.as_ptr()
                                .cast::<WLAN_CONNECTION_ATTRIBUTES>()
                                .read_unaligned()
                        };
                        let info = connection.wlanAssociationAttributes;
                        row["phy"] = json!(phy(info.dot11PhyType as u32));
                        row["signal"] = json!(
                            (info.wlanSignalQuality <= 100).then_some(info.wlanSignalQuality)
                        );
                        row["rxMbps"] = json!(f64::from(info.ulRxRate) / 1000.);
                        row["txMbps"] = json!(f64::from(info.ulTxRate) / 1000.);
                    }
                    Err(code) => {
                        row["unavailableReason"] = json!(describe(code));
                    }
                    _ => {}
                }
            }
            if row["rssi"].is_null() {
                row["rssi"] = json!(wlan
                    .query(guid, wlan_intf_opcode_rssi)
                    .ok()
                    .and_then(|data| dword(&data, 0))
                    .map(|v| v as i32)
                    .filter(|v| (-128..=0).contains(v)));
            }
        }
        rows.push(row);
    }
    Ok(rows)
}
