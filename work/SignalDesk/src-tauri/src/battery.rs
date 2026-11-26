use crate::device::{dword, interfaces, request, Device};
use serde_json::{json, Value};
use std::{
    mem::{offset_of, size_of, zeroed},
    ptr::null,
};
use windows_sys::{
    core::GUID,
    Win32::{Foundation::GENERIC_READ, System::Power::*},
};

const BATTERY_INTERFACE: GUID = GUID::from_u128(0x72631e54_78a4_11d0_bcf7_00aa00b7b32a);

fn information(
    device: &Device,
    tag: u32,
    level: BATTERY_QUERY_INFORMATION_LEVEL,
    size: usize,
) -> Option<Vec<u8>> {
    device
        .query(
            IOCTL_BATTERY_QUERY_INFORMATION,
            &request(&[tag, level as u32, 0]),
            size,
        )
        .ok()
}
fn text(device: &Device, tag: u32, level: BATTERY_QUERY_INFORMATION_LEVEL) -> Option<String> {
    let bytes = information(device, tag, level, 512)?;
    let name = String::from_utf16_lossy(
        &bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|v| *v != 0)
            .collect::<Vec<_>>(),
    );
    (!name.trim().is_empty()).then(|| name.trim().to_owned())
}
fn capacity(value: Option<u32>) -> Option<u32> {
    value.filter(|&v| v < BATTERY_UNKNOWN_CAPACITY)
}

pub fn packs() -> Result<Vec<Value>, String> {
    let paths = interfaces(&BATTERY_INTERFACE).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut failure = None;
    for path in paths {
        let device = match Device::open(&path, GENERIC_READ) {
            Ok(device) => device,
            Err(e) => {
                failure = Some(e.to_string());
                continue;
            }
        };
        let tag = match device.query(IOCTL_BATTERY_QUERY_TAG, &0u32.to_le_bytes(), 4) {
            Ok(bytes) => match dword(&bytes, 0).filter(|&v| v != 0) {
                Some(tag) => tag,
                None => continue,
            },
            Err(e) if matches!(e.raw_os_error(), Some(2 | 1167 | 1460)) => continue,
            Err(e) => {
                failure = Some(e.to_string());
                continue;
            }
        };
        let Some(data) = information(
            &device,
            tag,
            BatteryInformation,
            size_of::<BATTERY_INFORMATION>(),
        ) else {
            continue;
        };
        let Some(flags) = dword(&data, offset_of!(BATTERY_INFORMATION, Capabilities)) else {
            continue;
        };
        if flags & BATTERY_SYSTEM_BATTERY == 0 {
            continue;
        }
        let relative = flags & BATTERY_CAPACITY_RELATIVE != 0;
        let status = device
            .query(
                IOCTL_BATTERY_QUERY_STATUS,
                &request(&[tag, 0, 0, 0, u32::MAX]),
                size_of::<BATTERY_STATUS>(),
            )
            .unwrap_or_default();
        let rate = dword(&status, offset_of!(BATTERY_STATUS, Rate))
            .filter(|&v| v != BATTERY_UNKNOWN_RATE)
            .map(|v| v as i32);
        let remaining = information(&device, tag, BatteryEstimatedTime, 4)
            .and_then(|bytes| dword(&bytes, 0))
            .filter(|&v| v > 0 && v != BATTERY_UNKNOWN_TIME);
        results.push(json!({
            "manufacturer": text(&device, tag, BatteryManufactureName),
            "model": text(&device, tag, BatteryDeviceName),
            "design": capacity(dword(&data, offset_of!(BATTERY_INFORMATION, DesignedCapacity))).filter(|&v| v > 0),
            "full": capacity(dword(&data, offset_of!(BATTERY_INFORMATION, FullChargedCapacity))).filter(|&v| v > 0),
            "remainingCapacity": capacity(dword(&status, offset_of!(BATTERY_STATUS, Capacity))),
            "cycles": dword(&data, offset_of!(BATTERY_INFORMATION, CycleCount)).filter(|&v| v > 0 && v < u32::MAX),
            "charge": if relative { None } else { rate.filter(|&v| v > 0) },
            "discharge": if relative { None } else { rate.filter(|&v| v < 0).map(i32::unsigned_abs) },
            "remaining": remaining,
            "discharging": dword(&status, offset_of!(BATTERY_STATUS, PowerState)).map(|p| p & BATTERY_DISCHARGING != 0),
            "unit": if relative { "Relative units" } else { "mWh" },
            "source": "Windows battery driver"
        }));
    }
    if results.is_empty() {
        if let Some(error) = failure {
            return Err(error);
        }
    }
    Ok(results)
}

