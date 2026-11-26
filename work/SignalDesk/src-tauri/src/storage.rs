use crate::device::{dword, interfaces, request, word, Device};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    mem::{offset_of, size_of},
};
use windows_sys::Win32::{
    Foundation::GENERIC_READ,
    System::Ioctl::{
        StorageAdapterProtocolSpecificProperty, StorageDeviceProperty,
        StorageDeviceProtocolSpecificProperty, StorageDeviceTemperatureProperty,
        GUID_DEVINTERFACE_DISK, IOCTL_STORAGE_GET_DEVICE_NUMBER, IOCTL_STORAGE_PREDICT_FAILURE,
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_TEMPERATURE_DATA_DESCRIPTOR,
        STORAGE_TEMPERATURE_INFO,
    },
};

const NVME_HEALTH_PAGE: u32 = 2;
const NVME_HEALTH_SIZE: usize = 512;
const PROTOCOL_NVME: u32 = 3;
const NVME_LOG_PAGE: u32 = 2;
const PROTOCOL_HEADER_SIZE: usize = 40;
const DEVICE_BUS_ATA: u32 = 3;
const DEVICE_BUS_SATA: u32 = 11;
const DEVICE_BUS_NVME: u32 = 17;

fn bus_label(bus: u32) -> Option<&'static str> {
    match bus {
        1 => Some("SCSI"),
        DEVICE_BUS_ATA => Some("ATA"),
        7 => Some("USB"),
        8 => Some("RAID"),
        10 => Some("SAS"),
        DEVICE_BUS_SATA => Some("SATA"),
        12 => Some("SD"),
        13 => Some("MMC"),
        14 | 15 => Some("Virtual"),
        16 => Some("Storage Spaces"),
        DEVICE_BUS_NVME => Some("NVMe"),
        18 => Some("SCM"),
        19 => Some("UFS"),
        _ => None,
    }
}

fn property(device: &Device, property: i32, capacity: usize) -> Option<Vec<u8>> {
    device
        .query(
            IOCTL_STORAGE_QUERY_PROPERTY,
            &request(&[property as u32, 0, 0]),
            capacity,
        )
        .ok()
}
fn descriptor_string(data: &[u8], offset: usize) -> Option<String> {
    let index = usize::try_from(dword(data, offset)?).ok()?;
    if index == 0 {
        return None;
    }
    let bytes = data.get(index..)?.split(|&byte| byte == 0).next()?;
    let value = String::from_utf8_lossy(bytes);
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}
fn plausible_temp(celsius: f64) -> Option<f64> {
    (-60. ..=190.).contains(&celsius).then_some(celsius)
}
fn reported_temperature(device: &Device) -> Option<f64> {
    let data = property(device, StorageDeviceTemperatureProperty, 1024)?;
    let start = offset_of!(STORAGE_TEMPERATURE_DATA_DESCRIPTOR, TemperatureInfo);
    let count = word(
        &data,
        offset_of!(STORAGE_TEMPERATURE_DATA_DESCRIPTOR, InfoCount),
    )? as usize;
    data.get(start..)?
        .chunks_exact(size_of::<STORAGE_TEMPERATURE_INFO>())
        .take(count)
        .filter_map(|entry| {
            plausible_temp(
                word(entry, offset_of!(STORAGE_TEMPERATURE_INFO, Temperature))? as i16 as f64,
            )
        })
        .reduce(f64::max)
}
fn health_payload(output: &[u8]) -> Option<&[u8]> {
    if dword(output, 0)? < 48
        || dword(output, 4)? < 48
        || dword(output, 8)? != PROTOCOL_NVME
        || dword(output, 12)? != NVME_LOG_PAGE
    {
        return None;
    }
    let offset = dword(output, 24)? as usize;
    let length = dword(output, 28)? as usize;
    if offset < PROTOCOL_HEADER_SIZE || length < NVME_HEALTH_SIZE {
        return None;
    }
    let start = offset.checked_add(8)?;
    output.get(start..start.checked_add(NVME_HEALTH_SIZE)?)
}