pub fn remaining_seconds(details: &[Value]) -> Option<(u64, &'static str)> {
    let [pack] = details else {
        return None;
    };
    if pack["discharging"] == false {
        return None;
    }
    if let Some(sec) = pack["remaining"].as_u64().filter(|&t| t > 0) {
        return Some((sec, "Battery driver estimate"));
    }
    if pack["unit"] != "mWh" {
        return None;
    }
    let energy = pack["remainingCapacity"].as_u64()?;
    let rate = pack["discharge"].as_u64().filter(|&v| v > 0)?;
    let sec = energy.checked_mul(3600)? / rate;
    (sec > 0).then_some((sec, "Estimated from current discharge rate"))
}

pub fn system_state() -> Option<SYSTEM_BATTERY_STATE> {
    let mut state: SYSTEM_BATTERY_STATE = unsafe { zeroed() };
    let result = unsafe {
        CallNtPowerInformation(
            SystemBatteryState,
            null(),
            0,
            (&mut state as *mut SYSTEM_BATTERY_STATE).cast(),
            size_of::<SYSTEM_BATTERY_STATE>() as u32,
        )
    };
    (result == 0).then_some(state)
}

pub fn aggregate() -> Option<Value> {
    use ::windows::Devices::Power::Battery;
    use ::windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    // This worker is dedicated to read-only telemetry and has no UI apartment.
    if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
        return None;
    }
    let _apartment = Apartment;
    let report = Battery::AggregateBattery().ok()?.GetReport().ok()?;
    let design = report
        .DesignCapacityInMilliwattHours()
        .ok()
        .and_then(|v| v.Value().ok())
        .filter(|&v| v > 0);
    let full = report
        .FullChargeCapacityInMilliwattHours()
        .ok()
        .and_then(|v| v.Value().ok())
        .filter(|&v| v > 0);
    let remaining = report
        .RemainingCapacityInMilliwattHours()
        .ok()
        .and_then(|v| v.Value().ok())
        .filter(|&v| v >= 0);
    if design.is_none() && full.is_none() && remaining.is_none() {
        return None;
    }
    let rate = report
        .ChargeRateInMilliwatts()
        .ok()
        .and_then(|v| v.Value().ok())
        .filter(|&v| v != i32::MIN);
    let discharging = rate.map(|v| v < 0);
    Some(
        json!({"model":"System battery","design":design,"full":full,"remainingCapacity":remaining,
        "charge":rate.filter(|&r|r>0),"discharge":rate.filter(|&r|r<0).map(i32::unsigned_abs),
        "discharging":discharging,"unit":"mWh","source":"Windows battery report"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_requires_discharge_and_real_units() {
        let pack =
            json!({"unit":"mWh","remainingCapacity":2000,"discharge":1000,"discharging":true});
        assert_eq!(remaining_seconds(&[pack.clone()]).unwrap().0, 7200);
        assert!(remaining_seconds(&[pack.clone(), pack]).is_none());
        assert!(remaining_seconds(&[
            json!({"unit":"Relative units","remainingCapacity":100,"discharge":2})
        ])
        .is_none());
        assert!(remaining_seconds(&[json!({"remaining":300,"discharging":false})]).is_none());
    }
}