fn nvme_reading(data: &[u8]) -> Option<Value> {
    if data.len() < NVME_HEALTH_SIZE || data.iter().all(|&byte| byte == 0xff) {
        return None;
    }
    let temperature = word(data, 1)
        .filter(|&v| v != 0 && v != u16::MAX)
        .and_then(|kelvin| plausible_temp(f64::from(kelvin) - 273.15));
    let lifetime = |offset| {
        data.get(offset..offset + 16)
            .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
            .map(u128::from_le_bytes)
    };
    // JSON numbers above JavaScript's safe integer range are returned as decimal strings.
    let count = |offset| {
        lifetime(offset).map(|v| {
            if v <= (1u128 << 53) - 1 {
                json!(v as u64)
            } else {
                json!(v.to_string())
            }
        })
    };
    Some(json!({
        "status": if data[0] == 0 { "Healthy" } else { "Warning" },
        "criticalWarning": data[0],
        "temperature": temperature,
        "wear": data[5],
        "spare": data[3],
        "powerCycles": count(112),
        "hours": count(128),
        "mediaErrors": count(160),
        "errorLogEntries": count(176),
        "source": "NVMe SMART"
    }))
}
fn nvme(device: &Device) -> Option<Value> {
    for property in [
        StorageDeviceProtocolSpecificProperty,
        StorageAdapterProtocolSpecificProperty,
    ] {
        let mut input = request(&[
            property as u32,
            0,
            PROTOCOL_NVME,
            NVME_LOG_PAGE,
            NVME_HEALTH_PAGE,
            0,
            PROTOCOL_HEADER_SIZE as u32,
            NVME_HEALTH_SIZE as u32,
            0,
            0,
            0,
            0,
        ]);
        input.resize(8 + PROTOCOL_HEADER_SIZE + NVME_HEALTH_SIZE, 0);
        if let Ok(output) = device.query(IOCTL_STORAGE_QUERY_PROPERTY, &input, input.len()) {
            if let Some(row) = health_payload(&output).and_then(nvme_reading) {
                return Some(row);
            }
        }
    }
    None
}
fn ata(device: &Device) -> Option<Value> {
    let buffer = device.query(IOCTL_STORAGE_PREDICT_FAILURE, &[], 516).ok()?;
    let failed = dword(&buffer, 0)? != 0;
    let mut row =
        json!({"status":if failed {"Warning"} else {"Healthy"}, "source":"Windows ATA SMART"});
    if let Some(data) = buffer.get(4..516) {
        // ATA SMART READ DATA has a two-byte revision, 30 twelve-byte attributes, and a checksum.
        if word(data, 0).is_some_and(|v| v != 0 && v != u16::MAX)
            && data.iter().fold(0u8, |sum, &v| sum.wrapping_add(v)) == 0
        {
            let attr = |id| {
                data[2..362]
                    .chunks_exact(12)
                    .find(|entry| entry[0] == id)
                    .map(|entry| &entry[5..11])
            };
            let raw = |id| {
                attr(id).map(|bytes| {
                    bytes
                        .iter()
                        .enumerate()
                        .map(|(i, v)| u64::from(*v) << (8 * i))
                        .sum::<u64>()
                })
            };
            row["startStop"] = json!(raw(4));
            row["powerCycles"] = json!(raw(12));
            row["ataHoursRaw"] = json!(raw(9));
            row["temperature"] =
                json!(attr(194).and_then(|bytes| plausible_temp(f64::from(bytes[0]))));
        }
    }
    Some(row)
}
fn read_drive(device: &Device, fallback_index: Option<u32>) -> Option<(u32, Value)> {
    let desc = property(device, StorageDeviceProperty, 4096)?;
    let bus = dword(&desc, 28)?;
    let index = device
        .query(IOCTL_STORAGE_GET_DEVICE_NUMBER, &[], 12)
        .ok()
        .and_then(|data| dword(&data, 4))
        .or(fallback_index)?;
    let model = descriptor_string(&desc, 16);
    let vendor = descriptor_string(&desc, 12);
    let mut health = match bus {
        DEVICE_BUS_NVME => nvme(device),
        DEVICE_BUS_ATA | DEVICE_BUS_SATA => ata(device),
        _ => None,
    }
    .unwrap_or_else(|| json!({"source":"Windows storage driver"}));
    if health["temperature"].is_null() {
        health["temperature"] = json!(reported_temperature(device));
    }
    health["id"] = json!(index.to_string());
    health["diskIndex"] = json!(index);
    health["name"] = json!(model.unwrap_or_else(|| format!("Physical drive {index}")));
    health["manufacturer"] = json!(vendor);
    health["bus"] = json!(bus_label(bus));
    let measured = [
        "temperature",
        "status",
        "wear",
        "hours",
        "startStop",
        "powerCycles",
    ]
    .iter()
    .any(|&key| !health[key].is_null());
    if !measured {
        health["unavailableReason"] =
            json!("This drive or controller did not expose health counters to Windows.");
    }
    Some((index, health))
}

pub fn drives() -> Result<Vec<Value>, String> {
    let mut found = BTreeMap::new();
    let mut problem = None;
    let mut paths = interfaces(&GUID_DEVINTERFACE_DISK)
        .unwrap_or_else(|error| {
            problem = Some(error.to_string());
            Vec::new()
        })
        .into_iter()
        .map(|path| (path, None))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        // Some restricted Windows sessions hide SetupAPI interfaces.
        paths.extend((0..8).map(|index| {
            (
                format!("\\\\.\\PhysicalDrive{index}")
                    .encode_utf16()
                    .chain([0])
                    .collect(),
                Some(index),
            )
        }));
    }
    for (path, fallback_index) in paths {
        match Device::open(&path, 0) {
            Ok(device) => {
                if let Some((index, mut row)) = read_drive(&device, fallback_index) {
                    if row["unavailableReason"].is_string() {
                        if let Ok(access) = Device::open(&path, GENERIC_READ) {
                            if let Some((_, candidate)) = read_drive(&access, Some(index)) {
                                row = candidate;
                            }
                        }
                    }
                    found.insert(index, row);
                }
            }
            Err(error) if !matches!(error.raw_os_error(), Some(2 | 3 | 1167)) => {
                problem = Some(error.to_string());
            }
            Err(_) => {}
        }
    }
    if found.is_empty() {
        if let Some(error) = problem {
            return Err(error);
        }
    }
    Ok(found.into_values().collect())
}

pub fn merge(native: Vec<Value>, wmi: &[Value]) -> Vec<Value> {
    let mut rows = native;
    for source in wmi {
        let Some(model) = source["name"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let matches = |item: &Value| {
            item["name"]
                .as_str()
                .is_some_and(|name| name.trim().eq_ignore_ascii_case(model))
        };
        let single_model = wmi.iter().filter(|item| matches(item)).count() == 1
            && rows.iter().filter(|item| matches(item)).count() == 1;
        if !single_model {
            rows.push(source.clone());
            continue;
        }
        let target = rows.iter_mut().find(|item| matches(item)).unwrap();
        let mut added = false;
        for key in [
            "status",
            "temperature",
            "wear",
            "hours",
            "startStop",
            "powerCycles",
            "readErrors",
            "writeErrors",
        ] {
            if !source[key].is_null() && target[key].is_null() {
                target[key] = source[key].clone();
                added = true;
            }
        }
        // Keep a more severe health warning reported by the Windows storage provider.
        if matches!(source["status"].as_str(), Some("Warning" | "Unhealthy"))
            && target["status"] == "Healthy"
        {
            target["status"] = source["status"].clone();
            added = true;
        }
        if added {
            target["source"] = json!(format!(
                "{} + storage reliability",
                target["source"].as_str().unwrap_or("Windows")
            ));
            target["unavailableReason"] = Value::Null;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nvme_offsets_and_counter_semantics() {
        let mut data = [0u8; 512];
        data[0] = 4;
        data[1..3].copy_from_slice(&313u16.to_le_bytes());
        data[5] = 31;
        data[112..128].copy_from_slice(&94u128.to_le_bytes());
        data[128..144].copy_from_slice(&1258u128.to_le_bytes());
        data[160..176].copy_from_slice(&19u128.to_le_bytes());
        let log = nvme_reading(&data).unwrap();
        assert_eq!(log["wear"], 31);
        assert_eq!(log["hours"], 1258);
        assert_eq!(log["powerCycles"], 94);
        assert_eq!(log["mediaErrors"], 19);
        assert_eq!(log["status"], "Warning");
        assert!(log["startStop"].is_null());
        assert!(log["readErrors"].is_null());
        assert!(log["writeErrors"].is_null());
        assert!((log["temperature"].as_f64().unwrap() - 39.85).abs() < 0.001);
        assert!(nvme_reading(&data[..300]).is_none());
    }
    #[test]
    fn ambiguous_storage_models_are_not_merged() {
        let native = vec![
            json!({"name":"disk","status":"Healthy"}),
            json!({"name":"disk","status":"Healthy"}),
        ];
        let rows = merge(native, &[json!({"name":"DISK","readErrors":4})]);
        assert!(rows[0]["readErrors"].is_null());
        assert_eq!(rows.len(), 3);
    }
}
